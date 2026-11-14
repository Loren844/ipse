//! MCP over Streamable HTTP, on this computer only (`docs/spec/rights.md`, section 3;
//! decision 0012).
//!
//! - It listens on `127.0.0.1` only: any other address is refused before anything
//!   starts.
//! - Every request carries the access code as `Authorization: Bearer ipse_…`. It is
//!   checked through the same gateway as standard input and output, before the request
//!   reaches MCP; a missing or wrong code gets `401` and no detail. Each tool checks it
//!   again before reading.
//! - `Host` must name this computer, against DNS rebinding; a browser's `Origin` must
//!   be a page on this computer, against other sites calling it.
//! - Each request is served on its own (stateless Streamable HTTP, JSON answers): the
//!   tools only answer, so nothing needs a session.

use std::convert::Infallible;
use std::net::{Ipv4Addr, SocketAddr};
use std::path::PathBuf;
use std::sync::{Arc, Mutex, PoisonError};

use http_body_util::combinators::BoxBody;
use http_body_util::{BodyExt as _, Empty};
use hyper::body::{Bytes, Incoming};
use hyper::header::{AUTHORIZATION, HeaderMap, HeaderValue, WWW_AUTHENTICATE};
use hyper::{Request, Response, StatusCode};
use hyper_util::rt::TokioIo;
use ipse_core::rights::Gateway;
use ipse_core::{Error, Result};
use rmcp::transport::streamable_http_server::session::never::NeverSessionManager;
use rmcp::transport::streamable_http_server::{StreamableHttpServerConfig, StreamableHttpService};

use super::{Server, Token, stopped};

/// The path MCP is served at.
const PATH: &str = "/mcp";

/// The port used when none is given.
pub(crate) const DEFAULT_PORT: u16 = 7420;

/// The `Host` names that mean this computer, any port.
const HOSTS: [&str; 2] = ["127.0.0.1", "localhost"];

/// The browser origins allowed: pages served by this computer.
const ORIGINS: [&str; 2] = ["http://127.0.0.1:*", "http://localhost:*"];

type Body = BoxBody<Bytes, Infallible>;

/// The address to listen on: a port (`7420`, `0` for any free one), or
/// `127.0.0.1:<port>` or `localhost:<port>`. Any other address is refused.
///
/// # Errors
///
/// [`Error::Refused`] for an address that is not this computer's, or not an address.
pub(crate) fn address(text: &str) -> Result<SocketAddr> {
    let text = text.trim();
    let port = match text.rsplit_once(':') {
        None => text,
        Some((host, port)) if HOSTS.contains(&host) => port,
        Some(_) => return Err(not_here()),
    };
    let port: u16 = port.parse().map_err(|_| {
        Error::Refused(format!(
            "--http takes a port, like {DEFAULT_PORT}, or 127.0.0.1:{DEFAULT_PORT}."
        ))
    })?;
    Ok(SocketAddr::from((Ipv4Addr::LOCALHOST, port)))
}

fn not_here() -> Error {
    Error::Refused(format!(
        "Ipse serves AIs over HTTP only on this computer, at 127.0.0.1. Give a port, like \
         `--http {DEFAULT_PORT}`; reaching your memory from another device comes later."
    ))
}

/// The access code of an `Authorization: Bearer <code>` header.
pub(crate) fn bearer(headers: &HeaderMap) -> Option<&str> {
    let value = headers.get(AUTHORIZATION)?.to_str().ok()?;
    let (scheme, code) = value.trim().split_once(' ')?;
    let code = code.trim();
    (scheme.eq_ignore_ascii_case("bearer") && !code.is_empty()).then_some(code)
}

/// Serves the memory behind `gateway` at `address` until the process is stopped.
///
/// # Errors
///
/// [`Error::Io`] when the address cannot be listened on (a port already taken).
pub(crate) fn serve(gateway: Gateway, address: SocketAddr) -> Result<()> {
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .map_err(|e| stopped(&e))?;
    runtime.block_on(async move {
        let listener = tokio::net::TcpListener::bind(address)
            .await
            .map_err(|source| Error::Io {
                action: "listen on",
                path: PathBuf::from(address.to_string()),
                source,
            })?;
        let local = listener.local_addr().map_err(|e| stopped(&e))?;
        // Standard output stays free; this line is for the person and for logs.
        eprintln!(
            "Ipse serves your memory over MCP at http://{local}{PATH} (this computer only). \
             AI apps send their access code as `Authorization: Bearer <code>`. Stop with Ctrl+C."
        );

        let gateway = Arc::new(Mutex::new(gateway));
        let config = StreamableHttpServerConfig::default()
            .with_allowed_hosts(HOSTS)
            .with_allowed_origins(ORIGINS)
            .with_legacy_session_mode(false)
            .with_json_response(true);
        let for_tools = Arc::clone(&gateway);
        let mcp = StreamableHttpService::new(
            move || Ok(Server::new(Arc::clone(&for_tools), Token::PerRequest)),
            Arc::new(NeverSessionManager::default()),
            config,
        );

        loop {
            let Ok((stream, _)) = listener.accept().await else {
                continue;
            };
            let mcp = mcp.clone();
            let gateway = Arc::clone(&gateway);
            tokio::spawn(async move {
                let service = hyper::service::service_fn(move |request| {
                    answer(request, mcp.clone(), Arc::clone(&gateway))
                });
                // A client that hangs up mid-request is not Ipse's problem to report.
                let _ = hyper::server::conn::http1::Builder::new()
                    .serve_connection(TokioIo::new(stream), service)
                    .await;
            });
        }
    })
}

/// One request: the path, then the access code, then MCP.
async fn answer(
    request: Request<Incoming>,
    mcp: StreamableHttpService<Server, NeverSessionManager>,
    gateway: Arc<Mutex<Gateway>>,
) -> std::result::Result<Response<Body>, Infallible> {
    if request.uri().path() != PATH {
        return Ok(empty(StatusCode::NOT_FOUND));
    }
    let allowed = bearer(request.headers()).is_some_and(|code| {
        let mut gateway = gateway.lock().unwrap_or_else(PoisonError::into_inner);
        gateway.fresh_access(Some(code)).is_ok()
    });
    if !allowed {
        let mut response = empty(StatusCode::UNAUTHORIZED);
        response
            .headers_mut()
            .insert(WWW_AUTHENTICATE, HeaderValue::from_static("Bearer"));
        return Ok(response);
    }
    Ok(mcp.handle(request).await)
}

fn empty(status: StatusCode) -> Response<Body> {
    let mut response = Response::new(Empty::new().boxed());
    *response.status_mut() = status;
    response
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_this_computer() {
        let here = |port| SocketAddr::from((Ipv4Addr::LOCALHOST, port));
        assert_eq!(address("7420").unwrap(), here(7420));
        assert_eq!(address("0").unwrap(), here(0));
        assert_eq!(address("127.0.0.1:8000").unwrap(), here(8000));
        assert_eq!(address("localhost:8000").unwrap(), here(8000));
        for text in [
            "0.0.0.0:7420",
            "192.168.1.10:7420",
            "[::]:7420",
            "example.org:80",
        ] {
            let error = address(text).unwrap_err().to_string();
            assert!(error.contains("only on this computer"), "{text}: {error}");
        }
        assert!(
            address("port")
                .unwrap_err()
                .to_string()
                .contains("takes a port")
        );
        assert!(address("70000").is_err());
    }

    #[test]
    fn bearer_codes() {
        let headers = |value: &str| {
            let mut h = HeaderMap::new();
            h.insert(AUTHORIZATION, HeaderValue::from_str(value).unwrap());
            h
        };
        assert_eq!(bearer(&headers("Bearer ipse_abc")), Some("ipse_abc"));
        assert_eq!(bearer(&headers("bearer  ipse_abc ")), Some("ipse_abc"));
        assert_eq!(bearer(&headers("Basic ipse_abc")), None);
        assert_eq!(bearer(&headers("Bearer")), None);
        assert_eq!(bearer(&HeaderMap::new()), None);
    }
}
