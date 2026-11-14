//! The contacts connector: imports vCard files (`.vcf`) the person picked.
//!
//! Rung 4 of decision 0008: contacts apps export vCard files; this connector reads them
//! without any network access or account.

mod phone;
mod vcard;

use std::fs::File;
use std::io::{BufRead, BufReader, Write};

use ipse_core::journal::data_hash;
use ipse_protocol::{
    Access, DoneSummary, ErrorCode, FileAccess, Localized, MAX_BATCH_RECORDS, Manifest, Mode, Op,
    PROTOCOL_VERSION, Record,
};
use jiff::Timestamp;
use serde_json::{Map, Value, json};

use ipse_protocol::connector::{Session, SessionResult};
use phone::{Country, e164};
use vcard::{Card, CardError, Cards};

/// The connector's identifier.
pub(crate) const ID: &str = "contacts";

/// The record kind it sends.
pub(crate) const KIND: &str = "contact.card";

/// The built-in manifest (`docs/spec/connectors.md`, section 2).
#[allow(dead_code)] // read by the connector host, #25
pub(crate) fn manifest() -> Manifest {
    Manifest {
        protocol: PROTOCOL_VERSION,
        id: ID.to_owned(),
        name: Localized::ByLanguage(
            [("en", "Contacts"), ("fr", "Contacts")]
                .into_iter()
                .map(|(l, t)| (l.to_owned(), t.to_owned()))
                .collect(),
        ),
        summary: Localized::ByLanguage(
            [
                ("en", "The people you know, from a contacts file (.vcf)."),
                (
                    "fr",
                    "Les personnes que vous connaissez, depuis un fichier de contacts (.vcf).",
                ),
            ]
            .into_iter()
            .map(|(l, t)| (l.to_owned(), t.to_owned()))
            .collect(),
        ),
        version: env!("CARGO_PKG_VERSION").to_owned(),
        command: vec!["ipse".into(), "connector".into(), ID.into()],
        modes: vec![Mode::Import],
        rung: 4,
        kinds: vec![KIND.to_owned()],
        category: "people".to_owned(),
        access: Access {
            network: Vec::new(),
            files: FileAccess::Picked,
            secrets: Vec::new(),
        },
        sync: None,
    }
}

/// Runs one session: reads the picked files and sends their cards in batches.
pub(crate) fn run(input: impl BufRead, output: impl Write) -> SessionResult<()> {
    let (mut session, hello) = Session::open(input, output, ID, env!("CARGO_PKG_VERSION"))?;
    let french = hello.language == "fr";
    if hello.mode != Mode::Import {
        session.error(
            ErrorCode::Unsupported,
            say(
                french,
                "This connector only imports files.",
                "Ce connecteur importe seulement des fichiers.",
            ),
        )?;
        return session.finish();
    }
    session.wait_start()?;

    let observed_at = Timestamp::now();
    // National phone numbers are read with the country of the vault's time zone.
    let country = Country::of_time_zone(&hello.time_zone);
    let mut batch = Vec::with_capacity(MAX_BATCH_RECORDS);
    let mut skipped = 0_u64;
    for path in &hello.files {
        let Ok(file) = File::open(path) else {
            return bad_input(session, french);
        };
        for card in Cards::new(BufReader::new(file)) {
            match card {
                Ok(card) => {
                    batch.push(record(card, country, observed_at));
                    if batch.len() == MAX_BATCH_RECORDS {
                        send(&mut session, &mut batch)?;
                    }
                }
                Err(CardError::Unreadable) => skipped += 1,
                Err(CardError::Io) => return bad_input(session, french),
            }
        }
    }
    send(&mut session, &mut batch)?;
    let sent = session.flush()?.accepted;

    if skipped > 0 {
        let message = if french {
            format!("{skipped} fiche(s) illisible(s) ignorée(s).")
        } else {
            format!("{skipped} unreadable card(s) skipped.")
        };
        session.progress(sent, Some(sent), Some(message))?;
    }
    session.done(DoneSummary {
        upserts: sent,
        ..DoneSummary::default()
    })?;
    session.finish()
}

/// Sends a batch without waiting for its `ack`, up to 4 at a time.
fn send<R: BufRead, W: Write>(
    session: &mut Session<R, W>,
    batch: &mut Vec<Record>,
) -> SessionResult<()> {
    if batch.is_empty() {
        return Ok(());
    }
    let records = std::mem::replace(batch, Vec::with_capacity(MAX_BATCH_RECORDS));
    session.push(records, None)
}

fn bad_input<R: BufRead, W: Write>(mut session: Session<R, W>, french: bool) -> SessionResult<()> {
    session.error(
        ErrorCode::BadInput,
        say(
            french,
            "One of the picked files could not be read. Check that it is a contacts file (.vcf).",
            "Un des fichiers choisis n'a pas pu être lu. Vérifiez que c'est un fichier de contacts (.vcf).",
        ),
    )?;
    session.finish()
}

fn say(french: bool, en: &str, fr: &str) -> String {
    if french { fr } else { en }.to_owned()
}

/// A card as a `contact.card` record. Its identifier is the card's `UID`, or a hash of
/// its content when it has none, so that importing the same file twice gives the same
/// identifiers.
fn record(card: Card, country: Option<Country>, observed_at: Timestamp) -> Record {
    let data = data(&card, country);
    let external_id = card.uid.unwrap_or_else(|| data_hash(&data));
    Record {
        kind: KIND.to_owned(),
        op: Op::Upsert,
        external_id: Some(external_id),
        observed_at,
        data: Some(data),
    }
}

/// The `data` of a `contact.card` record. Empty fields are left out; phones in E.164
/// when they can be read with certainty.
fn data(card: &Card, country: Option<Country>) -> Value {
    let mut map = Map::new();
    map.insert("full_name".into(), json!(card.full_name));
    let optional = [
        ("given_name", &card.given_name),
        ("family_name", &card.family_name),
        ("birthday", &card.birthday),
        ("organization", &card.organization),
        ("title", &card.title),
        ("note", &card.note),
    ];
    for (key, value) in optional {
        if let Some(value) = value {
            map.insert(key.into(), json!(value));
        }
    }
    let phones: Vec<String> = card.phones.iter().map(|p| e164(p, country)).collect();
    let lists = [
        ("nicknames", &card.nicknames),
        ("emails", &card.emails),
        ("phones", &phones),
        ("addresses", &card.addresses),
    ];
    for (key, values) in lists {
        if !values.is_empty() {
            map.insert(key.into(), json!(values));
        }
    }
    if !card.related.is_empty() {
        let related: Vec<Value> = card
            .related
            .iter()
            .map(|r| {
                let mut entry = Map::new();
                for (key, value) in [("name", &r.name), ("uid", &r.uid), ("kind", &r.kind)] {
                    if let Some(value) = value {
                        entry.insert(key.into(), json!(value));
                    }
                }
                Value::Object(entry)
            })
            .collect();
        map.insert("related".into(), Value::Array(related));
    }
    Value::Object(map)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn manifest_follows_the_spec() {
        let m = manifest();
        assert_eq!(
            (m.id.as_str(), m.rung, m.category.as_str()),
            ("contacts", 4, "people")
        );
        assert_eq!(m.access.files, FileAccess::Picked);
        assert!(m.access.network.is_empty() && m.access.secrets.is_empty());
        assert_eq!(m.modes, [Mode::Import]);
        assert_eq!(m.kinds, [KIND]);
    }

    #[test]
    fn data_leaves_out_empty_fields() {
        let card = Card {
            full_name: "Ana Silva".into(),
            emails: vec!["ana@example.org".into()],
            ..Card::default()
        };
        assert_eq!(
            data(&card, None),
            json!({"full_name": "Ana Silva", "emails": ["ana@example.org"]})
        );
    }

    #[test]
    fn related_people_in_data() {
        let card = Card {
            full_name: "Camille Durand".into(),
            related: vec![
                vcard::Related {
                    name: Some("Marie Dupont".into()),
                    uid: None,
                    kind: Some("mother".into()),
                },
                vcard::Related {
                    name: None,
                    uid: Some("urn:uuid:7d1e".into()),
                    kind: None,
                },
            ],
            ..Card::default()
        };
        assert_eq!(
            data(&card, None)["related"],
            json!([{"name": "Marie Dupont", "kind": "mother"}, {"uid": "urn:uuid:7d1e"}])
        );
    }

    #[test]
    fn phones_in_e164_with_the_vaults_country() {
        let card = Card {
            full_name: "Ana Silva".into(),
            phones: vec![
                "06 12 34 56 78".into(),
                "+351 912 345 678".into(),
                "3615".into(),
            ],
            ..Card::default()
        };
        assert_eq!(
            data(&card, Country::of_time_zone("Europe/Paris"))["phones"],
            json!(["+33612345678", "+351912345678", "3615"])
        );
        assert_eq!(
            data(&card, None)["phones"],
            json!(["06 12 34 56 78", "+351912345678", "3615"]),
            "without a country, a national number stays as written"
        );
    }

    #[test]
    fn id_without_uid_is_stable() {
        let card = Card {
            full_name: "Ana Silva".into(),
            ..Card::default()
        };
        let now = Timestamp::now();
        let a = record(card.clone(), None, now);
        let b = record(card, None, now);
        assert_eq!(a.external_id, b.external_id);
        assert!(a.external_id.unwrap().starts_with("b3:"));
    }
}
