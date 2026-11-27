//! The Jellyfin connector (`docs/connectors/jellyfin.md`): reads, from the person's own
//! Jellyfin, the films and episodes they watched, read only.
//!
//! Decision 0008, rung 1: Jellyfin's REST API, with an API key the person creates. The
//! plays of one user, the one the person chose, are sent as `media.watched` records,
//! from where the last session stopped.

use std::io::{BufRead, Write};

use ipse_protocol::connector::{Session, SessionResult};
use ipse_protocol::{
    Access, DoneSummary, ErrorCode, FileAccess, Localized, Manifest, Mode, Op, PROTOCOL_VERSION,
    Record, SyncSettings,
};
use jiff::{SignedDuration, Timestamp};
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};

use super::rest::{self, Client, Failure, Service};

pub(crate) use super::rest::Found;

/// The connector's identifier.
pub(crate) const ID: &str = "jellyfin";

/// The secret holding the API key.
pub(crate) const KEY_SECRET: &str = "key";

/// The record kind: one film or episode watched.
pub(crate) const KIND: &str = "media.watched";

/// What the messages say of Jellyfin.
pub(crate) const SERVICE: Service = Service {
    name: "Jellyfin",
    refused: (
        "Jellyfin refused the API key. Create a new one in Dashboard → API Keys, then connect again.",
        "Jellyfin a refusé la clé d'API. Créez-en une nouvelle dans Tableau de bord → Clés API, puis connectez-vous à nouveau.",
    ),
    not_found: (
        "No Jellyfin was found at this address. Check it: it usually ends with :8096.",
        "Aucun Jellyfin n'a été trouvé à cette adresse. Vérifiez-la : elle finit souvent par :8096.",
    ),
};

/// How far back the first sync reads.
const FIRST_READ: SignedDuration = SignedDuration::from_hours(365 * 24);

/// Items asked per request.
const PAGE: usize = 200;

/// Items read per session at most: a year of evenings is far below it.
const MAX_ITEMS: usize = 5000;

/// The address the person typed, as Ipse connects to it, and whether it is plain
/// `http://` on the home network.
///
/// # Errors
///
/// A [`Failure`] for an address Ipse does not connect to.
pub(crate) fn address(text: &str) -> Result<(String, bool), Failure> {
    rest::address(text, &SERVICE)
}

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
        name: localized([("en", "Jellyfin"), ("fr", "Jellyfin")]),
        summary: localized([
            (
                "en",
                "The films and series you watch, from your own Jellyfin.",
            ),
            (
                "fr",
                "Les films et les séries que vous regardez, depuis votre propre Jellyfin.",
            ),
        ]),
        version: env!("CARGO_PKG_VERSION").to_owned(),
        command: vec!["ipse".into(), "connector".into(), ID.into()],
        modes: vec![Mode::Sync],
        rung: 1,
        kinds: vec![KIND.to_owned()],
        category: "media".to_owned(),
        access: Access {
            // Only the server the person gives.
            network: vec!["*".to_owned()],
            files: FileAccess::None,
            secrets: vec![KEY_SECRET.to_owned()],
        },
        sync: Some(SyncSettings {
            default_every: "1h".to_owned(),
        }),
    }
}

/// What the connector remembers between sessions.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct Cursor {
    /// The users found at the last session, to choose from.
    #[serde(default)]
    pub(crate) found: Vec<Found>,
    /// The last play read: the next session reads the plays after it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    since: Option<Timestamp>,
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
                "This connector keeps Jellyfin in sync; it imports no file.",
                "Ce connecteur synchronise Jellyfin ; il n'importe aucun fichier.",
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
    let Some(key) = hello.secrets.get(KEY_SECRET) else {
        session.error(
            ErrorCode::BadInput,
            say(
                french,
                "The Jellyfin API key is missing from the keychain. Connect Jellyfin again.",
                "La clé d'API de Jellyfin manque dans le trousseau. Connectez à nouveau Jellyfin.",
            ),
        )?;
        return session.finish();
    };
    let authorization = format!(
        "MediaBrowser Client=\"Ipse\", Device=\"Ipse\", DeviceId=\"ipse\", Version=\"{}\", Token=\"{}\"",
        env!("CARGO_PKG_VERSION"),
        key.expose()
    );
    let client = Client::new(SERVICE, &url, plain, ("Authorization", authorization));
    let cursor = hello
        .cursor
        .clone()
        .and_then(|c| serde_json::from_value::<Cursor>(c).ok())
        .unwrap_or_default();
    let user = Some(text("user")).filter(|u| !u.is_empty());
    match sync(&client, user, cursor, full, Timestamp::now()) {
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

/// Lists the users of Jellyfin, then reads the chosen one's plays since the last one
/// read, or over the last year.
fn sync(
    client: &Client,
    user: Option<&str>,
    mut cursor: Cursor,
    full: bool,
    now: Timestamp,
) -> Result<(Vec<Record>, Cursor), Failure> {
    cursor.found = users(&client.get("/Users")?)?;
    let Some(user) = user else {
        // Nothing chosen yet: the person picks from what was found.
        return Ok((Vec::new(), cursor));
    };
    let since = cursor
        .since
        .filter(|_| !full)
        .unwrap_or_else(|| now - FIRST_READ);
    let mut records = Vec::new();
    // What was watched to the end, then what is under way.
    for filter in ["IsPlayed", "IsResumable"] {
        let mut start = 0;
        loop {
            let path = format!(
                "/Users/{}/Items?Recursive=true&IncludeItemTypes=Episode,Movie&Filters={filter}&SortBy=DatePlayed&SortOrder=Descending&Fields=ProductionYear&EnableImages=false&StartIndex={start}&Limit={PAGE}",
                rest::encode(user)
            );
            let answer = client.get(&path)?;
            let items = answer
                .get("Items")
                .and_then(Value::as_array)
                .ok_or_else(|| Failure::not_found(&SERVICE))?;
            let mut older = false;
            for item in items {
                match play(item) {
                    Some(record) if record.observed_at > since => records.push(record),
                    Some(_) => older = true,
                    None => {}
                }
            }
            start += items.len();
            if older || items.len() < PAGE || start >= MAX_ITEMS {
                break;
            }
        }
    }
    if let Some(last) = records.iter().map(|r| r.observed_at).max() {
        cursor.since = Some(last.max(since));
    } else if cursor.since.is_none() || full {
        cursor.since = Some(since);
    }
    records.sort_by_key(|r| r.observed_at);
    Ok((records, cursor))
}

/// The users of `/Users`, with their names.
fn users(answer: &Value) -> Result<Vec<Found>, Failure> {
    let users = answer
        .as_array()
        .ok_or_else(|| Failure::not_found(&SERVICE))?;
    let mut found: Vec<Found> = users
        .iter()
        .filter_map(|user| {
            Some(Found {
                id: user.get("Id")?.as_str()?.to_owned(),
                name: user.get("Name")?.as_str()?.to_owned(),
            })
        })
        .collect();
    found.sort_by(|a, b| a.name.cmp(&b.name));
    Ok(found)
}

/// One play of an item of `/Users/{id}/Items`: a film or an episode, when it was last
/// played, and whether to the end. `None` for an item never played, or of another
/// type.
fn play(item: &Value) -> Option<Record> {
    let text = |key: &str| item.get(key).and_then(Value::as_str);
    let id = text("Id")?;
    let kind = match text("Type")? {
        "Movie" => "movie",
        "Episode" => "episode",
        _ => return None,
    };
    let user = item.get("UserData")?;
    let at: Timestamp = user.get("LastPlayedDate")?.as_str()?.parse().ok()?;
    let mut data = Map::new();
    data.insert("item".into(), id.into());
    data.insert("type".into(), kind.into());
    data.insert("title".into(), text("Name")?.into());
    data.insert("played_at".into(), at.to_string().into());
    data.insert(
        "finished".into(),
        user.get("Played")
            .and_then(Value::as_bool)
            .unwrap_or(false)
            .into(),
    );
    if let Some(series) = text("SeriesName") {
        data.insert("series".into(), series.into());
    }
    for (from, to) in [
        ("ParentIndexNumber", "season"),
        ("IndexNumber", "episode"),
        ("ProductionYear", "year"),
    ] {
        if let Some(n) = item.get(from).and_then(Value::as_u64) {
            data.insert(to.into(), n.into());
        }
    }
    Some(Record {
        kind: KIND.to_owned(),
        op: Op::Upsert,
        external_id: Some(format!("play:{id}:{}", at.as_second())),
        observed_at: at,
        data: Some(Value::Object(data)),
    })
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
    use serde_json::json;

    #[test]
    fn manifest_follows_the_spec() {
        let m = manifest();
        assert_eq!((m.id.as_str(), m.rung), ("jellyfin", 1));
        assert_eq!(m.modes, [Mode::Sync]);
        assert_eq!(m.access.secrets, ["key"]);
        assert_eq!(m.kinds, ["media.watched"]);
        assert_eq!(m.category, "media");
    }

    #[test]
    fn users_are_found() {
        let found = users(&json!([
            {"Name": "Paul", "Id": "u2"},
            {"Name": "Inès", "Id": "u1"},
            {"Name": "sans id"},
        ]))
        .unwrap();
        assert_eq!(
            found.iter().map(|f| f.id.as_str()).collect::<Vec<_>>(),
            ["u1", "u2"]
        );
        assert_eq!(users(&json!({})), Err(Failure::not_found(&SERVICE)));
    }

    #[test]
    fn a_play_says_what_and_when() {
        let episode = play(&json!({
            "Id": "e1", "Type": "Episode", "Name": "Le bureau", "SeriesName": "Les Archives",
            "ParentIndexNumber": 2, "IndexNumber": 3,
            "UserData": {"LastPlayedDate": "2026-11-20T21:30:00.0000000Z", "Played": true}
        }))
        .unwrap();
        let data = episode.data.unwrap();
        assert_eq!(data["series"], "Les Archives");
        assert_eq!(
            (data["season"].as_u64(), data["episode"].as_u64()),
            (Some(2), Some(3))
        );
        assert_eq!(data["finished"], true);
        assert_eq!(episode.external_id.as_deref(), Some("play:e1:1795210200"));

        let film = play(&json!({
            "Id": "m1", "Type": "Movie", "Name": "Le Phare", "ProductionYear": 2019,
            "UserData": {"LastPlayedDate": "2026-11-18T20:00:00Z", "Played": false}
        }))
        .unwrap();
        assert_eq!(film.data.unwrap()["type"], "movie");
        assert!(
            play(&json!({"Id": "x", "Type": "Movie", "Name": "Jamais vu", "UserData": {}}))
                .is_none()
        );
        assert!(play(&json!({"Id": "a", "Type": "Audio", "Name": "x", "UserData": {"LastPlayedDate": "2026-11-18T20:00:00Z"}})).is_none());
    }
}
