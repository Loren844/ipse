//! The Google Maps Timeline connector (issue #106): imports the location history the
//! person exported from their phone (`Timeline.json`, `location-history.json`) or from
//! the older Google Takeout (`docs/connectors/location.md`).
//!
//! Rung 4 of decision 0008: Google keeps the Timeline on the phone and lets it be
//! exported; this connector reads the export without any network access or account.
//! The file is read as a stream: a year holds hundreds of thousands of points.

mod format;

use std::fs::File;
use std::io::{BufRead, Write};

use ipse_protocol::{
    Access, DoneSummary, ErrorCode, FileAccess, Localized, MAX_BATCH_RECORDS, Manifest, Mode, Op,
    PROTOCOL_VERSION, Record,
};
use jiff::Timestamp;
use serde_json::{Map, Value, json};

use format::{Found, Frequent, Point, ReadError, Visit};
use ipse_protocol::connector::{Session, SessionResult};

/// The connector's identifier.
pub(crate) const ID: &str = "google-timeline";

/// A stay somewhere.
pub(crate) const VISIT: &str = "location.visit";

/// Positions, in batches (`docs/spec/journal.md`, section 4).
pub(crate) const POINTS: &str = "location.points";

/// A place Google marked as home or work.
pub(crate) const PLACE: &str = "location.place";

/// Points in one batch at most.
const MAX_POINTS: usize = 10_000;

/// A batch covers one hour at most.
const BATCH_SECONDS: i64 = 3_600;

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
        name: localized([
            ("en", "Location history"),
            ("fr", "Historique des positions"),
        ]),
        summary: localized([
            (
                "en",
                "Where you went, from your Google Maps Timeline export (.json).",
            ),
            (
                "fr",
                "Les lieux où vous êtes allé, depuis l'export de votre Timeline Google Maps (.json).",
            ),
        ]),
        version: env!("CARGO_PKG_VERSION").to_owned(),
        command: vec!["ipse".into(), "connector".into(), ID.into()],
        modes: vec![Mode::Import],
        rung: 4,
        kinds: vec![VISIT.to_owned(), POINTS.to_owned(), PLACE.to_owned()],
        category: "location".to_owned(),
        access: Access {
            network: Vec::new(),
            files: FileAccess::Picked,
            secrets: Vec::new(),
        },
        sync: None,
    }
}

/// Points waiting to be sent as one batch.
#[derive(Default)]
struct Points {
    t: Vec<i64>,
    lat: Vec<f64>,
    lon: Vec<f64>,
    acc: Vec<Option<f64>>,
}

impl Points {
    /// True when `point` starts a new batch: a batch holds one hour at most.
    fn full_before(&self, point: &Point) -> bool {
        self.t.len() >= MAX_POINTS
            || self
                .t
                .first()
                .is_some_and(|first| (point.t - first).abs() >= BATCH_SECONDS)
    }

    fn push(&mut self, point: Point) {
        self.t.push(point.t);
        self.lat.push(round(point.lat));
        self.lon.push(round(point.lon));
        self.acc.push(point.acc.map(f64::round));
    }

    /// The batch as a record; `None` when empty. Its identifier is its first time and
    /// size, so that importing the same file again changes nothing.
    fn take(&mut self) -> Option<Record> {
        let first = *self.t.first()?;
        let points = std::mem::take(self);
        let external_id = format!("points:{first}:{}", points.t.len());
        Some(Record {
            kind: POINTS.to_owned(),
            op: Op::Upsert,
            external_id: Some(external_id),
            observed_at: Timestamp::from_second(first).unwrap_or(Timestamp::UNIX_EPOCH),
            data: Some(json!({
                "t": points.t,
                "lat": points.lat,
                "lon": points.lon,
                "acc": points.acc,
            })),
        })
    }
}

/// Six decimals: about 10 cm, finer than any phone.
fn round(degrees: f64) -> f64 {
    (degrees * 1e6).round() / 1e6
}

/// A visit as a `location.visit` record.
fn visit_record(visit: Visit) -> Record {
    let at = visit
        .place_id
        .clone()
        .unwrap_or_else(|| format!("{:.5},{:.5}", visit.lat, visit.lon));
    let mut data = Map::new();
    data.insert("start".into(), json!(visit.start.to_string()));
    data.insert("end".into(), json!(visit.end.to_string()));
    data.insert("lat".into(), json!(round(visit.lat)));
    data.insert("lon".into(), json!(round(visit.lon)));
    let optional = [
        ("place_id", visit.place_id.map(Value::String)),
        ("name", visit.name.map(Value::String)),
        ("address", visit.address.map(Value::String)),
        (
            "semantic",
            visit.semantic.map(|s| Value::String(s.to_owned())),
        ),
        (
            "probability",
            visit
                .probability
                .map(|p| json!((p * 100.0).round() / 100.0)),
        ),
    ];
    for (key, value) in optional {
        if let Some(value) = value {
            data.insert(key.into(), value);
        }
    }
    Record {
        kind: VISIT.to_owned(),
        op: Op::Upsert,
        external_id: Some(format!("visit:{}:{at}", visit.start.as_second())),
        observed_at: visit.start,
        data: Some(Value::Object(data)),
    }
}

/// A frequent place as a `location.place` record.
fn place_record(place: &Frequent, observed_at: Timestamp) -> Record {
    let mut data = Map::new();
    data.insert("place_id".into(), json!(place.place_id));
    data.insert("label".into(), json!(place.label));
    if let (Some(lat), Some(lon)) = (place.lat, place.lon) {
        data.insert("lat".into(), json!(round(lat)));
        data.insert("lon".into(), json!(round(lon)));
    }
    Record {
        kind: PLACE.to_owned(),
        op: Op::Upsert,
        external_id: Some(format!("place:{}", place.place_id)),
        observed_at,
        data: Some(Value::Object(data)),
    }
}

/// What a session read.
#[derive(Default)]
struct Counts {
    visits: u64,
    points: u64,
}

/// Runs one session: reads the picked files and sends their visits, points and
/// frequent places in batches.
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

    let now = Timestamp::now();
    let mut batch: Vec<Record> = Vec::with_capacity(MAX_BATCH_RECORDS);
    let mut points = Points::default();
    let mut counts = Counts::default();
    let mut failed: Option<ipse_protocol::connector::SessionError> = None;
    for path in &hello.files {
        let Ok(file) = File::open(path) else {
            return bad_input(session, french);
        };
        let mut sink = |found: Found| -> Result<(), String> {
            match found {
                Found::Visit(visit) => {
                    counts.visits += 1;
                    batch.push(visit_record(visit));
                }
                Found::Point(point) => {
                    counts.points += 1;
                    if points.full_before(&point) {
                        batch.extend(points.take());
                    }
                    points.push(point);
                }
                Found::Frequent(place) => batch.push(place_record(&place, now)),
            }
            if batch.len() >= MAX_BATCH_RECORDS {
                let records = std::mem::replace(&mut batch, Vec::with_capacity(MAX_BATCH_RECORDS));
                if let Err(e) = session.push(records, None) {
                    let reason = e.to_string();
                    failed = Some(e);
                    return Err(reason);
                }
            }
            Ok(())
        };
        match format::read(file, &mut sink) {
            Ok(_) => {}
            Err(ReadError::Stopped(reason)) => {
                return Err(failed.unwrap_or_else(|| {
                    ipse_protocol::connector::SessionError::from(std::io::Error::other(reason))
                }));
            }
            Err(ReadError::NotTimeline) => return bad_input(session, french),
        }
    }
    batch.extend(points.take());
    if !batch.is_empty() {
        session.push(batch, None)?;
    }
    let sent = session.flush()?.accepted;
    let message = if french {
        format!(
            "{} visite(s) et {} position(s) lues.",
            counts.visits, counts.points
        )
    } else {
        format!(
            "{} visit(s) and {} position(s) read.",
            counts.visits, counts.points
        )
    };
    session.progress(sent, Some(sent), Some(message))?;
    session.done(DoneSummary {
        upserts: sent,
        ..DoneSummary::default()
    })?;
    session.finish()
}

fn bad_input<R: BufRead, W: Write>(mut session: Session<R, W>, french: bool) -> SessionResult<()> {
    session.error(
        ErrorCode::BadInput,
        say(
            french,
            "One of the picked files is not a Google Maps Timeline export. On your phone: Settings → Location → Location services → Timeline → Export Timeline data.",
            "Un des fichiers choisis n'est pas un export de la Timeline Google Maps. Sur votre téléphone : Paramètres → Localisation → Services de localisation → Timeline → Exporter les données de la Timeline.",
        ),
    )?;
    session.finish()
}

fn say(french: bool, en: &str, fr: &str) -> String {
    if french { fr } else { en }.to_owned()
}
