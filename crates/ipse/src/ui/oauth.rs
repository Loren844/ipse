//! OAuth 2.1 for `ipse serve` (issue #270, decision 0020): claude.ai, on the web and
//! the phone, adds Ipse as a custom connector, and the person approves it on Ipse's own
//! page, behind the interface's access code.
//!
//! - **Discovery.** `/.well-known/oauth-protected-resource` (RFC 9728) names this
//!   server as the authorization server of `/mcp`; `/.well-known/oauth-authorization-
//!   server` (RFC 8414) gives its endpoints. A request to `/mcp` without a code is
//!   answered `401` with `WWW-Authenticate: Bearer resource_metadata="…"`.
//! - **Registration** (`/oauth/register`, RFC 7591): any app may register itself, as a
//!   public client, with `https` redirect addresses (or this computer's). It gains
//!   nothing by it: only the person can approve it. At most [`MAX_REGISTERED`] are
//!   kept, in `.ipse/oauth-clients.json`, the oldest dropped first.
//! - **Approval** (`/oauth/authorize`): a page of the interface, behind its session.
//!   The person names the AI and chooses the categories it reads, the sensitive ones
//!   closed unless they tick them. Approving gives a one-time code, valid
//!   [`GRANT_MINUTES`] minutes, bound to the app, its redirect address and its PKCE
//!   challenge (`S256` only).
//! - **Token** (`/oauth/token`): the code and its PKCE verifier make the connected AI,
//!   as `ipse client add` does, and its access code is the access token. It does not
//!   expire: disconnecting the AI in **Connected AIs** revokes it, from its next
//!   request.

use std::fmt::Write as _;
use std::path::Path;
use std::time::{Duration, Instant};

use http_body_util::{BodyExt as _, Full};
use hyper::body::Bytes;
use hyper::header::{self, HeaderMap, HeaderValue};
use hyper::{Method, Request, Response, StatusCode};
use ipse_core::rights::{Categories, Clients};
use ipse_core::vault::{DateTime, Language, write_atomic};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use sha2::{Digest as _, Sha256};

use super::{Body, State, decode, parameter};

/// Where the registered apps are kept, relative to the vault.
const REGISTERED: &str = ".ipse/oauth-clients.json";

/// Apps kept registered at most.
const MAX_REGISTERED: usize = 50;

/// How long an approval's one-time code can be exchanged.
const GRANT_MINUTES: u64 = 10;

/// The path of the approval page.
pub(super) const AUTHORIZE: &str = "/oauth/authorize";
const TOKEN: &str = "/oauth/token";
const REGISTER: &str = "/oauth/register";
const PROTECTED_RESOURCE: &str = "/.well-known/oauth-protected-resource";
const AUTHORIZATION_SERVER: &str = "/.well-known/oauth-authorization-server";

/// An approval waiting for its code to be exchanged.
#[derive(Debug, Clone)]
pub(super) struct Grant {
    /// The one-time code's hash.
    code: blake3::Hash,
    client_id: String,
    redirect_uri: String,
    /// The PKCE challenge: Base64url of the SHA-256 of the verifier.
    challenge: String,
    /// What the person chose: the AI's name, and the categories it reads.
    name: String,
    read: Vec<String>,
    given: Instant,
}

/// An app registered to ask for approval.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
struct Registered {
    client_id: String,
    client_name: String,
    redirect_uris: Vec<String>,
    /// When it registered, in seconds since 1970.
    issued_at: i64,
}

/// The paths answered to any app, without the interface's session: discovery,
/// registration, token.
pub(super) fn is_open(path: &str) -> bool {
    path == TOKEN
        || path == REGISTER
        || path == AUTHORIZATION_SERVER
        || path == "/.well-known/openid-configuration"
        || path == PROTECTED_RESOURCE
        || path.starts_with(&format!("{PROTECTED_RESOURCE}/"))
}

/// The address this server is reached at, as the app sees it: `--public-url` when
/// given, otherwise the `Host` (or `X-Forwarded-Host`) of the request, in `https` when
/// a proxy says so. `None` for a host that is not one.
pub(super) fn base(headers: &HeaderMap, public: Option<&str>) -> Option<String> {
    if let Some(public) = public {
        return Some(public.trim_end_matches('/').to_owned());
    }
    let first = |name: &str| {
        headers
            .get(name)
            .and_then(|h| h.to_str().ok())
            .and_then(|h| h.split(',').next())
            .map(str::trim)
            .filter(|h| !h.is_empty())
    };
    let host = first("x-forwarded-host").or_else(|| first("host"))?;
    let fine = host
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || "-.:[]".contains(c));
    if !fine {
        return None;
    }
    let scheme = match first("x-forwarded-proto") {
        Some(p) if p.eq_ignore_ascii_case("https") => "https",
        _ => "http",
    };
    Some(format!("{scheme}://{host}"))
}

/// Where `/mcp` says its metadata is, for the `401` of a request without a code.
pub(super) fn resource_metadata(base: &str) -> String {
    format!("{base}{PROTECTED_RESOURCE}/mcp")
}

/// Answers discovery, registration and token requests.
pub(super) fn open_answer(request: &Request<Bytes>, state: &State) -> Response<Body> {
    let Some(root) = state.serving.as_deref() else {
        return error(StatusCode::NOT_FOUND, "not_found", "");
    };
    let Some(base) = base(request.headers(), state.public_url.as_deref()) else {
        return error(StatusCode::BAD_REQUEST, "invalid_request", "No host.");
    };
    let path = request.uri().path();
    let method = request.method();
    match path {
        TOKEN if method == Method::POST => token(request, state, root),
        REGISTER if method == Method::POST => register(request.body(), root),
        TOKEN | REGISTER => error(StatusCode::METHOD_NOT_ALLOWED, "invalid_request", ""),
        _ if method != Method::GET && method != Method::HEAD => {
            error(StatusCode::METHOD_NOT_ALLOWED, "invalid_request", "")
        }
        AUTHORIZATION_SERVER | "/.well-known/openid-configuration" => {
            json_answer(StatusCode::OK, &server_metadata(&base))
        }
        _ => json_answer(
            StatusCode::OK,
            &json!({
                "resource": format!("{base}{}", crate::mcp::http::PATH),
                "authorization_servers": [base],
                "bearer_methods_supported": ["header"],
                "resource_name": "Ipse",
            }),
        ),
    }
}

fn server_metadata(base: &str) -> Value {
    json!({
        "issuer": base,
        "authorization_endpoint": format!("{base}{AUTHORIZE}"),
        "token_endpoint": format!("{base}{TOKEN}"),
        "registration_endpoint": format!("{base}{REGISTER}"),
        "response_types_supported": ["code"],
        "grant_types_supported": ["authorization_code"],
        "code_challenge_methods_supported": ["S256"],
        "token_endpoint_auth_methods_supported": ["none"],
    })
}

/// The body of a registration.
#[derive(Deserialize)]
struct Registration {
    #[serde(default)]
    redirect_uris: Vec<String>,
    #[serde(default)]
    client_name: Option<String>,
}

/// A redirect address Ipse sends a code to: `https`, or `http` on this computer, with
/// no fragment.
fn redirect_allowed(uri: &str) -> bool {
    let local = ["http://localhost", "http://127.0.0.1", "http://[::1]"]
        .iter()
        .any(|p| {
            uri.strip_prefix(p)
                .is_some_and(|r| r.is_empty() || r.starts_with([':', '/']))
        });
    (uri.starts_with("https://") || local)
        && uri.len() <= 512
        && !uri.contains('#')
        && !uri.chars().any(char::is_whitespace)
}

fn register(body: &[u8], root: &Path) -> Response<Body> {
    let Ok(asked) = serde_json::from_slice::<Registration>(body) else {
        return error(
            StatusCode::BAD_REQUEST,
            "invalid_client_metadata",
            "Not JSON.",
        );
    };
    if asked.redirect_uris.is_empty()
        || asked.redirect_uris.len() > 5
        || !asked.redirect_uris.iter().all(|u| redirect_allowed(u))
    {
        return error(
            StatusCode::BAD_REQUEST,
            "invalid_redirect_uri",
            "Redirect addresses must be https, or http on this computer.",
        );
    }
    let name: String = asked
        .client_name
        .as_deref()
        .map(str::trim)
        .filter(|n| !n.is_empty())
        .unwrap_or("AI app")
        .chars()
        .filter(|c| !c.is_control())
        .take(80)
        .collect();
    let Ok(id) = super::random() else {
        return error(StatusCode::INTERNAL_SERVER_ERROR, "server_error", "");
    };
    let now = jiff::Timestamp::now().as_second();
    let entry = Registered {
        client_id: format!("ipse-app-{}", &id[..24]),
        client_name: name,
        redirect_uris: asked.redirect_uris,
        issued_at: now,
    };
    let mut all = registered(root);
    all.push(entry.clone());
    if all.len() > MAX_REGISTERED {
        let extra = all.len() - MAX_REGISTERED;
        all.drain(..extra);
    }
    let Ok(text) = serde_json::to_vec_pretty(&all) else {
        return error(StatusCode::INTERNAL_SERVER_ERROR, "server_error", "");
    };
    if write_atomic(&root.join(REGISTERED), &text).is_err() {
        return error(StatusCode::INTERNAL_SERVER_ERROR, "server_error", "");
    }
    json_answer(
        StatusCode::CREATED,
        &json!({
            "client_id": entry.client_id,
            "client_name": entry.client_name,
            "redirect_uris": entry.redirect_uris,
            "client_id_issued_at": now,
            "token_endpoint_auth_method": "none",
            "grant_types": ["authorization_code"],
            "response_types": ["code"],
        }),
    )
}

fn registered(root: &Path) -> Vec<Registered> {
    std::fs::read(root.join(REGISTERED))
        .ok()
        .and_then(|bytes| serde_json::from_slice(&bytes).ok())
        .unwrap_or_default()
}

fn find(root: &Path, client_id: &str) -> Option<Registered> {
    registered(root)
        .into_iter()
        .find(|r| r.client_id == client_id)
}

/// The parameters of an authorization request, from the query or the approval form.
#[derive(Debug, Clone, PartialEq, Eq)]
struct Asked {
    client_id: String,
    redirect_uri: String,
    state: Option<String>,
    challenge: String,
}

/// What is wrong with an authorization request: shown to the person when the app
/// cannot be told (no known app or redirect address), otherwise sent back to it.
enum Wrong {
    Shown(&'static str),
    Sent(String),
}

fn asked(text: &str, root: &Path) -> Result<(Asked, Registered), Wrong> {
    let get = |name| parameter(text, name).filter(|v| !v.is_empty());
    let client_id = get("client_id").ok_or(Wrong::Shown("No client_id."))?;
    let app = find(root, &client_id).ok_or(Wrong::Shown(
        "This app is not registered with Ipse: add the connector again.",
    ))?;
    let redirect_uri = match get("redirect_uri") {
        Some(uri) if app.redirect_uris.contains(&uri) => uri,
        None if app.redirect_uris.len() == 1 => app.redirect_uris[0].clone(),
        _ => return Err(Wrong::Shown("This redirect address was not registered.")),
    };
    let state = get("state");
    let back = |error| {
        Wrong::Sent(redirect(
            &redirect_uri,
            &[("error", error)],
            state.as_deref(),
        ))
    };
    if get("response_type").as_deref() != Some("code") {
        return Err(back("unsupported_response_type"));
    }
    let challenge = match (
        get("code_challenge"),
        get("code_challenge_method").as_deref(),
    ) {
        (Some(c), Some("S256")) if (43..=128).contains(&c.len()) => c,
        _ => return Err(back("invalid_request")),
    };
    Ok((
        Asked {
            client_id,
            redirect_uri: redirect_uri.clone(),
            state,
            challenge,
        },
        app,
    ))
}

/// `uri` with `pairs` and the `state` added to its query.
fn redirect(uri: &str, pairs: &[(&str, &str)], state: Option<&str>) -> String {
    let mut out = uri.to_owned();
    let mut sep = if uri.contains('?') { '&' } else { '?' };
    for (key, value) in pairs.iter().copied().chain(state.map(|s| ("state", s))) {
        out.push(sep);
        out.push_str(key);
        out.push('=');
        out.push_str(&encode(value));
        sep = '&';
    }
    out
}

/// Percent-encodes everything but unreserved characters.
fn encode(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for byte in text.bytes() {
        if byte.is_ascii_alphanumeric() || b"-._~".contains(&byte) {
            out.push(char::from(byte));
        } else {
            let _ = write!(out, "%{byte:02X}");
        }
    }
    out
}

/// A `GET /oauth/authorize` the browser makes without the session's cookie: it was
/// sent from another site, where a `SameSite=Strict` cookie is not sent. Once
/// reloaded from Ipse's own page, it is; still without one, the person logs in first.
pub(super) fn without_session(request: &Request<Bytes>) -> Response<Body> {
    let query = request.uri().query().unwrap_or_default();
    if parameter(query, "again").is_some() {
        let next = format!("{AUTHORIZE}?{query}");
        return see_other(&format!("/login?next={}", encode(&next)));
    }
    let again = format!("{AUTHORIZE}?{query}&again=1");
    let html = format!(
        "<!doctype html><html><head><meta charset=\"utf-8\">\
<meta http-equiv=\"refresh\" content=\"0;url={url}\"><title>Ipse</title></head>\
<body><p><a href=\"{url}\">Ipse</a></p></body></html>",
        url = attribute(&again)
    );
    page_answer(StatusCode::OK, html, None)
}

/// The approval page (`GET`), and the person's answer (`POST`), behind the session.
pub(super) fn authorize(request: &Request<Bytes>, state: &State) -> Response<Body> {
    let Some(root) = state.serving.as_deref() else {
        return super::refused(StatusCode::NOT_FOUND);
    };
    let language = state
        .gateway
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
        .settings()
        .language();
    let fr = language == Language::Fr;
    let text = if request.method() == Method::POST {
        String::from_utf8_lossy(request.body()).into_owned()
    } else {
        request.uri().query().unwrap_or_default().to_owned()
    };
    let (asked, app) = match asked(&text, root) {
        Ok(found) => found,
        Err(Wrong::Shown(message)) => {
            return page_answer(StatusCode::BAD_REQUEST, message_page(message, fr), None);
        }
        Err(Wrong::Sent(to)) => return see_other(&to),
    };
    match *request.method() {
        Method::GET | Method::HEAD => approval_page(&asked, &app, state, fr),
        Method::POST => {
            let issuer = base(request.headers(), state.public_url.as_deref());
            decided(&text, asked, state, issuer.as_deref())
        }
        _ => super::refused(StatusCode::METHOD_NOT_ALLOWED),
    }
}

/// The person's answer: refused, or approved with a name and categories.
fn decided(text: &str, asked: Asked, state: &State, issuer: Option<&str>) -> Response<Body> {
    if parameter(text, "decision").as_deref() != Some("approve") {
        return see_other(&redirect(
            &asked.redirect_uri,
            &[("error", "access_denied")],
            asked.state.as_deref(),
        ));
    }
    let categories = {
        let gateway = state
            .gateway
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        Categories::of(gateway.settings())
    };
    let read: Vec<String> = text
        .split('&')
        .filter_map(|pair| pair.split_once('='))
        .filter(|(key, _)| *key == "read")
        .map(|(_, value)| decode(value))
        .filter(|c| categories.known(c))
        .collect();
    let name = parameter(text, "name")
        .map(|n| n.trim().to_owned())
        .filter(|n| !n.is_empty())
        .unwrap_or_else(|| "AI".to_owned());
    let Ok(code) = super::random() else {
        return super::refused(StatusCode::INTERNAL_SERVER_ERROR);
    };
    let mut grants = state
        .grants
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    grants.retain(|g| g.given.elapsed() < Duration::from_mins(GRANT_MINUTES));
    grants.push(Grant {
        code: blake3::hash(code.as_bytes()),
        client_id: asked.client_id,
        redirect_uri: asked.redirect_uri.clone(),
        challenge: asked.challenge,
        name,
        read,
        given: Instant::now(),
    });
    let mut pairs = vec![("code", code.as_str())];
    if let Some(issuer) = issuer {
        pairs.push(("iss", issuer));
    }
    see_other(&redirect(
        &asked.redirect_uri,
        &pairs,
        asked.state.as_deref(),
    ))
}

/// The code and its verifier exchanged for the access token: the AI is connected now.
fn token(request: &Request<Bytes>, state: &State, root: &Path) -> Response<Body> {
    let body = String::from_utf8_lossy(request.body()).into_owned();
    let get = |name| parameter(&body, name).filter(|v| !v.is_empty());
    if get("grant_type").as_deref() != Some("authorization_code") {
        return error(StatusCode::BAD_REQUEST, "unsupported_grant_type", "");
    }
    let (Some(code), Some(verifier)) = (get("code"), get("code_verifier")) else {
        return error(
            StatusCode::BAD_REQUEST,
            "invalid_request",
            "A code and a code_verifier.",
        );
    };
    let presented = blake3::hash(code.as_bytes());
    let grant = {
        let mut grants = state
            .grants
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        grants.retain(|g| g.given.elapsed() < Duration::from_mins(GRANT_MINUTES));
        // Used once, right or wrong.
        let at = grants.iter().position(|g| g.code == presented);
        at.map(|at| grants.remove(at))
    };
    let Some(grant) = grant else {
        return error(
            StatusCode::BAD_REQUEST,
            "invalid_grant",
            "Unknown or expired code.",
        );
    };
    let same_app = get("client_id").is_none_or(|id| id == grant.client_id);
    let same_redirect = get("redirect_uri").is_none_or(|uri| uri == grant.redirect_uri);
    if !same_app || !same_redirect || challenge_of(&verifier) != grant.challenge {
        return error(StatusCode::BAD_REQUEST, "invalid_grant", "");
    }
    let connected = (|| {
        let gateway = state
            .gateway
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let categories = Categories::of(gateway.settings());
        let mut clients = Clients::load(root)?;
        let (client, token) = clients.add(
            &grant.name,
            DateTime::now(gateway.settings().time_zone()),
            &categories,
            false,
        )?;
        clients.set_rights(&client.id, Some(grant.read.clone()), None, &categories)?;
        Ok::<_, ipse_core::Error>(token)
    })();
    match connected {
        Ok(token) => json_answer(
            StatusCode::OK,
            &json!({ "access_token": token.expose(), "token_type": "Bearer" }),
        ),
        Err(e) => {
            eprintln!("An AI approved over OAuth could not be connected: {e}");
            error(StatusCode::INTERNAL_SERVER_ERROR, "server_error", "")
        }
    }
}

/// The PKCE `S256` challenge of `verifier`: Base64url, without padding, of its SHA-256.
fn challenge_of(verifier: &str) -> String {
    base64url(&Sha256::digest(verifier.as_bytes()))
}

fn base64url(bytes: &[u8]) -> String {
    const ALPHABET: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789-_";
    let mut out = String::with_capacity(bytes.len().div_ceil(3) * 4);
    for chunk in bytes.chunks(3) {
        let b = [
            chunk[0],
            chunk.get(1).copied().unwrap_or(0),
            chunk.get(2).copied().unwrap_or(0),
        ];
        let n = (u32::from(b[0]) << 16) | (u32::from(b[1]) << 8) | u32::from(b[2]);
        for (i, shift) in [18, 12, 6, 0].into_iter().enumerate() {
            if i <= chunk.len() {
                out.push(char::from(ALPHABET[(n >> shift) as usize & 63]));
            }
        }
    }
    out
}

/// `text` safe inside an HTML attribute or element.
fn attribute(text: &str) -> String {
    text.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&#39;")
}

/// A category's name for people.
fn category_label(name: &str, fr: bool) -> String {
    let words = match name {
        "identity" => ("Identité", "Identity"),
        "people" => ("Personnes", "People"),
        "agenda" => ("Agenda", "Agenda"),
        "places" => ("Lieux", "Places"),
        "media" => ("Médias", "Media"),
        "work" => ("Travail", "Work"),
        "things" => ("Objets", "Things"),
        "summaries" => ("Résumés", "Summaries"),
        "location" => ("Position", "Location"),
        "home" => ("Maison", "Home"),
        "health" => ("Santé", "Health"),
        "private" => ("Privé", "Private"),
        other => return other.to_owned(),
    };
    if fr { words.0 } else { words.1 }.to_owned()
}

/// The categories to tick, those a new AI reads ticked, the sensitive ones said.
fn category_boxes(state: &State, fr: bool) -> String {
    let (categories, labels) = {
        let gateway = state
            .gateway
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let settings = gateway.settings();
        let labels: Vec<(String, String)> = settings
            .categories()
            .iter()
            .filter_map(|(name, c)| c.label.clone().map(|l| (name.clone(), l)))
            .collect();
        (Categories::of(settings), labels)
    };
    let open = categories.default_read();
    let sensitive = categories.sensitive();
    let mut boxes = String::new();
    for category in categories.all() {
        let label = labels
            .iter()
            .find(|(n, _)| n == category)
            .map_or_else(|| category_label(category, fr), |(_, l)| l.clone());
        let tag = if sensitive.contains(&category) {
            if fr {
                " <em>sensible</em>"
            } else {
                " <em>sensitive</em>"
            }
        } else {
            ""
        };
        let _ = write!(
            boxes,
            "<label class=\"box\"><input type=\"checkbox\" name=\"read\" value=\"{value}\"{checked}> {label}{tag}</label>",
            value = attribute(category),
            checked = if open.iter().any(|o| o == category) {
                " checked"
            } else {
                ""
            },
            label = attribute(&label),
        );
    }
    boxes
}

fn approval_page(asked: &Asked, app: &Registered, state: &State, fr: bool) -> Response<Body> {
    let to = host_of(&asked.redirect_uri);
    let name = attribute(&app.client_name);
    let boxes = category_boxes(state, fr);
    let hidden: String = [
        ("response_type", Some("code")),
        ("client_id", Some(asked.client_id.as_str())),
        ("redirect_uri", Some(asked.redirect_uri.as_str())),
        ("state", asked.state.as_deref()),
        ("code_challenge", Some(asked.challenge.as_str())),
        ("code_challenge_method", Some("S256")),
    ]
    .iter()
    .filter_map(|(k, v)| {
        v.map(|v| {
            format!(
                "<input type=\"hidden\" name=\"{k}\" value=\"{}\">",
                attribute(v)
            )
        })
    })
    .collect();
    let words = if fr {
        [
            format!("Connecter {name} à votre mémoire\u{a0}?"),
            format!(
                "{name}, depuis <b>{}</b>, demande à lire votre mémoire à travers Internet. Elle \
verra les catégories cochées ci-dessous, et rien d’autre\u{a0}; elle ne pourra rien écrire sans \
votre accord. Vous pourrez changer ses droits ou la déconnecter à tout moment dans IA \
connectées.",
                attribute(&to)
            ),
            "Son nom".to_owned(),
            "Ce qu’elle peut lire".to_owned(),
            "Les catégories sensibles restent fermées si vous ne les cochez pas.".to_owned(),
            "Connecter".to_owned(),
            "Refuser".to_owned(),
        ]
    } else {
        [
            format!("Connect {name} to your memory?"),
            format!(
                "{name}, from <b>{}</b>, asks to read your memory over the internet. It will see \
the categories ticked below, and nothing else; it cannot write anything without your OK. You \
can change its rights or disconnect it at any time in Connected AIs.",
                attribute(&to)
            ),
            "Its name".to_owned(),
            "What it may read".to_owned(),
            "Sensitive categories stay closed unless you tick them.".to_owned(),
            "Connect".to_owned(),
            "Refuse".to_owned(),
        ]
    };
    let [
        title,
        lead,
        name_label,
        read_label,
        sensitive_hint,
        approve,
        refuse,
    ] = words;
    let html = format!(
        "<!doctype html><html lang=\"{lang}\"><head><meta charset=\"utf-8\">\
<meta name=\"viewport\" content=\"width=device-width, initial-scale=1\"><title>Ipse</title>\
<style>{STYLE}</style></head><body><main><h1>{title}</h1><p>{lead}</p>\
<form method=\"post\" action=\"{AUTHORIZE}\">{hidden}\
<label for=\"name\">{name_label}</label><input id=\"name\" name=\"name\" type=\"text\" maxlength=\"80\" required value=\"{name}\">\
<fieldset><legend>{read_label}</legend>{boxes}<p class=\"hint\">{sensitive_hint}</p></fieldset>\
<div class=\"actions\"><button type=\"submit\" name=\"decision\" value=\"approve\">{approve}</button>\
<button type=\"submit\" name=\"decision\" value=\"refuse\" class=\"quiet\">{refuse}</button></div>\
</form></main></body></html>",
        lang = if fr { "fr" } else { "en" },
    );
    page_answer(
        StatusCode::OK,
        html,
        origin_of(&asked.redirect_uri).as_deref(),
    )
}

const STYLE: &str = "body{font-family:system-ui,sans-serif;margin:0;background:#f7f6f3;color:#1d1c1a}\
main{max-width:30rem;margin:8vh auto;padding:0 16px}h1{font-weight:600;font-size:1.5rem}\
label{display:block;margin:1.2rem 0 .4rem;font-weight:600}input[type=text]{width:100%;box-sizing:border-box;\
padding:.6rem;font:inherit;border:1px solid #bbb;border-radius:6px}fieldset{border:0;padding:0;margin:1.2rem 0}\
legend{font-weight:600;margin-bottom:.4rem}label.box{font-weight:400;margin:.3rem 0}em{font-style:normal;\
font-size:.85em;color:#8a5a00;margin-left:.3rem}.hint{font-size:.9em;color:#5d5a55}.actions{display:flex;\
gap:.6rem;flex-wrap:wrap;margin-top:1rem}button{padding:.6rem 1.2rem;font:inherit;border:0;border-radius:6px;\
background:#1d1c1a;color:#fff}button.quiet{background:transparent;color:inherit;border:1px solid #bbb}\
@media (prefers-color-scheme:dark){body{background:#1b1a18;color:#eceae5}input[type=text]{background:#262522;\
color:inherit;border-color:#555}button{background:#eceae5;color:#1b1a18}button.quiet{color:inherit;\
border-color:#555}em{color:#e0b060}.hint{color:#aaa59c}}";

fn message_page(message: &str, fr: bool) -> String {
    let title = if fr {
        "Connexion impossible"
    } else {
        "Cannot connect"
    };
    format!(
        "<!doctype html><html><head><meta charset=\"utf-8\">\
<meta name=\"viewport\" content=\"width=device-width, initial-scale=1\"><title>Ipse</title>\
<style>{STYLE}</style></head><body><main><h1>{title}</h1><p lang=\"en\">{}</p></main></body></html>",
        attribute(message)
    )
}

/// The host of `uri`, for people: `claude.ai`.
fn host_of(uri: &str) -> String {
    let rest = uri.split_once("://").map_or(uri, |(_, r)| r);
    rest.split(['/', '?']).next().unwrap_or(rest).to_owned()
}

/// `https://claude.ai` of `https://claude.ai/api/mcp/auth_callback`.
fn origin_of(uri: &str) -> Option<String> {
    let (scheme, _) = uri.split_once("://")?;
    Some(format!("{scheme}://{}", host_of(uri)))
}

/// A page of its own: no script, and its form may go to `send_to` (the app's
/// redirect address, where approving sends the browser).
fn page_answer(status: StatusCode, html: String, send_to: Option<&str>) -> Response<Body> {
    let mut response = Response::new(Full::new(Bytes::from(html)).boxed());
    *response.status_mut() = status;
    let headers = response.headers_mut();
    headers.insert(
        header::CONTENT_TYPE,
        HeaderValue::from_static("text/html; charset=utf-8"),
    );
    headers.insert(header::CACHE_CONTROL, HeaderValue::from_static("no-store"));
    let policy = format!(
        "default-src 'none'; style-src 'unsafe-inline'; base-uri 'none'; frame-ancestors 'none'; \
         form-action 'self'{}",
        send_to.map(|o| format!(" {o}")).unwrap_or_default()
    );
    if let Ok(policy) = HeaderValue::from_str(&policy) {
        headers.insert(header::CONTENT_SECURITY_POLICY, policy);
    }
    // Its form must say where it comes from (`Origin`), which `no-referrer` hides; the
    // app's site is told nothing of this page.
    headers.insert(
        header::REFERRER_POLICY,
        HeaderValue::from_static("same-origin"),
    );
    response
}

fn see_other(location: &str) -> Response<Body> {
    let mut response = Response::new(super::empty_body());
    *response.status_mut() = StatusCode::SEE_OTHER;
    if let Ok(value) = HeaderValue::from_str(location) {
        response.headers_mut().insert(header::LOCATION, value);
    }
    response
        .headers_mut()
        .insert(header::CACHE_CONTROL, HeaderValue::from_static("no-store"));
    response
}

fn json_answer(status: StatusCode, value: &Value) -> Response<Body> {
    let mut response = Response::new(Full::new(Bytes::from(value.to_string())).boxed());
    *response.status_mut() = status;
    let headers = response.headers_mut();
    headers.insert(
        header::CONTENT_TYPE,
        HeaderValue::from_static("application/json"),
    );
    headers.insert(header::CACHE_CONTROL, HeaderValue::from_static("no-store"));
    response
}

fn error(status: StatusCode, code: &str, description: &str) -> Response<Body> {
    let mut value = json!({ "error": code });
    if !description.is_empty() {
        value["error_description"] = Value::from(description);
    }
    json_answer(status, &value)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pkce_s256_is_base64url_of_sha256() {
        // SHA-256 of "abc" (FIPS 180-2), in Base64url without padding.
        assert_eq!(
            challenge_of("abc"),
            "ungWv48Bz-pBQUDeXa4iI7ADYaOWF3qctBD_YfIAFa0"
        );
        assert_eq!(challenge_of("").len(), 43);
    }

    #[test]
    fn redirect_addresses_are_https_or_this_computer() {
        assert!(redirect_allowed("https://claude.ai/api/mcp/auth_callback"));
        assert!(redirect_allowed("http://localhost:6274/oauth/callback"));
        assert!(redirect_allowed("http://127.0.0.1/cb"));
        for wrong in [
            "http://claude.ai/cb",
            "http://localhost.evil.example/cb",
            "https://claude.ai/cb#frag",
            "javascript:alert(1)",
            "https://claude.ai/a b",
        ] {
            assert!(!redirect_allowed(wrong), "{wrong}");
        }
    }

    #[test]
    fn the_address_seen_by_the_app() {
        let mut headers = HeaderMap::new();
        headers.insert(
            header::HOST,
            HeaderValue::from_static("nas.tail1234.ts.net"),
        );
        assert_eq!(base(&headers, None).unwrap(), "http://nas.tail1234.ts.net");
        headers.insert("x-forwarded-proto", HeaderValue::from_static("https"));
        assert_eq!(base(&headers, None).unwrap(), "https://nas.tail1234.ts.net");
        assert_eq!(
            base(&headers, Some("https://ipse.example.org/")).unwrap(),
            "https://ipse.example.org"
        );
        headers.insert(header::HOST, HeaderValue::from_static("evil\"host"));
        assert_eq!(base(&headers, None), None);
    }

    #[test]
    fn redirects_carry_the_state() {
        assert_eq!(
            redirect("https://claude.ai/cb", &[("code", "a b")], Some("x&y")),
            "https://claude.ai/cb?code=a%20b&state=x%26y"
        );
        assert_eq!(
            redirect("https://app/cb?k=1", &[("error", "access_denied")], None),
            "https://app/cb?k=1&error=access_denied"
        );
        assert_eq!(
            origin_of("https://claude.ai/api/mcp/auth_callback").unwrap(),
            "https://claude.ai"
        );
    }
}
