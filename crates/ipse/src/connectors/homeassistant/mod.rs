//! The Home Assistant connector (`docs/connectors/homeassistant.md`): reads, from the
//! person's own Home Assistant, when they were at home and in which zones, read only.
//!
//! Decision 0008, rung 1: Home Assistant's REST API, with a long-lived access token the
//! person creates. The presence of one `person.*` entity, the one the person chose, is
//! sent as one `home.presence` record per stay.

mod http;

use std::fmt::Write as _;
use std::io::{BufRead, Write};

use ipse_protocol::connector::{Session, SessionResult};
use ipse_protocol::{
    Access, DoneSummary, ErrorCode, FileAccess, Localized, Manifest, Mode, Op, PROTOCOL_VERSION,
    Record, SyncSettings,
};
use jiff::{SignedDuration, Timestamp};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

use http::{Client, Failure};

pub(crate) use http::address;

/// The connector's identifier.
pub(crate) const ID: &str = "homeassistant";

/// The secret holding the long-lived access token.
pub(crate) const TOKEN_SECRET: &str = "token";

/// The record kind: one stay of the person at home, in a zone, or away.
pub(crate) const KIND: &str = "home.presence";

/// How far back the first sync reads: Home Assistant keeps 10 days by default.
const FIRST_READ: SignedDuration = SignedDuration::from_hours(10 * 24);

/// The built-in manifest (`docs/spec/connectors.md`, section 2).
#[allow(dead_code)] // read by the connector host, #25
pub(crate) fn manifest() -> Manifest {
    let localized = |pairs: [(&str, &str); 2]| {
        Localized::ByLanguage(
            pairs
                .into_iter()
                .map(|(l, t)| (l.to_owned(), t.to_owned()))
                .collect(),
        )
    };
    Manifest {
        protocol: PROTOCOL_VERSION,
        id: ID.to_owned(),
        name: localized([("en", "Home Assistant"), ("fr", "Home Assistant")]),
        summary: localized([
            (
                "en",
                "When you are at home, and in which zones, from your own Home Assistant.",
            ),
            (
                "fr",
                "Quand vous êtes à la maison, et dans quelles zones, depuis votre propre Home Assistant.",
            ),
        ]),
        version: env!("CARGO_PKG_VERSION").to_owned(),
        command: vec!["ipse".into(), "connector".into(), ID.into()],
        modes: vec![Mode::Sync],
        rung: 1,
        kinds: vec![KIND.to_owned()],
        category: "home".to_owned(),
        access: Access {
            // Only the server the person gives.
            network: vec!["*".to_owned()],
            files: FileAccess::None,
            secrets: vec![TOKEN_SECRET.to_owned()],
        },
        sync: Some(SyncSettings {
            default_every: "5m".to_owned(),
        }),
    }
}

/// A person of Home Assistant, to choose from.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct Found {
    /// Its entity: what the person chooses, in the instance's `person`.
    pub(crate) entity: String,
    /// Its name in Home Assistant.
    pub(crate) name: String,
}

/// The stay going on at the last session.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
struct Stay {
    state: String,
    start: Timestamp,
}

/// What the connector remembers between sessions.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct Cursor {
    /// The people found at the last session, to choose from.
    #[serde(default)]
    pub(crate) found: Vec<Found>,
    /// The chosen person's stay going on, read again at the next session.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    current: Option<Stay>,
}

/// Runs one session.
pub(crate) fn run(input: impl BufRead, output: impl Write) -> SessionResult<()> {
    let (mut session, hello) = Session::open(input, output, ID, env!("CARGO_PKG_VERSION"))?;
    let french = hello.language == "fr";
    if hello.mode != Mode::Sync {
        session.error(
            ErrorCode::Unsupported,
            say(
                french,
                "This connector keeps Home Assistant in sync; it imports no file.",
                "Ce connecteur synchronise Home Assistant ; il n'importe aucun fichier.",
            ),
        )?;
        return session.finish();
    }
    let full = session.wait_start()?;
    let text = |key: &str| {
        hello
            .config
            .get(key)
            .and_then(Value::as_str)
            .unwrap_or_default()
    };
    let (url, plain) = match address(text("url")) {
        Ok(found) => found,
        Err(failure) => return fail(session, &failure, french),
    };
    let Some(token) = hello.secrets.get(TOKEN_SECRET) else {
        session.error(
            ErrorCode::BadInput,
            say(
                french,
                "The Home Assistant token is missing from the keychain. Connect Home Assistant again.",
                "Le jeton de Home Assistant manque dans le trousseau. Connectez à nouveau Home Assistant.",
            ),
        )?;
        return session.finish();
    };
    let client = Client::new(&url, plain, token.expose());
    let cursor = hello
        .cursor
        .clone()
        .and_then(|c| serde_json::from_value::<Cursor>(c).ok())
        .unwrap_or_default();
    let person = Some(text("person")).filter(|p| !p.is_empty());
    match sync(&client, person, cursor, full, Timestamp::now()) {
        Ok((records, cursor)) => {
            let upserts = records.len() as u64;
            session.push(records, serde_json::to_value(&cursor).ok())?;
            session.flush()?;
            session.done(DoneSummary {
                upserts,
                ..DoneSummary::default()
            })?;
            session.finish()
        }
        Err(failure) => fail(session, &failure, french),
    }
}

/// Lists the people of Home Assistant, then reads the chosen one's presence since the
/// stay going on at the last session, or over the last 10 days.
fn sync(
    client: &Client,
    person: Option<&str>,
    mut cursor: Cursor,
    full: bool,
    now: Timestamp,
) -> Result<(Vec<Record>, Cursor), Failure> {
    cursor.found = people(&client.get("/api/states")?)?;
    let Some(person) = person else {
        // Nothing chosen yet: the person picks from what was found.
        return Ok((Vec::new(), cursor));
    };
    let previous = cursor.current.take().filter(|_| !full);
    let from = previous
        .as_ref()
        .map_or_else(|| now - FIRST_READ, |stay| stay.start);
    let path = format!(
        "/api/history/period/{}?filter_entity_id={}&end_time={}&minimal_response&no_attributes",
        from.round(jiff::Unit::Second).unwrap_or(from),
        encode(person),
        now.round(jiff::Unit::Second).unwrap_or(now),
    );
    let stays = stays(&client.get(&path)?, previous.as_ref());
    let records = records(person, &stays, now);
    cursor.current = stays.last().cloned();
    Ok((records, cursor))
}

/// The `person.*` entities of `/api/states`, with their names.
fn people(states: &Value) -> Result<Vec<Found>, Failure> {
    let states = states.as_array().ok_or(http::NOT_HOME_ASSISTANT)?;
    let mut found: Vec<Found> = states
        .iter()
        .filter_map(|state| {
            let entity = state.get("entity_id")?.as_str()?;
            if !entity.starts_with("person.") {
                return None;
            }
            let name = state
                .pointer("/attributes/friendly_name")
                .and_then(Value::as_str)
                .unwrap_or(entity);
            Some(Found {
                entity: entity.to_owned(),
                name: name.to_owned(),
            })
        })
        .collect();
    found.sort_by(|a, b| a.name.cmp(&b.name));
    Ok(found)
}

/// The stays in an answer of `/api/history/period`, in order: one per change of state.
/// The first state of the answer is the one at the start of the period; when it goes on
/// the stay of the last session, that stay keeps its own start.
fn stays(history: &Value, previous: Option<&Stay>) -> Vec<Stay> {
    let mut stays: Vec<Stay> = history
        .get(0)
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(|item| {
            let state = item.get("state")?.as_str()?;
            let start = item.get("last_changed")?.as_str()?.parse().ok()?;
            Some(Stay {
                state: state.to_owned(),
                start,
            })
        })
        .collect();
    stays.sort_by_key(|s| s.start);
    stays.dedup_by(|later, earlier| later.state == earlier.state);
    if let (Some(first), Some(previous)) = (stays.first_mut(), previous)
        && first.state == previous.state
        && previous.start < first.start
    {
        first.start = previous.start;
    }
    stays
}

/// One `upsert` per stay. A stay that ended has its `end`; the one going on has `seen`,
/// the time of this session, so that the memory knows it still holds.
fn records(person: &str, stays: &[Stay], now: Timestamp) -> Vec<Record> {
    stays
        .iter()
        .enumerate()
        .map(|(i, stay)| {
            let mut data = json!({
                "entity": person,
                "state": stay.state,
                "start": stay.start.to_string(),
            });
            match stays.get(i + 1) {
                Some(next) => data["end"] = json!(next.start.to_string()),
                None => {
                    data["seen"] = json!(now.round(jiff::Unit::Second).unwrap_or(now).to_string());
                }
            }
            Record {
                kind: KIND.to_owned(),
                op: Op::Upsert,
                external_id: Some(format!("presence:{person}:{}", stay.start.as_second())),
                observed_at: stay.start,
                data: Some(data),
            }
        })
        .collect()
}

/// `text` for a query string: letters, digits and `._-` as they are, the rest
/// percent-encoded.
fn encode(text: &str) -> String {
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

fn fail<R: BufRead, W: Write>(
    mut session: Session<R, W>,
    failure: &Failure,
    french: bool,
) -> SessionResult<()> {
    session.error_retry(
        failure.code,
        failure.retry_after.clone(),
        failure.message(french),
    )?;
    session.finish()
}

fn say(french: bool, en: &str, fr: &str) -> String {
    if french { fr } else { en }.to_owned()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn at(text: &str) -> Timestamp {
        text.parse().unwrap()
    }

    #[test]
    fn manifest_follows_the_spec() {
        let m = manifest();
        assert_eq!((m.id.as_str(), m.rung), ("homeassistant", 1));
        assert_eq!(m.modes, [Mode::Sync]);
        assert_eq!(m.access.secrets, ["token"]);
        assert_eq!(m.kinds, ["home.presence"]);
        assert_eq!(m.category, "home");
    }

    #[test]
    fn people_are_found_by_their_entity() {
        let states = json!([
            {"entity_id": "person.ines", "state": "home", "attributes": {"friendly_name": "Inès"}},
            {"entity_id": "light.salon", "state": "on", "attributes": {}},
            {"entity_id": "person.paul", "state": "not_home", "attributes": {"friendly_name": "Paul"}},
        ]);
        let found = people(&states).unwrap();
        assert_eq!(
            found.iter().map(|f| f.entity.as_str()).collect::<Vec<_>>(),
            ["person.ines", "person.paul"]
        );
        assert_eq!(found[0].name, "Inès");
        assert_eq!(
            people(&json!({"message": "x"})),
            Err(http::NOT_HOME_ASSISTANT)
        );
    }

    #[test]
    fn a_stay_going_on_keeps_its_start() {
        let history = json!([[
            {"entity_id": "person.ines", "state": "home", "last_changed": "2026-11-20T18:00:00+00:00"},
            {"state": "home", "last_changed": "2026-11-20T18:30:00+00:00"},
            {"state": "not_home", "last_changed": "2026-11-21T08:00:00.120+00:00"},
            {"state": "Travail", "last_changed": "2026-11-21T08:40:00+00:00"},
        ]]);
        let previous = Stay {
            state: "home".to_owned(),
            start: at("2026-11-19T19:00:00Z"),
        };
        let found = stays(&history, Some(&previous));
        assert_eq!(
            found.iter().map(|s| s.state.as_str()).collect::<Vec<_>>(),
            ["home", "not_home", "Travail"]
        );
        assert_eq!(found[0].start, at("2026-11-19T19:00:00Z"));

        let now = at("2026-11-21T09:00:00Z");
        let sent = records("person.ines", &found, now);
        assert_eq!(sent.len(), 3);
        let first = sent[0].data.as_ref().unwrap();
        assert_eq!(first["end"], "2026-11-21T08:00:00.12Z");
        assert_eq!(
            sent[0].external_id.as_deref(),
            Some("presence:person.ines:1795114800")
        );
        let last = sent[2].data.as_ref().unwrap();
        assert_eq!(last["seen"], "2026-11-21T09:00:00Z");
        assert!(last.get("end").is_none());
    }

    #[test]
    fn query_values_are_encoded() {
        assert_eq!(encode("person.ines"), "person.ines");
        assert_eq!(encode("a b&c"), "a%20b%26c");
    }
}
