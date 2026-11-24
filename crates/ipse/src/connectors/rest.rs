//! Requests to a self-hosted service's REST API (Home Assistant, Jellyfin, Immich):
//! `GET`, and `POST` for searches that only read, with the person's token in a header,
//! no redirect followed.
//!
//! The address and the token never appear in a message or a log.

use std::net::IpAddr;
use std::time::Duration;

use ipse_protocol::ErrorCode;
use ureq::http::{Method, Request, Uri, header};

/// The largest answer read: a page of a server's items is far below it.
const MAX_BYTES: u64 = 16 * 1024 * 1024;

/// What the messages say of one service.
#[derive(Debug, Clone, Copy)]
pub(crate) struct Service {
    /// Its name: `Home Assistant`.
    pub(crate) name: &'static str,
    /// When it refuses the token, and how to make a new one.
    pub(crate) refused: (&'static str, &'static str),
    /// When the address answers, but not as this service.
    pub(crate) not_found: (&'static str, &'static str),
}

/// Someone of the server to choose from: a person of Home Assistant, a user of
/// Jellyfin. Kept in the connector's cursor.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub(crate) struct Found {
    /// What the person chooses, in the instance's config: `person.ines`, a user id.
    #[serde(alias = "entity")]
    pub(crate) id: String,
    /// The name on the server.
    pub(crate) name: String,
}

/// A failure, in words for the person, never quoting the address or the token.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Failure {
    pub(crate) code: ErrorCode,
    pub(crate) retry_after: Option<String>,
    en: String,
    fr: String,
}

impl Failure {
    fn new(code: ErrorCode, en: String, fr: String) -> Self {
        Self {
            code,
            retry_after: None,
            en,
            fr,
        }
    }

    /// The failure in the person's language.
    pub(crate) fn message(&self, french: bool) -> String {
        if french { &self.fr } else { &self.en }.clone()
    }

    /// The service cannot be reached, or failed.
    pub(crate) fn unreachable(service: &Service) -> Self {
        let name = service.name;
        Self::new(
            ErrorCode::Unavailable,
            format!("{name} could not be reached. Ipse will try again later."),
            format!("{name} n'a pas pu être joint. Ipse réessaiera plus tard."),
        )
    }

    /// The service refused the token.
    pub(crate) fn refused(service: &Service) -> Self {
        Self::new(
            ErrorCode::Auth,
            service.refused.0.to_owned(),
            service.refused.1.to_owned(),
        )
    }

    /// The address is not the service's, or its answer cannot be read.
    pub(crate) fn not_found(service: &Service) -> Self {
        Self::new(
            ErrorCode::BadInput,
            service.not_found.0.to_owned(),
            service.not_found.1.to_owned(),
        )
    }

    /// `http://` to an address outside the home network.
    pub(crate) fn not_secure() -> Self {
        Self::new(
            ErrorCode::BadInput,
            "Ipse connects with http:// only to an address of your home network. For any other address, use https://: with http://, anyone on the way could read your token.".to_owned(),
            "Ipse se connecte en http:// seulement à une adresse de votre réseau local. Pour toute autre adresse, utilisez https:// : avec http://, n'importe qui sur le trajet pourrait lire votre jeton.".to_owned(),
        )
    }

    fn slow_down(service: &Service) -> Self {
        let name = service.name;
        Self::new(
            ErrorCode::RateLimited,
            format!("{name} asks Ipse to slow down. Ipse will try again later."),
            format!("{name} demande à Ipse de ralentir. Ipse réessaiera plus tard."),
        )
    }
}

/// The address the person typed, as Ipse connects to it: `https://` added when
/// missing, a trailing `/` removed. The flag is true for plain `http://`, accepted only
/// on the home network.
///
/// # Errors
///
/// [`Failure::not_found`] when it is not an address; [`Failure::not_secure`] for
/// `http://` to an address outside the home network.
pub(crate) fn address(text: &str, service: &Service) -> Result<(String, bool), Failure> {
    let text = text.trim().trim_end_matches('/');
    let url = if text.contains("://") {
        text.to_owned()
    } else {
        format!("https://{text}")
    };
    let uri: Uri = url.parse().map_err(|_| Failure::not_found(service))?;
    let host = uri
        .host()
        .filter(|h| !h.is_empty())
        .ok_or_else(|| Failure::not_found(service))?;
    match uri.scheme_str() {
        Some("https") => Ok((url, false)),
        Some("http") if on_home_network(host) => Ok((url, true)),
        Some("http") => Err(Failure::not_secure()),
        _ => Err(Failure::not_found(service)),
    }
}

/// Whether `host` names a machine of the home network: this computer, a private or
/// link-local address, or a local name (`homeassistant.local`, `nas.lan`, `hass`).
fn on_home_network(host: &str) -> bool {
    let host = host.trim_start_matches('[').trim_end_matches(']');
    if let Ok(ip) = host.parse::<IpAddr>() {
        return match ip {
            IpAddr::V4(v4) => v4.is_loopback() || v4.is_private() || v4.is_link_local(),
            IpAddr::V6(v6) => {
                let first = v6.segments()[0];
                v6.is_loopback() || first & 0xfe00 == 0xfc00 || first & 0xffc0 == 0xfe80
            }
        };
    }
    let host = host.to_ascii_lowercase();
    !host.contains('.')
        || [".local", ".lan", ".home", ".home.arpa", ".internal"]
            .iter()
            .any(|suffix| host.ends_with(suffix))
}

/// A connection to one server.
pub(crate) struct Client {
    agent: ureq::Agent,
    base: String,
    /// The header carrying the token, and its value.
    token: (&'static str, String),
    service: Service,
}

impl std::fmt::Debug for Client {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("Client(hidden)")
    }
}

impl Client {
    /// A client for `service` at `base` (from [`address`]); `plain` for `http://`. The
    /// token goes in the header `token.0`, as `token.1`.
    pub(crate) fn new(
        service: Service,
        base: &str,
        plain: bool,
        token: (&'static str, String),
    ) -> Self {
        let config = ureq::Agent::config_builder()
            .https_only(!plain)
            .http_status_as_error(false)
            .max_redirects(0)
            .timeout_connect(Some(Duration::from_secs(15)))
            .timeout_global(Some(Duration::from_mins(2)))
            .user_agent(concat!("Ipse/", env!("CARGO_PKG_VERSION")));
        // A server of the home network is reached directly, never through a proxy.
        let config = if plain { config.proxy(None) } else { config };
        Self {
            agent: ureq::Agent::new_with_config(config.build()),
            base: base.to_owned(),
            token,
            service,
        }
    }

    /// `GET <base><path>` and its JSON answer.
    ///
    /// # Errors
    ///
    /// A [`Failure`] for the person: the server unreachable, the token refused, an
    /// answer that is not the service's.
    pub(crate) fn get(&self, path: &str) -> Result<serde_json::Value, Failure> {
        self.send(Method::GET, path, None)
    }

    /// `POST <base><path>` with a JSON `body`, for a search that only reads, and its JSON
    /// answer.
    ///
    /// # Errors
    ///
    /// As [`Client::get`].
    pub(crate) fn post(
        &self,
        path: &str,
        body: &serde_json::Value,
    ) -> Result<serde_json::Value, Failure> {
        self.send(Method::POST, path, Some(body))
    }

    fn send(
        &self,
        method: Method,
        path: &str,
        body: Option<&serde_json::Value>,
    ) -> Result<serde_json::Value, Failure> {
        let service = &self.service;
        let request = Request::builder()
            .method(method)
            .uri(format!("{}{path}", self.base))
            .header(self.token.0, &self.token.1)
            .header(header::ACCEPT, "application/json")
            .header(header::CONTENT_TYPE, "application/json")
            .body(body.map(ToString::to_string).unwrap_or_default())
            .map_err(|_| Failure::not_found(service))?;
        let response = self.agent.run(request).map_err(|e| match e {
            ureq::Error::RequireHttpsOnly(_) => Failure::not_secure(),
            _ => Failure::unreachable(service),
        })?;
        match response.status().as_u16() {
            200 | 201 => {}
            401 | 403 => return Err(Failure::refused(service)),
            429 => {
                return Err(Failure {
                    retry_after: retry_after(response.headers()),
                    ..Failure::slow_down(service)
                });
            }
            500.. => return Err(Failure::unreachable(service)),
            _ => return Err(Failure::not_found(service)),
        }
        let body = response
            .into_body()
            .into_with_config()
            .limit(MAX_BYTES)
            .read_to_string()
            .map_err(|_| Failure::unreachable(service))?;
        serde_json::from_str(&body).map_err(|_| Failure::not_found(service))
    }
}

/// `Retry-After` in seconds, as the protocol's duration (`90s`).
fn retry_after(headers: &ureq::http::HeaderMap) -> Option<String> {
    let seconds: u64 = headers
        .get(header::RETRY_AFTER)?
        .to_str()
        .ok()?
        .trim()
        .parse()
        .ok()?;
    Some(format!("{seconds}s"))
}

/// `text` for a query string: letters, digits and `._-` as they are, the rest
/// percent-encoded.
pub(crate) fn encode(text: &str) -> String {
    use std::fmt::Write as _;
    let mut out = String::with_capacity(text.len());
    for byte in text.bytes() {
        if byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'_' | b'-') {
            out.push(char::from(byte));
        } else {
            let _ = write!(out, "%{byte:02X}");
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    const TEST: Service = Service {
        name: "Test",
        refused: ("refused", "refusé"),
        not_found: ("not found", "introuvable"),
    };

    #[test]
    fn addresses_people_type() {
        assert_eq!(
            address(" ha.example.org/ ", &TEST).unwrap(),
            ("https://ha.example.org".to_owned(), false)
        );
        assert_eq!(
            address("http://homeassistant.local:8123", &TEST).unwrap(),
            ("http://homeassistant.local:8123".to_owned(), true)
        );
        assert_eq!(
            address("http://192.168.1.20:8123", &TEST).unwrap(),
            ("http://192.168.1.20:8123".to_owned(), true)
        );
        assert_eq!(
            address("http://ha.example.org", &TEST),
            Err(Failure::not_secure())
        );
        assert_eq!(
            address("http://8.8.8.8:8123", &TEST),
            Err(Failure::not_secure())
        );
        assert_eq!(
            address("ftp://ha.example.org", &TEST),
            Err(Failure::not_found(&TEST))
        );
        assert_eq!(address("", &TEST), Err(Failure::not_found(&TEST)));
    }

    #[test]
    fn the_home_network() {
        for host in [
            "127.0.0.1",
            "10.0.0.4",
            "172.16.3.1",
            "192.168.0.10",
            "169.254.1.1",
            "[::1]",
            "[fd00::1]",
            "[fe80::1]",
            "hass",
            "homeassistant.local",
            "nas.lan",
            "box.home.arpa",
        ] {
            assert!(on_home_network(host), "{host}");
        }
        for host in ["8.8.8.8", "172.32.0.1", "[2001:db8::1]", "ha.example.org"] {
            assert!(!on_home_network(host), "{host}");
        }
    }

    #[test]
    fn query_values_are_encoded() {
        assert_eq!(encode("person.ines"), "person.ines");
        assert_eq!(encode("a b&c"), "a%20b%26c");
    }
}
