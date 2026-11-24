//! Requests to a Home Assistant server: `GET` on its REST API with the person's
//! long-lived access token (`Authorization: Bearer`), no redirect followed.
//!
//! The address and the token never appear in a message or a log.

use std::net::IpAddr;
use std::time::Duration;

use ipse_protocol::ErrorCode;
use ureq::http::{Request, Uri, header};

/// The largest answer read: ten days of a person's presence is far below it.
const MAX_BYTES: u64 = 16 * 1024 * 1024;

/// A failure, in words for the person, never quoting the address or the token.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Failure {
    pub(crate) code: ErrorCode,
    pub(crate) retry_after: Option<String>,
    en: &'static str,
    fr: &'static str,
}

impl Failure {
    const fn new(code: ErrorCode, en: &'static str, fr: &'static str) -> Self {
        Self {
            code,
            retry_after: None,
            en,
            fr,
        }
    }

    /// The failure in the person's language.
    pub(crate) fn message(&self, french: bool) -> String {
        if french { self.fr } else { self.en }.to_owned()
    }
}

pub(crate) const UNREACHABLE: Failure = Failure::new(
    ErrorCode::Unavailable,
    "Home Assistant could not be reached. Ipse will try again later.",
    "Home Assistant n'a pas pu être joint. Ipse réessaiera plus tard.",
);

pub(crate) const REFUSED_TOKEN: Failure = Failure::new(
    ErrorCode::Auth,
    "Home Assistant refused the access token. Create a new long-lived access token in your Home Assistant profile, then connect again.",
    "Home Assistant a refusé le jeton d'accès. Créez un nouveau jeton d'accès longue durée dans votre profil Home Assistant, puis connectez-vous à nouveau.",
);

pub(crate) const NOT_HOME_ASSISTANT: Failure = Failure::new(
    ErrorCode::BadInput,
    "No Home Assistant was found at this address. Check it: it usually ends with :8123.",
    "Aucun Home Assistant n'a été trouvé à cette adresse. Vérifiez-la : elle finit souvent par :8123.",
);

pub(crate) const NOT_SECURE: Failure = Failure::new(
    ErrorCode::BadInput,
    "Ipse connects with http:// only to an address of your home network. For any other address, use https://: with http://, anyone on the way could read your token.",
    "Ipse se connecte en http:// seulement à une adresse de votre réseau local. Pour toute autre adresse, utilisez https:// : avec http://, n'importe qui sur le trajet pourrait lire votre jeton.",
);

const SLOW_DOWN: Failure = Failure::new(
    ErrorCode::RateLimited,
    "Home Assistant asks Ipse to slow down. Ipse will try again later.",
    "Home Assistant demande à Ipse de ralentir. Ipse réessaiera plus tard.",
);

/// The address the person typed, as Ipse connects to it: `https://` added when
/// missing, a trailing `/` removed. The flag is true for plain `http://`, accepted only
/// on the home network.
///
/// # Errors
///
/// [`NOT_HOME_ASSISTANT`] when it is not an address; [`NOT_SECURE`] for `http://` to
/// an address outside the home network.
pub(crate) fn address(text: &str) -> Result<(String, bool), Failure> {
    let text = text.trim().trim_end_matches('/');
    let url = if text.contains("://") {
        text.to_owned()
    } else {
        format!("https://{text}")
    };
    let uri: Uri = url.parse().map_err(|_| NOT_HOME_ASSISTANT)?;
    let host = uri
        .host()
        .filter(|h| !h.is_empty())
        .ok_or(NOT_HOME_ASSISTANT)?;
    match uri.scheme_str() {
        Some("https") => Ok((url, false)),
        Some("http") if on_home_network(host) => Ok((url, true)),
        Some("http") => Err(NOT_SECURE),
        _ => Err(NOT_HOME_ASSISTANT),
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

/// A connection to one Home Assistant.
pub(crate) struct Client {
    agent: ureq::Agent,
    base: String,
    authorization: String,
}

impl std::fmt::Debug for Client {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("Client(hidden)")
    }
}

impl Client {
    /// A client for the server at `base` (from [`address`]); `plain` for `http://`.
    pub(crate) fn new(base: &str, plain: bool, token: &str) -> Self {
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
            authorization: format!("Bearer {token}"),
        }
    }

    /// `GET <base><path>` and its JSON answer.
    ///
    /// # Errors
    ///
    /// A [`Failure`] for the person: the server unreachable, the token refused, an
    /// answer that is not Home Assistant's.
    pub(crate) fn get(&self, path: &str) -> Result<serde_json::Value, Failure> {
        let request = Request::get(format!("{}{path}", self.base))
            .header(header::AUTHORIZATION, &self.authorization)
            .header(header::ACCEPT, "application/json")
            .body(())
            .map_err(|_| NOT_HOME_ASSISTANT)?;
        let response = self.agent.run(request).map_err(|e| match e {
            ureq::Error::RequireHttpsOnly(_) => NOT_SECURE,
            _ => UNREACHABLE,
        })?;
        match response.status().as_u16() {
            200 => {}
            401 | 403 => return Err(REFUSED_TOKEN),
            429 => {
                return Err(Failure {
                    retry_after: retry_after(response.headers()),
                    ..SLOW_DOWN
                });
            }
            500.. => return Err(UNREACHABLE),
            _ => return Err(NOT_HOME_ASSISTANT),
        }
        let body = response
            .into_body()
            .into_with_config()
            .limit(MAX_BYTES)
            .read_to_string()
            .map_err(|_| UNREACHABLE)?;
        serde_json::from_str(&body).map_err(|_| NOT_HOME_ASSISTANT)
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn addresses_people_type() {
        assert_eq!(
            address(" ha.example.org/ ").unwrap(),
            ("https://ha.example.org".to_owned(), false)
        );
        assert_eq!(
            address("http://homeassistant.local:8123").unwrap(),
            ("http://homeassistant.local:8123".to_owned(), true)
        );
        assert_eq!(
            address("http://192.168.1.20:8123").unwrap(),
            ("http://192.168.1.20:8123".to_owned(), true)
        );
        assert_eq!(address("http://ha.example.org"), Err(NOT_SECURE));
        assert_eq!(address("http://8.8.8.8:8123"), Err(NOT_SECURE));
        assert_eq!(address("ftp://ha.example.org"), Err(NOT_HOME_ASSISTANT));
        assert_eq!(address(""), Err(NOT_HOME_ASSISTANT));
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
}
