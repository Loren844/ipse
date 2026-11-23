//! Requests to a CalDAV and CardDAV server: WebDAV methods with the person's user name
//! and app password (HTTP Basic, over HTTPS only), redirects followed by hand so that
//! the credentials only go where they should.
//!
//! The address, the user name and the password never appear in a message or a log.

use std::net::IpAddr;
use std::time::Duration;

use ipse_protocol::ErrorCode;
use ureq::http::{Method, Request, StatusCode, Uri, header};

/// The largest answer read: a multiget of 100 items is far below it.
const MAX_BYTES: u64 = 64 * 1024 * 1024;

/// Redirects followed for one request.
const MAX_REDIRECTS: usize = 5;

/// A failure, in words for the person, never quoting the address or the user name.
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

    pub(crate) fn message(&self, french: bool) -> String {
        if french { self.fr } else { self.en }.to_owned()
    }
}

pub(crate) const UNREACHABLE: Failure = Failure::new(
    ErrorCode::Unavailable,
    "The server could not be reached. Ipse will try again later.",
    "Le serveur n'a pas pu être joint. Ipse réessaiera plus tard.",
);

pub(crate) const REFUSED_LOGIN: Failure = Failure::new(
    ErrorCode::Auth,
    "The server refused the user name or the password. Check them, and use an app password where the service gives one.",
    "Le serveur a refusé le nom d'utilisateur ou le mot de passe. Vérifiez-les, et utilisez un mot de passe d'application si le service en donne.",
);

pub(crate) const NOT_DAV: Failure = Failure::new(
    ErrorCode::BadInput,
    "No calendar and no address book were found at this address. Check the server's address.",
    "Aucun agenda ni carnet d'adresses n'a été trouvé à cette adresse. Vérifiez l'adresse du serveur.",
);

pub(crate) const NOT_SECURE: Failure = Failure::new(
    ErrorCode::BadInput,
    "Ipse only connects to secure addresses (https://): with http://, anyone on the way could read your password.",
    "Ipse se connecte seulement aux adresses sécurisées (https://) : avec http://, n'importe qui sur le trajet pourrait lire votre mot de passe.",
);

const ELSEWHERE: Failure = Failure::new(
    ErrorCode::BadInput,
    "The server sends Ipse to another site, so Ipse did not send your password there. Check the server's address.",
    "Le serveur renvoie Ipse vers un autre site : Ipse n'y a pas envoyé votre mot de passe. Vérifiez l'adresse du serveur.",
);

const SLOW_DOWN: Failure = Failure::new(
    ErrorCode::RateLimited,
    "The server asks Ipse to slow down. Ipse will try again later.",
    "Le serveur demande à Ipse de ralentir. Ipse réessaiera plus tard.",
);

/// What the server answered.
#[derive(Debug)]
pub(crate) struct Answer {
    pub(crate) status: u16,
    /// Where the answer came from, after redirects: relative `href`s are read from it.
    pub(crate) url: String,
    pub(crate) body: String,
}

/// A connection to one account.
pub(crate) struct Client {
    agent: ureq::Agent,
    authorization: String,
    /// The host of the address the person gave, lowercase.
    host: String,
    /// Plain `http` is accepted only on this computer, for tests and local servers.
    local: bool,
}

impl std::fmt::Debug for Client {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("Client(hidden)")
    }
}

/// The address the person typed, as Ipse connects to it: `https://` added when
/// missing, a trailing `/` removed.
///
/// # Errors
///
/// [`NOT_DAV`] when it is not an address; [`NOT_SECURE`] for `http://` to another
/// computer.
pub(crate) fn address(text: &str) -> Result<(String, bool), Failure> {
    let text = text.trim().trim_end_matches('/');
    let url = if text.contains("://") {
        text.to_owned()
    } else {
        format!("https://{text}")
    };
    let uri: Uri = url.parse().map_err(|_| NOT_DAV)?;
    let host = uri.host().filter(|h| !h.is_empty()).ok_or(NOT_DAV)?;
    let local = is_loopback(host);
    match uri.scheme_str() {
        Some("https") => {}
        Some("http") if local => {}
        Some("http") => return Err(NOT_SECURE),
        _ => return Err(NOT_DAV),
    }
    Ok((url, local))
}

fn is_loopback(host: &str) -> bool {
    let host = host.trim_start_matches('[').trim_end_matches(']');
    host.eq_ignore_ascii_case("localhost")
        || host.parse::<IpAddr>().is_ok_and(|ip| ip.is_loopback())
}

/// The host of `url`, lowercase.
pub(crate) fn host_of(url: &str) -> Option<String> {
    let uri: Uri = url.parse().ok()?;
    uri.host().map(str::to_ascii_lowercase)
}

/// Whether credentials given for `from` may go to `to`: the same host, or another
/// host of the same domain (`caldav.icloud.com` and `p42-caldav.icloud.com`).
pub(crate) fn same_site(from: &str, to: &str) -> bool {
    if from == to {
        return true;
    }
    if is_loopback(from) || from.parse::<IpAddr>().is_ok() {
        return false;
    }
    let domain = |host: &str| {
        let labels: Vec<&str> = host.rsplitn(3, '.').collect();
        (labels.len() >= 2).then(|| format!("{}.{}", labels[1], labels[0]))
    };
    domain(from).is_some_and(|d| domain(to).as_deref() == Some(d.as_str()))
}

/// `href` read against `base`: an absolute address, an absolute path, or a path
/// relative to `base`.
pub(crate) fn join(base: &str, href: &str) -> Option<String> {
    if href.starts_with("http://") || href.starts_with("https://") {
        return Some(href.to_owned());
    }
    let uri: Uri = base.parse().ok()?;
    let origin = format!("{}://{}", uri.scheme_str()?, uri.authority()?);
    if href.starts_with('/') {
        return Some(format!("{origin}{href}"));
    }
    let path = uri.path();
    let dir = &path[..=path.rfind('/').unwrap_or(0)];
    Some(format!("{origin}{dir}{href}"))
}

impl Client {
    /// A client for the account at `base` (from [`address`]).
    pub(crate) fn new(base: &str, local: bool, user: &str, password: &str) -> Self {
        let config = ureq::Agent::config_builder()
            .https_only(!local)
            .http_status_as_error(false)
            .allow_non_standard_methods(true)
            .max_redirects(0)
            .timeout_connect(Some(Duration::from_secs(15)))
            .timeout_global(Some(Duration::from_mins(2)))
            .user_agent(concat!("Ipse/", env!("CARGO_PKG_VERSION")));
        let config = if local { config.proxy(None) } else { config };
        let credentials = base64(format!("{user}:{password}").as_bytes());
        Self {
            agent: ureq::Agent::new_with_config(config.build()),
            authorization: format!("Basic {credentials}"),
            host: host_of(base).unwrap_or_default(),
            local,
        }
    }

    /// Sends `method` to `url` with `depth` and an XML `body`, following redirects to
    /// the same site. A status other than a success is an answer, not an error, except
    /// for those that say to stop (401, 403, 429, 5xx).
    ///
    /// # Errors
    ///
    /// A [`Failure`] for the person.
    pub(crate) fn send(
        &self,
        method: &str,
        url: &str,
        depth: Option<&str>,
        body: &str,
    ) -> Result<Answer, Failure> {
        let method = Method::from_bytes(method.as_bytes()).map_err(|_| UNREACHABLE)?;
        let mut url = url.to_owned();
        for _ in 0..=MAX_REDIRECTS {
            let mut request = Request::builder()
                .method(method.clone())
                .uri(&url)
                .header(header::AUTHORIZATION, &self.authorization)
                .header(header::CONTENT_TYPE, "application/xml; charset=utf-8");
            if let Some(depth) = depth {
                request = request.header("Depth", depth);
            }
            let request = request.body(body.to_owned()).map_err(|_| UNREACHABLE)?;
            let response = self.agent.run(request).map_err(|e| match e {
                ureq::Error::RequireHttpsOnly(_) => NOT_SECURE,
                _ => UNREACHABLE,
            })?;
            let status = response.status();
            if status.is_redirection() && status != StatusCode::NOT_MODIFIED {
                let location = response
                    .headers()
                    .get(header::LOCATION)
                    .and_then(|v| v.to_str().ok())
                    .and_then(|to| join(&url, to))
                    .ok_or(NOT_DAV)?;
                let to = host_of(&location).ok_or(NOT_DAV)?;
                if !(location.starts_with("https://") || self.local && is_loopback(&to)) {
                    return Err(NOT_SECURE);
                }
                if !same_site(&self.host, &to) {
                    return Err(ELSEWHERE);
                }
                url = location;
                continue;
            }
            match status.as_u16() {
                401 | 403 => return Err(REFUSED_LOGIN),
                429 => {
                    return Err(Failure {
                        retry_after: retry_after(response.headers()),
                        ..SLOW_DOWN
                    });
                }
                500.. => return Err(UNREACHABLE),
                _ => {}
            }
            let body = response
                .into_body()
                .into_with_config()
                .limit(MAX_BYTES)
                .read_to_string()
                .map_err(|_| UNREACHABLE)?;
            return Ok(Answer {
                status: status.as_u16(),
                url,
                body,
            });
        }
        Err(NOT_DAV)
    }
}

/// Standard Base64 with padding (RFC 4648), for HTTP Basic: too small a need for a
/// dependency.
fn base64(bytes: &[u8]) -> String {
    const ALPHABET: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
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
            } else {
                out.push('=');
            }
        }
    }
    out
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
    fn basic_credentials() {
        assert_eq!(
            base64(b"Aladdin:open sesame"),
            "QWxhZGRpbjpvcGVuIHNlc2FtZQ=="
        );
        assert_eq!(base64(b"ab"), "YWI=");
        assert_eq!(base64(b"abc"), "YWJj");
        assert_eq!(base64("inès:é".as_bytes()), "aW7DqHM6w6k=");
    }

    #[test]
    fn addresses_people_type() {
        assert_eq!(
            address(" cloud.example.org/ ").unwrap(),
            ("https://cloud.example.org".to_owned(), false)
        );
        assert_eq!(
            address("http://127.0.0.1:5232").unwrap(),
            ("http://127.0.0.1:5232".to_owned(), true)
        );
        assert_eq!(address("http://cloud.example.org"), Err(NOT_SECURE));
        assert_eq!(address("ftp://cloud.example.org"), Err(NOT_DAV));
        assert_eq!(address(""), Err(NOT_DAV));
    }

    #[test]
    fn credentials_stay_on_the_site() {
        assert!(same_site("caldav.icloud.com", "p42-caldav.icloud.com"));
        assert!(same_site("fastmail.com", "caldav.fastmail.com"));
        assert!(same_site("127.0.0.1", "127.0.0.1"));
        assert!(!same_site("cloud.example.org", "evil.example.net"));
        assert!(!same_site("127.0.0.1", "127.0.0.2"));
        assert!(!same_site("10.0.0.1", "10.0.0.2"));
    }

    #[test]
    fn hrefs_are_read_against_where_they_came_from() {
        let base = "https://dav.example.org/remote.php/dav/calendars/ines/";
        assert_eq!(
            join(base, "perso/").unwrap(),
            "https://dav.example.org/remote.php/dav/calendars/ines/perso/"
        );
        assert_eq!(
            join(base, "/principals/ines/").unwrap(),
            "https://dav.example.org/principals/ines/"
        );
        assert_eq!(
            join(base, "https://p42.example.org/x/").unwrap(),
            "https://p42.example.org/x/"
        );
        assert_eq!(
            join("http://127.0.0.1:5232/a/b.ics", "c.ics").unwrap(),
            "http://127.0.0.1:5232/a/c.ics"
        );
    }
}
