//! Fetching a calendar from its link, for mode `sync` (decision 0008, rung 3: a
//! subscription link the calendar app gives, read-only, without an account).
//!
//! The link is a secret: whoever has it reads the calendar. It reaches the connector in
//! `hello`, from the keychain, and is never written, logged or put in a message.

use std::io::{BufRead, BufReader};
use std::net::IpAddr;
use std::time::Duration;

use ipse_protocol::ErrorCode;
use serde::{Deserialize, Serialize};
use ureq::http::{StatusCode, Uri, header};

/// A calendar larger than this is refused: no calendar is near it, a mistaken link
/// could be.
const MAX_BYTES: u64 = 64 * 1024 * 1024;

/// A link Ipse can follow.
#[derive(Clone, PartialEq, Eq)]
pub(crate) struct Link {
    url: String,
    /// Plain `http` is accepted only on this computer, for tests and local servers.
    local: bool,
}

impl std::fmt::Debug for Link {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("Link(hidden)")
    }
}

impl Link {
    /// The link as Ipse fetches it: `webcal://` read as `https://`.
    pub(crate) fn as_str(&self) -> &str {
        &self.url
    }
}

/// Why a text is not a link Ipse can follow.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum LinkError {
    /// Not a web link at all.
    NotALink,
    /// A plain `http://` link to another computer: anyone on the way could read it.
    NotSecure,
}

impl LinkError {
    /// What to do, in the person's language. Never quotes the link.
    pub(crate) fn message(self, french: bool) -> String {
        match (self, french) {
            (Self::NotALink, false) => "This is not a calendar link. It starts with https:// or webcal://.",
            (Self::NotALink, true) => "Ce n'est pas un lien d'agenda. Il commence par https:// ou webcal://.",
            (Self::NotSecure, false) => "Ipse only follows secure links (https:// or webcal://): with http://, anyone on the way could read your calendar.",
            (Self::NotSecure, true) => "Ipse suit seulement les liens sécurisés (https:// ou webcal://) : avec http://, n'importe qui sur le trajet pourrait lire votre agenda.",
        }
        .to_owned()
    }
}

/// Reads a link as the person pasted it.
///
/// # Errors
///
/// When it is not a web link, or a plain `http://` one to another computer.
pub(crate) fn parse(text: &str) -> Result<Link, LinkError> {
    let text = text.trim();
    let (scheme, rest) = text.split_once("://").ok_or(LinkError::NotALink)?;
    let secure = match scheme.to_ascii_lowercase().as_str() {
        "https" | "webcal" | "webcals" => true,
        "http" => false,
        _ => return Err(LinkError::NotALink),
    };
    let url = format!("{}://{rest}", if secure { "https" } else { "http" });
    let uri: Uri = url.parse().map_err(|_| LinkError::NotALink)?;
    let host = uri
        .host()
        .filter(|h| !h.is_empty())
        .ok_or(LinkError::NotALink)?;
    let local = is_loopback(host);
    if !secure && !local {
        return Err(LinkError::NotSecure);
    }
    Ok(Link { url, local })
}

fn is_loopback(host: &str) -> bool {
    let host = host.trim_start_matches('[').trim_end_matches(']');
    host.eq_ignore_ascii_case("localhost")
        || host.parse::<IpAddr>().is_ok_and(|ip| ip.is_loopback())
}

/// What the connector remembers between sessions: how the server named this version of
/// the calendar, and the day it was read whole.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct Cursor {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) etag: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) last_modified: Option<String>,
    /// The day, in the vault's time zone, of the last full read.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) day: Option<String>,
}

/// What the server answered.
pub(crate) enum Fetched {
    /// Nothing changed since the cursor.
    Unchanged,
    /// The calendar, to read as a stream, and the cursor to save once it is read whole.
    Calendar(Box<dyn BufRead + Send>, Cursor),
}

/// A failure, in words for the person.
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

/// The calendar could not be reached.
pub(crate) const UNREACHABLE: Failure = Failure::new(
    ErrorCode::Unavailable,
    "The calendar could not be reached. Ipse will try again later.",
    "L'agenda n'a pas pu être joint. Ipse réessaiera plus tard.",
);

/// The download stopped before the end of the calendar.
pub(crate) const CUT_SHORT: Failure = Failure::new(
    ErrorCode::Unavailable,
    "The calendar stopped downloading before its end. Nothing was removed; Ipse will try again later.",
    "Le téléchargement de l'agenda s'est arrêté avant la fin. Rien n'a été retiré ; Ipse réessaiera plus tard.",
);

/// The link leads to something else than a calendar.
pub(crate) const NOT_A_CALENDAR: Failure = Failure::new(
    ErrorCode::BadInput,
    "This link does not lead to a calendar. Copy the calendar's address in iCal format (.ics) again from your calendar app.",
    "Ce lien ne mène pas à un agenda. Copiez à nouveau l'adresse de l'agenda au format iCal (.ics) depuis votre application d'agenda.",
);

const GONE: Failure = Failure::new(
    ErrorCode::Auth,
    "This calendar link no longer works: it may have been reset or the calendar deleted. Subscribe again with the new link.",
    "Ce lien d'agenda ne fonctionne plus : il a peut-être été réinitialisé, ou l'agenda supprimé. Abonnez-vous à nouveau avec le nouveau lien.",
);

const INSECURE_REDIRECT: Failure = Failure::new(
    ErrorCode::BadInput,
    "This calendar link sends Ipse to an insecure address (http://), so Ipse did not follow it.",
    "Ce lien d'agenda renvoie Ipse vers une adresse non sécurisée (http://) : Ipse ne l'a pas suivi.",
);

const TOO_LARGE: Failure = Failure::new(
    ErrorCode::BadInput,
    "This link leads to a file far larger than a calendar, so Ipse did not read it.",
    "Ce lien mène à un fichier bien plus gros qu'un agenda : Ipse ne l'a pas lu.",
);

const SLOW_DOWN: Failure = Failure::new(
    ErrorCode::RateLimited,
    "The calendar's server asks Ipse to slow down. Ipse will try again later.",
    "Le serveur de l'agenda demande à Ipse de ralentir. Ipse réessaiera plus tard.",
);

const REFUSED: Failure = Failure::new(
    ErrorCode::BadInput,
    "The calendar's server refused to give the calendar. Check the link in your calendar app.",
    "Le serveur de l'agenda a refusé de le donner. Vérifiez le lien dans votre application d'agenda.",
);

/// Fetches the calendar at `link`. With a `cursor` read on `today`, asks the server for
/// it only if it changed; otherwise reads it whole, so that repeating events are
/// expanded for the days that came since.
///
/// # Errors
///
/// A [`Failure`] saying what went wrong, never quoting the link.
pub(crate) fn fetch(link: &Link, cursor: Option<&Cursor>, today: &str) -> Result<Fetched, Failure> {
    let config = ureq::Agent::config_builder()
        .https_only(!link.local)
        .http_status_as_error(false)
        .timeout_connect(Some(Duration::from_secs(15)))
        .timeout_global(Some(Duration::from_mins(2)))
        .max_redirects(5)
        .user_agent(concat!("Ipse/", env!("CARGO_PKG_VERSION")));
    let config = if link.local {
        config.proxy(None)
    } else {
        config
    };
    let agent = ureq::Agent::new_with_config(config.build());

    let mut request = agent.get(link.as_str());
    if let Some(cursor) = cursor.filter(|c| c.day.as_deref() == Some(today)) {
        if let Some(etag) = &cursor.etag {
            request = request.header(header::IF_NONE_MATCH, etag);
        }
        if let Some(last_modified) = &cursor.last_modified {
            request = request.header(header::IF_MODIFIED_SINCE, last_modified);
        }
    }
    let response = request.call().map_err(|e| match e {
        ureq::Error::RequireHttpsOnly(_) => INSECURE_REDIRECT,
        _ => UNREACHABLE,
    })?;

    let status = response.status();
    if status == StatusCode::NOT_MODIFIED {
        return Ok(Fetched::Unchanged);
    }
    if !status.is_success() {
        return Err(match status.as_u16() {
            401 | 403 | 404 | 410 => GONE,
            429 => Failure {
                retry_after: retry_after(response.headers()),
                ..SLOW_DOWN
            },
            500.. => UNREACHABLE,
            _ => REFUSED,
        });
    }
    let length = response
        .headers()
        .get(header::CONTENT_LENGTH)
        .and_then(|v| v.to_str().ok()?.parse::<u64>().ok());
    if length.is_some_and(|l| l > MAX_BYTES) {
        return Err(TOO_LARGE);
    }
    let text = |name| {
        response
            .headers()
            .get(name)
            .and_then(|v| v.to_str().ok())
            .map(str::to_owned)
    };
    let cursor = Cursor {
        etag: text(header::ETAG),
        last_modified: text(header::LAST_MODIFIED),
        day: Some(today.to_owned()),
    };
    let body = response
        .into_body()
        .into_with_config()
        .limit(MAX_BYTES)
        .reader();
    Ok(Fetched::Calendar(
        Box::new(BufReader::with_capacity(64 * 1024, body)),
        cursor,
    ))
}

/// `Retry-After` in seconds, as the protocol's duration (`90s`). Dates are ignored:
/// the core's backoff applies then.
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
    fn links_people_paste() {
        let link = parse("  webcal://p42-caldav.icloud.example/published/2/abc ").unwrap();
        assert_eq!(
            link.as_str(),
            "https://p42-caldav.icloud.example/published/2/abc"
        );
        assert_eq!(
            parse("WEBCALS://cal.example/x.ics").unwrap().as_str(),
            "https://cal.example/x.ics"
        );
        let google =
            "https://calendar.example/calendar/ical/me%40example.org/private-0123abc/basic.ics";
        assert_eq!(parse(google).unwrap().as_str(), google);
        assert!(!parse(google).unwrap().local);
    }

    #[test]
    fn plain_http_only_on_this_computer() {
        assert_eq!(parse("http://cal.example/x.ics"), Err(LinkError::NotSecure));
        assert_eq!(parse("http://10.0.0.2/x.ics"), Err(LinkError::NotSecure));
        for local in [
            "http://127.0.0.1:8080/x.ics",
            "http://localhost/x.ics",
            "http://[::1]:9000/x.ics",
        ] {
            assert!(parse(local).unwrap().local, "{local}");
        }
    }

    #[test]
    fn not_links() {
        for text in [
            "",
            "agenda.ics",
            "ftp://cal.example/x.ics",
            "https://",
            "https:///x",
        ] {
            assert_eq!(parse(text), Err(LinkError::NotALink), "{text:?}");
        }
    }

    #[test]
    fn messages_never_show_the_link() {
        let link = parse("https://cal.example/private-secret.ics").unwrap();
        assert_eq!(format!("{link:?}"), "Link(hidden)");
        for error in [LinkError::NotALink, LinkError::NotSecure] {
            assert!(!error.message(false).is_empty());
            assert!(!error.message(true).is_empty());
        }
    }

    #[test]
    fn cursors_leave_out_what_is_unknown() {
        let cursor = Cursor {
            etag: Some("\"v1\"".into()),
            last_modified: None,
            day: Some("2026-11-14".into()),
        };
        let json = serde_json::to_value(&cursor).unwrap();
        assert_eq!(
            json,
            serde_json::json!({"etag": "\"v1\"", "day": "2026-11-14"})
        );
        assert_eq!(serde_json::from_value::<Cursor>(json).unwrap(), cursor);
    }
}
