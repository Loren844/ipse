//! `ipse ui`: serves the interface on this computer and opens the browser on it
//! (decisions 0012 and 0013).
//!
//! The interface is written in Svelte, built into `ui/build/` and embedded in the
//! binary at compile time. The server around it only lets the person who ran the
//! command in:
//!
//! - It listens on `127.0.0.1` only, on a port the system picks at start.
//! - The browser is opened on `/login?code=…` with a one-time code, exchanged for a
//!   session cookie (`HttpOnly`, `SameSite=Strict`). Every other request without that
//!   cookie is refused, the interface's own files included.
//! - `Host` must name this computer and port, against DNS rebinding; `Origin` and
//!   `Sec-Fetch-Site`, when the browser sends them, must say the request comes from the
//!   interface itself, against other sites. A request that changes something must say
//!   so. `GET` never changes anything, but for exchanging the one-time code.
//! - The interface reads through the query layer with the owner's rights
//!   ([`Gateway::fresh_owner`]), and writes through the same paths as the command line.

use std::convert::Infallible;
use std::io::Write as _;
use std::net::{Ipv4Addr, SocketAddr};
use std::path::PathBuf;
use std::sync::{Arc, Mutex, PoisonError};

use http_body_util::combinators::BoxBody;
use http_body_util::{BodyExt as _, Full, Limited};
use hyper::body::{Bytes, Incoming};
use hyper::header::{self, HeaderMap, HeaderValue};
use hyper::{Method, Request, Response, StatusCode};
use hyper_util::rt::TokioIo;
use ipse_core::rights::Gateway;
use ipse_core::{Error, Result};
use serde::Serialize;

mod ais;
mod api;
mod review;

mod assets {
    include!(concat!(env!("OUT_DIR"), "/ui_assets.rs"));
}

type Body = BoxBody<Bytes, Infallible>;

/// The largest request body read.
const MAX_BODY: usize = 64 * 1024;

/// Who may use this server: the one-time code, then the sessions it opened.
struct Keys {
    /// BLAKE3 of the one-time code; `None` once it was used.
    code: Option<blake3::Hash>,
    /// BLAKE3 of each session's cookie value.
    sessions: Vec<blake3::Hash>,
}

/// One server: the vault, who may use it, where it listens.
struct State {
    gateway: Mutex<Gateway>,
    keys: Mutex<Keys>,
    port: u16,
}

/// Serves the interface of the memory in `vault` (the current folder by default) and
/// opens the browser on it, unless `open` is false: then it prints the address.
///
/// # Errors
///
/// Errors of [`Gateway::open`]; [`Error::Io`] when no port can be listened on.
pub(crate) fn run(vault: Option<PathBuf>, open: bool) -> Result<()> {
    let gateway = Gateway::open(&vault.unwrap_or_else(|| PathBuf::from(".")))?;
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .map_err(|e| Error::Refused(format!("Ipse could not start: {e}")))?;
    runtime.block_on(async move {
        let address = SocketAddr::from((Ipv4Addr::LOCALHOST, 0));
        let listener = tokio::net::TcpListener::bind(address)
            .await
            .map_err(|source| Error::Io {
                action: "listen on",
                path: PathBuf::from(address.to_string()),
                source,
            })?;
        let port = listener
            .local_addr()
            .map_err(|e| Error::Refused(format!("Ipse could not start: {e}")))?
            .port();
        let code = random()?;
        let state = Arc::new(State {
            gateway: Mutex::new(gateway),
            keys: Mutex::new(Keys {
                code: Some(blake3::hash(code.as_bytes())),
                sessions: Vec::new(),
            }),
            port,
        });
        let login = format!("http://127.0.0.1:{port}/login?code={code}");
        let said = if open && open_browser(&login) {
            format!("Ipse is open in your browser, at http://127.0.0.1:{port}/.")
        } else {
            format!("Open this address in your browser (it works once): {login}")
        };
        // Written without panicking: the terminal may be gone while the server runs on.
        let mut out = std::io::stdout();
        let _ = writeln!(out, "{said}");
        let _ = writeln!(
            out,
            "Keep this window open while you use Ipse; Ctrl+C closes it."
        );
        let _ = out.flush();

        loop {
            let Ok((stream, _)) = listener.accept().await else {
                continue;
            };
            let state = Arc::clone(&state);
            tokio::spawn(async move {
                let service = hyper::service::service_fn(move |request: Request<Incoming>| {
                    let state = Arc::clone(&state);
                    async move {
                        let (parts, body) = request.into_parts();
                        // A body is read whole, up to a size no form of the interface
                        // comes near.
                        let answer = match Limited::new(body, MAX_BODY).collect().await {
                            Ok(body) => {
                                answer(&Request::from_parts(parts, body.to_bytes()), &state)
                            }
                            Err(_) => refused(StatusCode::PAYLOAD_TOO_LARGE),
                        };
                        Ok::<_, Infallible>(secured(answer))
                    }
                });
                let _ = hyper::server::conn::http1::Builder::new()
                    .serve_connection(TokioIo::new(stream), service)
                    .await;
            });
        }
    })
}

/// 32 random bytes, in hex.
fn random() -> Result<String> {
    let mut bytes = [0_u8; 32];
    getrandom::fill(&mut bytes)
        .map_err(|_| Error::Refused("This computer could not make a random code.".to_owned()))?;
    Ok(blake3::Hash::from_bytes(bytes).to_hex().to_string())
}

/// Opens `url` in the person's browser. Returns whether it could.
fn open_browser(url: &str) -> bool {
    let mut command = if cfg!(target_os = "macos") {
        std::process::Command::new("open")
    } else if cfg!(windows) {
        let mut command = std::process::Command::new("rundll32");
        command.arg("url.dll,FileProtocolHandler");
        command
    } else {
        std::process::Command::new("xdg-open")
    };
    command
        .arg(url)
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .spawn()
        .is_ok()
}

/// One request: where it comes from, who sends it, then what it asks.
fn answer(request: &Request<Bytes>, state: &State) -> Response<Body> {
    let headers = request.headers();
    if !host_is_here(headers, state.port) {
        return refused(StatusCode::MISDIRECTED_REQUEST);
    }
    if !from_the_interface(request.method(), headers, state.port) {
        return refused(StatusCode::FORBIDDEN);
    }
    let path = request.uri().path();
    if path == "/login" {
        return login(request, state);
    }
    if !has_session(headers, state) {
        return if path.starts_with("/api/") {
            json_error(StatusCode::UNAUTHORIZED, "This session has ended.")
        } else {
            closed_page()
        };
    }
    if let Some(api) = path.strip_prefix("/api/") {
        return api_answer(request, api, state);
    }
    if request.method() != Method::GET && request.method() != Method::HEAD {
        return refused(StatusCode::METHOD_NOT_ALLOWED);
    }
    asset(path)
}

/// `Host` is this computer, at this port.
fn host_is_here(headers: &HeaderMap, port: u16) -> bool {
    let Some(host) = headers.get(header::HOST).and_then(|h| h.to_str().ok()) else {
        return false;
    };
    host == format!("127.0.0.1:{port}") || host == format!("localhost:{port}")
}

/// The request comes from the interface's own pages, or from the person opening it: a
/// browser's `Origin` and `Sec-Fetch-Site` say so when they are sent. A request that
/// changes something must send an `Origin` of this server.
fn from_the_interface(method: &Method, headers: &HeaderMap, port: u16) -> bool {
    let origin = headers.get(header::ORIGIN).and_then(|h| h.to_str().ok());
    let ours = |o: &str| {
        o == format!("http://127.0.0.1:{port}") || o == format!("http://localhost:{port}")
    };
    if origin.is_some_and(|o| !ours(o)) {
        return false;
    }
    let site = headers.get("sec-fetch-site").and_then(|h| h.to_str().ok());
    if site.is_some_and(|s| s != "same-origin" && s != "none") {
        return false;
    }
    let reads = *method == Method::GET || *method == Method::HEAD;
    reads || origin.is_some()
}

/// Exchanges the one-time code for a session cookie, once.
fn login(request: &Request<Bytes>, state: &State) -> Response<Body> {
    if request.method() != Method::GET {
        return refused(StatusCode::METHOD_NOT_ALLOWED);
    }
    let code = request
        .uri()
        .query()
        .unwrap_or_default()
        .split('&')
        .find_map(|pair| pair.strip_prefix("code="))
        .unwrap_or_default();
    let mut keys = state.keys.lock().unwrap_or_else(PoisonError::into_inner);
    // `blake3::Hash` compares in constant time.
    let valid = keys
        .code
        .is_some_and(|expected| !code.is_empty() && blake3::hash(code.as_bytes()) == expected);
    if !valid {
        return closed_page();
    }
    let Ok(session) = random() else {
        return refused(StatusCode::INTERNAL_SERVER_ERROR);
    };
    keys.code = None;
    keys.sessions.push(blake3::hash(session.as_bytes()));
    let mut response = Response::new(empty_body());
    *response.status_mut() = StatusCode::SEE_OTHER;
    let headers = response.headers_mut();
    headers.insert(header::LOCATION, HeaderValue::from_static("/"));
    if let Ok(cookie) = HeaderValue::from_str(&format!(
        "{}={session}; HttpOnly; SameSite=Strict; Path=/",
        cookie_name(state.port)
    )) {
        headers.insert(header::SET_COOKIE, cookie);
    }
    response
}

/// The cookie's name: one per server, as browsers share cookies between the ports of
/// one computer.
fn cookie_name(port: u16) -> String {
    format!("ipse_{port}")
}

/// The request carries the cookie of a session this server opened.
fn has_session(headers: &HeaderMap, state: &State) -> bool {
    let name = cookie_name(state.port);
    let keys = state.keys.lock().unwrap_or_else(PoisonError::into_inner);
    headers
        .get_all(header::COOKIE)
        .iter()
        .filter_map(|h| h.to_str().ok())
        .flat_map(|h| h.split(';'))
        .filter_map(|pair| pair.trim().split_once('='))
        .filter(|(key, _)| *key == name)
        .any(|(_, value)| {
            let presented = blake3::hash(value.as_bytes());
            // `blake3::Hash` compares in constant time.
            keys.sessions.contains(&presented)
        })
}

/// The JSON API, every answer read or written with the owner's rights. Reads are
/// `GET`; a change is a `POST` of JSON from the interface's own page.
fn api_answer(request: &Request<Bytes>, path: &str, state: &State) -> Response<Body> {
    let method = request.method();
    let query = request.uri().query().unwrap_or_default();
    let param = |name: &str| parameter(query, name);
    let mut gateway = state.gateway.lock().unwrap_or_else(PoisonError::into_inner);
    let gateway = &mut *gateway;
    let answer = if method == Method::GET {
        match path {
            "app" => api::app(gateway).map(|v| Some(to_json(&v))),
            "home" => api::home(gateway).map(|v| Some(to_json(&v))),
            "people" => {
                let offset = param("offset").and_then(|o| o.parse().ok()).unwrap_or(0);
                api::people(gateway, param("q").as_deref(), offset).map(|v| Some(to_json(&v)))
            }
            "events" => api::events(gateway, param("from").as_deref(), param("to").as_deref())
                .map(|v| Some(to_json(&v))),
            "proposals" => review::list(gateway).map(|v| Some(to_json(&v))),
            "ais" => ais::view(gateway).map(|v| Some(to_json(&v))),
            _ => match (path.strip_prefix("file/"), path.strip_prefix("proposals/")) {
                (Some(id), _) => api::file(gateway, id).map(|v| v.map(|v| to_json(&v))),
                (_, Some(id)) => review::one(gateway, id).map(|v| v.map(|v| to_json(&v))),
                _ => Ok(None),
            },
        }
    } else if method == Method::POST
        && (path.starts_with("proposals/")
            || path == "ais"
            || path.starts_with("ais/")
            || path == "categories")
    {
        let json = request
            .headers()
            .get(header::CONTENT_TYPE)
            .and_then(|h| h.to_str().ok())
            .is_some_and(|t| t.split(';').next() == Some("application/json"));
        if !json {
            return refused(StatusCode::UNSUPPORTED_MEDIA_TYPE);
        }
        if path.starts_with("proposals/") {
            review::decide(gateway, path, request.body()).map(|v| v.map(|v| to_json(&v)))
        } else {
            ais::change(gateway, path, request.body())
        }
    } else {
        return refused(StatusCode::METHOD_NOT_ALLOWED);
    };
    match answer {
        Ok(Some(body)) => json_body(StatusCode::OK, body),
        Ok(None) => json_error(StatusCode::NOT_FOUND, "There is nothing at this address."),
        // A change that no longer fits (already decided, stale) conflicts with the
        // memory as it is; a read asked wrongly is a bad request.
        Err(Error::Refused(message)) if method == Method::POST => {
            json_error(StatusCode::CONFLICT, &message)
        }
        Err(Error::Refused(message)) => json_error(StatusCode::BAD_REQUEST, &message),
        Err(error) => {
            // The details name files of the vault: they stay in this terminal.
            eprintln!("{error}");
            json_error(
                StatusCode::INTERNAL_SERVER_ERROR,
                "Ipse could not read or write the memory folder. The terminal says why.",
            )
        }
    }
}

/// The value of `name` in a query string, percent-decoded (`+` is a space).
fn parameter(query: &str, name: &str) -> Option<String> {
    query
        .split('&')
        .filter_map(|pair| pair.split_once('='))
        .find(|(key, _)| *key == name)
        .map(|(_, value)| decode(value))
}

fn decode(text: &str) -> String {
    let bytes = text.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        match bytes[i] {
            b'+' => out.push(b' '),
            b'%' if i + 2 < bytes.len() => {
                let hex = std::str::from_utf8(&bytes[i + 1..i + 3]).ok();
                match hex.and_then(|h| u8::from_str_radix(h, 16).ok()) {
                    Some(byte) => {
                        out.push(byte);
                        i += 2;
                    }
                    None => out.push(b'%'),
                }
            }
            byte => out.push(byte),
        }
        i += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

fn to_json(value: &impl Serialize) -> Vec<u8> {
    serde_json::to_vec(value).unwrap_or_default()
}

/// A file of the interface; any other page is the interface's own `index.html`, which
/// shows the right screen.
fn asset(path: &str) -> Response<Body> {
    let rel = path.trim_start_matches('/');
    let found = assets::ASSETS.iter().find(|(name, _, _)| *name == rel);
    let (name, content, media) = match found {
        Some(file) => *file,
        None if rel.starts_with("_app/") || rel.contains('.') => {
            return refused(StatusCode::NOT_FOUND);
        }
        None => match assets::ASSETS
            .iter()
            .find(|(name, _, _)| *name == "index.html")
        {
            Some(file) => *file,
            None => return refused(StatusCode::NOT_FOUND),
        },
    };
    let mut response = Response::new(Full::new(Bytes::from_static(content)).boxed());
    let headers = response.headers_mut();
    headers.insert(header::CONTENT_TYPE, HeaderValue::from_static(media));
    let cache = if name.starts_with("_app/immutable/") {
        "private, max-age=31536000, immutable"
    } else {
        "no-cache"
    };
    headers.insert(header::CACHE_CONTROL, HeaderValue::from_static(cache));
    response
}

/// The page shown without a session: how to open Ipse, in both languages.
fn closed_page() -> Response<Body> {
    const PAGE: &str = "<!doctype html><html lang=\"fr\"><head><meta charset=\"utf-8\">\
<meta name=\"viewport\" content=\"width=device-width, initial-scale=1\"><title>Ipse</title></head>\
<body><main><h1>Ipse</h1><p>Pour ouvrir votre mémoire, lancez <code>ipse ui</code> sur cet ordinateur.</p>\
<p lang=\"en\">To open your memory, run <code>ipse ui</code> on this computer.</p></main></body></html>";
    let mut response = Response::new(Full::new(Bytes::from_static(PAGE.as_bytes())).boxed());
    *response.status_mut() = StatusCode::FORBIDDEN;
    response.headers_mut().insert(
        header::CONTENT_TYPE,
        HeaderValue::from_static("text/html; charset=utf-8"),
    );
    response
}

fn json(status: StatusCode, value: &impl Serialize) -> Response<Body> {
    json_body(status, to_json(value))
}

fn json_body(status: StatusCode, body: Vec<u8>) -> Response<Body> {
    let mut response = Response::new(Full::new(Bytes::from(body)).boxed());
    *response.status_mut() = status;
    let headers = response.headers_mut();
    headers.insert(
        header::CONTENT_TYPE,
        HeaderValue::from_static("application/json"),
    );
    headers.insert(header::CACHE_CONTROL, HeaderValue::from_static("no-store"));
    response
}

fn json_error(status: StatusCode, message: &str) -> Response<Body> {
    json(status, &serde_json::json!({ "error": message }))
}

fn refused(status: StatusCode) -> Response<Body> {
    let mut response = Response::new(empty_body());
    *response.status_mut() = status;
    response
}

fn empty_body() -> Body {
    Full::new(Bytes::new()).boxed()
}

/// Headers every answer carries: what the page may load (nothing from the network),
/// and that it may not be framed or sniffed.
fn secured(mut response: Response<Body>) -> Response<Body> {
    let policy = format!(
        "default-src 'self'; script-src {}; style-src 'self' 'unsafe-inline'; img-src 'self' data:; \
         font-src 'self'; connect-src 'self'; object-src 'none'; base-uri 'none'; \
         form-action 'self'; frame-ancestors 'none'",
        assets::SCRIPT_SRC
    );
    let headers = response.headers_mut();
    if let Ok(policy) = HeaderValue::from_str(&policy) {
        headers.insert(header::CONTENT_SECURITY_POLICY, policy);
    }
    for (name, value) in [
        (header::X_CONTENT_TYPE_OPTIONS, "nosniff"),
        (header::REFERRER_POLICY, "no-referrer"),
        (header::X_FRAME_OPTIONS, "DENY"),
    ] {
        headers.insert(name, HeaderValue::from_static(value));
    }
    headers.insert(
        "cross-origin-opener-policy",
        HeaderValue::from_static("same-origin"),
    );
    headers.insert(
        "cross-origin-resource-policy",
        HeaderValue::from_static("same-origin"),
    );
    response
}

#[cfg(test)]
mod tests {
    use super::*;

    fn headers(pairs: &[(&str, &str)]) -> HeaderMap {
        let mut map = HeaderMap::new();
        for (name, value) in pairs {
            map.append(
                header::HeaderName::from_bytes(name.as_bytes()).unwrap(),
                HeaderValue::from_str(value).unwrap(),
            );
        }
        map
    }

    #[test]
    fn only_this_computer_and_port() {
        assert!(host_is_here(&headers(&[("host", "127.0.0.1:5000")]), 5000));
        assert!(host_is_here(&headers(&[("host", "localhost:5000")]), 5000));
        for host in [
            "127.0.0.1:5001",
            "evil.example:5000",
            "127.0.0.1",
            "0.0.0.0:5000",
        ] {
            assert!(!host_is_here(&headers(&[("host", host)]), 5000), "{host}");
        }
        assert!(!host_is_here(&HeaderMap::new(), 5000));
    }

    #[test]
    fn only_requests_from_the_interface() {
        let get = Method::GET;
        let post = Method::POST;
        assert!(from_the_interface(&get, &HeaderMap::new(), 5000));
        assert!(from_the_interface(
            &get,
            &headers(&[("sec-fetch-site", "none")]),
            5000
        ));
        assert!(from_the_interface(
            &post,
            &headers(&[
                ("origin", "http://127.0.0.1:5000"),
                ("sec-fetch-site", "same-origin")
            ]),
            5000
        ));
        for refused in [
            headers(&[("origin", "https://evil.example")]),
            headers(&[("origin", "http://127.0.0.1:3000")]),
            headers(&[("sec-fetch-site", "cross-site")]),
            headers(&[("sec-fetch-site", "same-site")]),
        ] {
            assert!(!from_the_interface(&get, &refused, 5000), "{refused:?}");
        }
        assert!(
            !from_the_interface(&post, &HeaderMap::new(), 5000),
            "a change says where it comes from"
        );
    }

    #[test]
    fn query_parameters_are_decoded() {
        assert_eq!(
            parameter("q=%C3%A9lodie+m&offset=10", "q").as_deref(),
            Some("élodie m")
        );
        assert_eq!(
            parameter("q=%C3%A9lodie+m&offset=10", "offset").as_deref(),
            Some("10")
        );
        assert_eq!(parameter("q=50%", "q").as_deref(), Some("50%"));
        assert_eq!(parameter("q=%zz", "q").as_deref(), Some("%zz"));
        assert_eq!(parameter("", "q"), None);
    }

    #[test]
    fn the_interface_is_embedded_with_its_script_hash() {
        assert!(
            assets::ASSETS
                .iter()
                .any(|(name, _, _)| *name == "index.html")
        );
        assert!(
            assets::SCRIPT_SRC.starts_with("'self' 'sha256-"),
            "{}",
            assets::SCRIPT_SRC
        );
    }
}
