//! The Immich connector (`docs/connectors/immich.md`): reads, from the person's own
//! Immich, the people its face recognition found and the person named, with how many
//! photos show them and when the first and the last were taken. Read only.
//!
//! Decision 0008, rung 1: Immich's REST API, with an API key the person creates. The
//! key belongs to one user of Immich: the connector reads that user's library.

use std::io::{BufRead, Write};

use ipse_protocol::connector::{Session, SessionResult};
use ipse_protocol::{
    Access, DoneSummary, ErrorCode, FileAccess, Localized, Manifest, Mode, Op, PROTOCOL_VERSION,
    Record, SyncSettings,
};
use jiff::Timestamp;
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value, json};

use super::rest::{self, Client, Failure, Service};

pub(crate) use super::rest::Found;

/// The connector's identifier.
pub(crate) const ID: &str = "immich";

/// The secret holding the API key.
pub(crate) const KEY_SECRET: &str = "key";

/// The record kind: one person of the photos, as Immich names them.
pub(crate) const KIND: &str = "photos.person";

/// What the messages say of Immich.
pub(crate) const SERVICE: Service = Service {
    name: "Immich",
    refused: (
        "Immich refused the API key. Create a new one in Account Settings → API Keys, then connect again.",
        "Immich a refusé la clé d'API. Créez-en une nouvelle dans Paramètres du compte → Clés API, puis connectez-vous à nouveau.",
    ),
    not_found: (
        "No Immich was found at this address. Check it: it usually ends with :2283.",
        "Aucun Immich n'a été trouvé à cette adresse. Vérifiez-la : elle finit souvent par :2283.",
    ),
};

/// People asked per request.
const PAGE: usize = 500;

/// People read per session at most.
const MAX_PEOPLE: usize = 5000;

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
        name: localized([("en", "Immich"), ("fr", "Immich")]),
        summary: localized([
            (
                "en",
                "Who is on your photos, and when you last took one together, from your own Immich.",
            ),
            (
                "fr",
                "Qui est sur vos photos, et quand vous en avez pris une ensemble pour la dernière fois, depuis votre propre Immich.",
            ),
        ]),
        version: env!("CARGO_PKG_VERSION").to_owned(),
        command: vec!["ipse".into(), "connector".into(), ID.into()],
        modes: vec![Mode::Sync],
        rung: 1,
        kinds: vec![KIND.to_owned()],
        category: "people".to_owned(),
        access: Access {
            // Only the server the person gives.
            network: vec!["*".to_owned()],
            files: FileAccess::None,
            secrets: vec![KEY_SECRET.to_owned()],
        },
        sync: Some(SyncSettings {
            default_every: "6h".to_owned(),
        }),
    }
}

/// What the connector remembers between sessions.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct Cursor {
    /// The user the key belongs to, found at the last session.
    #[serde(default)]
    pub(crate) found: Vec<Found>,
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
                "This connector keeps Immich in sync; it imports no file.",
                "Ce connecteur synchronise Immich ; il n'importe aucun fichier.",
            ),
        )?;
        return session.finish();
    }
    session.wait_start()?;
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
                "The Immich API key is missing from the keychain. Connect Immich again.",
                "La clé d'API d'Immich manque dans le trousseau. Connectez à nouveau Immich.",
            ),
        )?;
        return session.finish();
    };
    let client = Client::new(SERVICE, &url, plain, ("x-api-key", key.expose().to_owned()));
    let user = Some(text("user")).filter(|u| !u.is_empty());
    match sync(&client, user, Timestamp::now()) {
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

/// Finds the user the key belongs to, then, once it is chosen, reads the people of
/// their photos that have a name.
fn sync(
    client: &Client,
    user: Option<&str>,
    now: Timestamp,
) -> Result<(Vec<Record>, Cursor), Failure> {
    let me = client.get("/api/users/me")?;
    let cursor = Cursor {
        found: me_found(&me).into_iter().collect(),
    };
    if cursor.found.is_empty() {
        return Err(Failure::not_found(&SERVICE));
    }
    if user.is_none() {
        // Nothing chosen yet: the only user is chosen next.
        return Ok((Vec::new(), cursor));
    }
    let mut records = Vec::new();
    let mut page = 1;
    let mut read = 0;
    loop {
        let answer = client.get(&format!(
            "/api/people?withHidden=false&page={page}&size={PAGE}"
        ))?;
        let people = answer
            .get("people")
            .and_then(Value::as_array)
            .ok_or_else(|| Failure::not_found(&SERVICE))?;
        for person in people {
            if let Some(record) = person_record(client, person, now)? {
                records.push(record);
            }
        }
        read += people.len();
        let more = answer
            .get("hasNextPage")
            .and_then(Value::as_bool)
            .unwrap_or(false);
        if !more || people.is_empty() || read >= MAX_PEOPLE {
            break;
        }
        page += 1;
    }
    Ok((records, cursor))
}

/// The user of `/api/users/me`.
fn me_found(me: &Value) -> Option<Found> {
    Some(Found {
        id: me.get("id")?.as_str()?.to_owned(),
        name: me.get("name")?.as_str()?.to_owned(),
    })
}

/// One person of `/api/people`, with their photos: how many, the first and the last.
/// `None` for a person without a name, or with no photo.
fn person_record(
    client: &Client,
    person: &Value,
    now: Timestamp,
) -> Result<Option<Record>, Failure> {
    let text = |key: &str| {
        person
            .get(key)
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|t| !t.is_empty())
    };
    let (Some(id), Some(name)) = (text("id"), text("name")) else {
        return Ok(None);
    };
    let photos = client
        .get(&format!("/api/people/{}/statistics", rest::encode(id)))?
        .get("assets")
        .and_then(Value::as_u64)
        .unwrap_or(0);
    if photos == 0 {
        return Ok(None);
    }
    let taken = |order: &str| -> Result<Option<Timestamp>, Failure> {
        let answer = client.post(
            "/api/search/metadata",
            &json!({"personIds": [id], "size": 1, "order": order}),
        )?;
        Ok(answer
            .pointer("/assets/items/0")
            .and_then(|item| {
                item.get("fileCreatedAt")
                    .or_else(|| item.get("localDateTime"))
            })
            .and_then(Value::as_str)
            .and_then(|t| t.parse().ok()))
    };
    let (first, last) = (taken("asc")?, taken("desc")?);
    let mut data = Map::new();
    data.insert("person".into(), id.into());
    data.insert("name".into(), name.into());
    data.insert("photos".into(), photos.into());
    if let Some(first) = first {
        data.insert("first".into(), first.to_string().into());
    }
    if let Some(last) = last {
        data.insert("last".into(), last.to_string().into());
    }
    Ok(Some(Record {
        kind: KIND.to_owned(),
        op: Op::Upsert,
        external_id: Some(format!("person:{id}")),
        observed_at: last.unwrap_or(now),
        data: Some(Value::Object(data)),
    }))
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

    #[test]
    fn manifest_follows_the_spec() {
        let m = manifest();
        assert_eq!((m.id.as_str(), m.rung), ("immich", 1));
        assert_eq!(m.modes, [Mode::Sync]);
        assert_eq!(m.access.secrets, ["key"]);
        assert_eq!(m.kinds, ["photos.person"]);
        assert_eq!(m.category, "people");
    }

    #[test]
    fn the_key_s_user_is_found() {
        assert_eq!(
            me_found(&json!({"id": "u1", "name": "Inès", "email": "x"})),
            Some(Found {
                id: "u1".to_owned(),
                name: "Inès".to_owned()
            })
        );
        assert_eq!(me_found(&json!({"message": "x"})), None);
    }
}
