//! `ipse serve`: the memory on a home server (decision 0019). One process, on one port:
//! the interface at `/`, MCP at `/mcp`, and the background sync of the followed sources.
//!
//! - It listens on `127.0.0.1` unless `--listen` gives another address.
//! - The interface asks for its own access code (`ipse access-code`) on a login page,
//!   exchanged for a session cookie; MCP asks each AI for its code, as `ipse mcp --http`.
//! - Any `Host` is taken, as the person's own address for the server is not known; a
//!   browser's `Origin` must be the address the request was sent to.
//! - HTTPS is the person's reverse proxy's (`docs/guide/self-hosting.md`).

use std::io::Write as _;
use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use http_body_util::{BodyExt as _, Full};
use hyper::body::Bytes;
use hyper::header::{self, HeaderMap, HeaderValue};
use hyper::{Method, Request, Response, StatusCode};
use ipse_core::rights::Gateway;
use ipse_core::{Error, Result};
use rmcp::transport::streamable_http_server::StreamableHttpService;
use rmcp::transport::streamable_http_server::session::never::NeverSessionManager;

use super::{Body, Keys, SESSION_DAYS, Session, State, accept, background, cookie_name, random};
use crate::cmd::access;
use crate::mcp::Server;
use crate::mcp::http::{DEFAULT_PORT, PATH, service};

/// MCP, served beside the interface.
#[derive(Clone)]
pub(crate) struct Mcp {
    pub(super) service: StreamableHttpService<Server, NeverSessionManager>,
    pub(super) gateway: Arc<Mutex<Gateway>>,
}

/// The address to listen on: a port alone is on this computer (`127.0.0.1`); any
/// other address is given whole, like `0.0.0.0:7420`.
///
/// # Errors
///
/// [`Error::Refused`] for text that is not a port or an address.
pub(crate) fn address(text: &str) -> Result<SocketAddr> {
    let text = text.trim();
    if let Ok(port) = text.parse::<u16>() {
        return Ok(SocketAddr::from((Ipv4Addr::LOCALHOST, port)));
    }
    let text = text.replace("localhost:", "127.0.0.1:");
    text.parse().map_err(|_| {
        Error::Refused(format!(
            "--listen takes a port, like {DEFAULT_PORT}, or an address and a port, like \
             0.0.0.0:{DEFAULT_PORT} to be reached from your network."
        ))
    })
}

/// Serves the memory in `vault` (the current folder by default) at `listen` (this
/// computer, port 7420, by default), until the process is stopped.
///
/// # Errors
///
/// Errors of [`Gateway::open`]; [`Error::Io`] when the address cannot be listened on.
pub(crate) fn run(vault: Option<PathBuf>, listen: Option<&str>) -> Result<()> {
    let root = vault.unwrap_or_else(|| PathBuf::from("."));
    let address = match listen {
        Some(text) => address(text)?,
        None => SocketAddr::from((Ipv4Addr::LOCALHOST, DEFAULT_PORT)),
    };
    let gateway = Gateway::open(&root)?;
    let root = gateway.vault().root().to_path_buf();
    let for_mcp = Arc::new(Mutex::new(Gateway::open(&root)?));
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .map_err(|e| Error::Refused(format!("Ipse could not start: {e}")))?;
    runtime.block_on(async move {
        let listener = tokio::net::TcpListener::bind(address)
            .await
            .map_err(|source| Error::Io {
                action: "listen on",
                path: PathBuf::from(address.to_string()),
                source,
            })?;
        let local = listener
            .local_addr()
            .map_err(|e| Error::Refused(format!("Ipse could not start: {e}")))?;
        let (wake, wakes) = std::sync::mpsc::channel();
        let state = Arc::new(State {
            gateway: Mutex::new(gateway),
            keys: Mutex::new(Keys {
                code: None,
                sessions: Vec::new(),
            }),
            port: local.port(),
            app: false,
            wake: Some(wake),
            serving: Some(root.clone()),
        });
        let mcp = Mcp {
            // Any `Host`, as for the interface; no browser.
            service: service(&for_mcp, true),
            gateway: for_mcp,
        };
        say(&started(local, access::current(&root).is_some()));
        background::start(&state, wakes);
        accept(listener, state, Some(mcp)).await
    })
}

/// What `ipse serve` says when it starts.
fn started(local: SocketAddr, has_code: bool) -> String {
    let shown = match local.ip() {
        IpAddr::V4(ip) if ip.is_unspecified() => format!("port {}", local.port()),
        _ => format!("http://{local}"),
    };
    let mut said = format!(
        "Ipse serves your memory at {shown}: the interface at /, AI apps at {PATH} with their \
         access code. The followed sources sync in the background."
    );
    if local.ip().is_loopback() {
        said.push_str(
            " Only this computer can reach it; --listen 0.0.0.0:7420 opens it to your network.",
        );
    } else {
        said.push_str(
            "\nYour memory can be reached from the network, by anyone with the interface's \
             code or an AI's code. Put it behind a reverse proxy with HTTPS \
             (docs/guide/self-hosting.md): over plain HTTP, the codes cross the network readable.",
        );
    }
    if !has_code {
        said.push_str(
            "\nThe interface has no access code yet: run `ipse access-code` (with Docker: \
             `docker compose exec ipse ipse access-code`).",
        );
    }
    said
}

fn say(text: &str) {
    // Written without panicking: the output may be gone while the server runs on.
    let mut out = std::io::stdout();
    let _ = writeln!(out, "{text}");
    let _ = out.flush();
}

/// The request comes from the interface's own pages, those of the address it names in
/// `Host`, or from the person opening it.
pub(super) fn from_its_pages(method: &Method, headers: &HeaderMap) -> bool {
    let Some(host) = headers
        .get(header::HOST)
        .and_then(|h| h.to_str().ok())
        .filter(|h| !h.is_empty())
    else {
        return false;
    };
    super::same_origin(method, headers, |origin| {
        origin == format!("http://{host}") || origin == format!("https://{host}")
    })
}

/// Sends a page asked without a session to the login page.
pub(super) fn to_login() -> Response<Body> {
    let mut response = Response::new(super::empty_body());
    *response.status_mut() = StatusCode::SEE_OTHER;
    response
        .headers_mut()
        .insert(header::LOCATION, HeaderValue::from_static("/login"));
    response
}

/// What the login page says.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Said {
    /// Asks for the code.
    Ask,
    /// The code entered is not the interface's.
    Wrong,
    /// No code was made yet.
    NoCode,
}

/// The login page, and the code entered on it exchanged for a session cookie.
pub(super) fn login(request: &Request<Bytes>, state: &State) -> Response<Body> {
    let Some(root) = state.serving.as_deref() else {
        return super::refused(StatusCode::NOT_FOUND);
    };
    if access::current(root).is_none() {
        return page(Said::NoCode, StatusCode::OK);
    }
    match *request.method() {
        Method::GET | Method::HEAD => return page(Said::Ask, StatusCode::OK),
        Method::POST => {}
        _ => return super::refused(StatusCode::METHOD_NOT_ALLOWED),
    }
    let body = String::from_utf8_lossy(request.body());
    let code = super::parameter(&body, "code").unwrap_or_default();
    let Some(code) = access::check(root, &code) else {
        return page(Said::Wrong, StatusCode::UNAUTHORIZED);
    };
    let Ok(session) = random() else {
        return super::refused(StatusCode::INTERNAL_SERVER_ERROR);
    };
    state
        .keys
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
        .sessions
        .push(Session {
            key: blake3::hash(session.as_bytes()),
            code: Some(code),
            opened: std::time::Instant::now(),
        });
    let mut response = Response::new(super::empty_body());
    *response.status_mut() = StatusCode::SEE_OTHER;
    let headers = response.headers_mut();
    headers.insert(header::LOCATION, HeaderValue::from_static("/"));
    let secure = request
        .headers()
        .get("x-forwarded-proto")
        .and_then(|h| h.to_str().ok())
        .is_some_and(|p| p.trim().eq_ignore_ascii_case("https"));
    if let Ok(cookie) = HeaderValue::from_str(&format!(
        "{}={session}; HttpOnly; SameSite=Strict; Path=/; Max-Age={}{}",
        cookie_name(state.port),
        SESSION_DAYS * 24 * 3600,
        if secure { "; Secure" } else { "" }
    )) {
        headers.insert(header::SET_COOKIE, cookie);
    }
    response
}

/// The login page, in both languages.
fn page(said: Said, status: StatusCode) -> Response<Body> {
    const STYLE: &str = "body{font-family:system-ui,sans-serif;margin:0;background:#f7f6f3;color:#1d1c1a}\
main{max-width:26rem;margin:12vh auto;padding:0 16px}h1{font-weight:600}\
label{display:block;margin:1.5rem 0 .4rem}input{width:100%;box-sizing:border-box;padding:.6rem;font:inherit;\
border:1px solid #bbb;border-radius:6px}button{margin-top:1rem;padding:.6rem 1.2rem;font:inherit;border:0;\
border-radius:6px;background:#1d1c1a;color:#fff}.wrong{color:#a12a1a}code{font-size:.9em}\
@media (prefers-color-scheme:dark){body{background:#1b1a18;color:#eceae5}input{background:#262522;color:inherit;\
border-color:#555}button{background:#eceae5;color:#1b1a18}.wrong{color:#f08a78}}";
    let content = match said {
        Said::NoCode => "<p>Aucun code d'accès n'a encore été créé pour l'interface. Sur le serveur, \
lancez <code>ipse access-code</code> (avec Docker : <code>docker compose exec ipse ipse access-code</code>), \
puis rechargez cette page.</p><p lang=\"en\">The interface has no access code yet. On the server, run \
<code>ipse access-code</code> (with Docker: <code>docker compose exec ipse ipse access-code</code>), then \
reload this page.</p>"
            .to_owned(),
        Said::Ask | Said::Wrong => format!(
            "{}<form method=\"post\" action=\"/login\"><label for=\"code\">Code d'accès \
<span lang=\"en\">· Access code</span></label><input id=\"code\" name=\"code\" type=\"password\" \
autocomplete=\"current-password\" required autofocus><button type=\"submit\">Ouvrir \
<span lang=\"en\">· Open</span></button></form><p>Le code vient de <code>ipse access-code</code>, \
sur le serveur.</p><p lang=\"en\">The code comes from <code>ipse access-code</code>, on the server.</p>",
            if said == Said::Wrong {
                "<p class=\"wrong\" role=\"alert\">Ce n'est pas le bon code. \
<span lang=\"en\">This is not the right code.</span></p>"
            } else {
                ""
            }
        ),
    };
    let html = format!(
        "<!doctype html><html lang=\"fr\"><head><meta charset=\"utf-8\">\
<meta name=\"viewport\" content=\"width=device-width, initial-scale=1\"><title>Ipse</title>\
<style>{STYLE}</style></head><body><main><h1>Ipse</h1>{content}</main></body></html>"
    );
    let mut response = Response::new(Full::new(Bytes::from(html)).boxed());
    *response.status_mut() = status;
    let headers = response.headers_mut();
    headers.insert(
        header::CONTENT_TYPE,
        HeaderValue::from_static("text/html; charset=utf-8"),
    );
    headers.insert(header::CACHE_CONTROL, HeaderValue::from_static("no-store"));
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
    fn listening_beyond_this_computer_is_asked() {
        let here = |port| SocketAddr::from((Ipv4Addr::LOCALHOST, port));
        assert_eq!(address("7420").unwrap(), here(7420));
        assert_eq!(address("localhost:8000").unwrap(), here(8000));
        assert_eq!(
            address("0.0.0.0:7420").unwrap(),
            SocketAddr::from((Ipv4Addr::UNSPECIFIED, 7420))
        );
        assert!(address("[::]:7420").is_ok());
        assert!(
            address("nas")
                .unwrap_err()
                .to_string()
                .contains("--listen takes")
        );
        assert!(started(here(7420), true).contains("Only this computer"));
        let open = started(SocketAddr::from((Ipv4Addr::UNSPECIFIED, 7420)), false);
        assert!(open.contains("reached from the network"), "{open}");
        assert!(open.contains("ipse access-code"), "{open}");
    }

    #[test]
    fn pages_of_the_address_asked() {
        let (get, post) = (Method::GET, Method::POST);
        let host = ("host", "ipse.example.org");
        assert!(from_its_pages(&get, &headers(&[host])));
        assert!(from_its_pages(
            &post,
            &headers(&[host, ("origin", "https://ipse.example.org")])
        ));
        assert!(from_its_pages(
            &post,
            &headers(&[
                ("host", "192.168.1.10:7420"),
                ("origin", "http://192.168.1.10:7420")
            ])
        ));
        for refused in [
            headers(&[host, ("origin", "https://evil.example")]),
            headers(&[host, ("sec-fetch-site", "cross-site")]),
            headers(&[("origin", "https://ipse.example.org")]),
        ] {
            assert!(!from_its_pages(&get, &refused), "{refused:?}");
        }
        assert!(
            !from_its_pages(&post, &headers(&[host])),
            "a change says where it comes from"
        );
    }
}
