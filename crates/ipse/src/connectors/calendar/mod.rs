//! The calendar connector: imports iCalendar files (`.ics`) the person picked, or
//! follows a calendar's subscription link.
//!
//! Decision 0008: calendar apps give a link to a calendar in iCal format, their official
//! way to let other apps read it (rung 1). In mode `sync`, the connector fetches it and
//! sends the whole calendar each time, with `done.complete`, so that the core removes
//! what disappeared (`fetch`). Calendar apps also export iCalendar files (rung 4); in
//! mode `import`, the connector reads the picked files without any network access.
//! Events are sent as they are read.
//! Repeating events are kept until the end of the file, because an occurrence moved or
//! cancelled may come after them; their rule is sent once, and their occurrences from
//! one year back to 90 days ahead are expanded (`docs/spec/vault.md`, section 4).

pub(crate) mod fetch;
mod ics;
mod rrule;
mod time;

use std::collections::{BTreeMap, HashMap, HashSet};
use std::fs::File;
use std::io::{BufRead, BufReader, Write};

use ipse_core::journal::data_hash;
use ipse_protocol::{
    Access, DoneSummary, ErrorCode, FileAccess, Localized, MAX_BATCH_RECORDS, Manifest, Mode, Op,
    PROTOCOL_VERSION, Record, SyncSettings,
};
use jiff::tz::TimeZone;
use jiff::{Span, Timestamp, ToSpan, Unit, Zoned};
use serde_json::{Map, Value, json};

use super::session::{Hello, Session, SessionResult};
use fetch::{CUT_SHORT, Cursor, Failure, Fetched, NOT_A_CALENDAR};
use ics::{Event, Item, Items, Person};
use rrule::{Rule, Until};
use time::{Key, When, Zones};

/// The connector's identifier.
pub(crate) const ID: &str = "calendar";

/// The record kind it sends.
pub(crate) const KIND: &str = "calendar.event";

/// How far back occurrences of repeating events are expanded.
const DAYS_BACK: i64 = 365;

/// How far ahead occurrences of repeating events are expanded.
const DAYS_AHEAD: i64 = 90;

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
        name: localized([("en", "Calendar"), ("fr", "Agenda")]),
        summary: localized([
            (
                "en",
                "Your events, from your calendar's link or a calendar file (.ics).",
            ),
            (
                "fr",
                "Vos événements, depuis le lien de votre agenda ou un fichier d'agenda (.ics).",
            ),
        ]),
        version: env!("CARGO_PKG_VERSION").to_owned(),
        command: vec!["ipse".into(), "connector".into(), ID.into()],
        modes: vec![Mode::Import, Mode::Sync],
        rung: 1,
        kinds: vec![KIND.to_owned()],
        category: "agenda".to_owned(),
        access: Access {
            // Only the host of the link the person gives.
            network: vec!["*".to_owned()],
            files: FileAccess::Picked,
            secrets: vec![LINK_SECRET.to_owned()],
        },
        sync: Some(SyncSettings {
            default_every: "15m".to_owned(),
        }),
    }
}

/// The secret holding the calendar's link, in mode `sync`.
pub(crate) const LINK_SECRET: &str = "url";

/// Runs one session: reads the picked files, or fetches the calendar's link, and sends
/// their events in batches.
pub(crate) fn run(input: impl BufRead, output: impl Write) -> SessionResult<()> {
    let (mut session, hello) = Session::open(input, output, ID, env!("CARGO_PKG_VERSION"))?;
    let french = hello.language == "fr";
    if !matches!(hello.mode, Mode::Import | Mode::Sync) {
        session.error(
            ErrorCode::Unsupported,
            say(
                french,
                "This connector imports calendar files or follows a calendar's link.",
                "Ce connecteur importe des fichiers d'agenda ou suit le lien d'un agenda.",
            ),
        )?;
        return session.finish();
    }
    let full = session.wait_start()?;
    let time_zone = TimeZone::get(&hello.time_zone).unwrap_or(TimeZone::UTC);
    let now = Timestamp::now();
    if hello.mode == Mode::Sync {
        return sync(session, &hello, full, time_zone, now);
    }

    let mut import = Import::new(time_zone, now);
    let mut sent = 0_u64;
    for path in &hello.files {
        let Ok(file) = File::open(path) else {
            return bad_input(session, french);
        };
        for item in Items::new(BufReader::new(file)) {
            match item {
                Ok(item) => import.add(item),
                Err(_) => return bad_input(session, french),
            }
            if import.records.len() >= MAX_BATCH_RECORDS {
                sent += send(&mut session, &mut import.records, None)?;
            }
        }
    }
    import.finish();
    sent += send(&mut session, &mut import.records, None)?;
    import.done(session, sent, french, false)
}

/// Mode `sync`: fetches the link and sends the whole calendar, or nothing when it did
/// not change.
fn sync<R: BufRead, W: Write>(
    mut session: Session<R, W>,
    hello: &Hello,
    full: bool,
    time_zone: TimeZone,
    now: Timestamp,
) -> SessionResult<()> {
    let french = hello.language == "fr";
    let link = match hello
        .secrets
        .get(LINK_SECRET)
        .map(|s| fetch::parse(s.expose()))
    {
        Some(Ok(link)) => link,
        Some(Err(e)) => {
            session.error(ErrorCode::BadInput, e.message(french))?;
            return session.finish();
        }
        None => {
            session.error(
                ErrorCode::BadInput,
                say(
                    french,
                    "This calendar's link is missing from the keychain. Subscribe to it again.",
                    "Le lien de cet agenda manque dans le trousseau. Abonnez-vous à nouveau.",
                ),
            )?;
            return session.finish();
        }
    };
    let cursor = hello
        .cursor
        .clone()
        .filter(|_| !full)
        .and_then(|c| serde_json::from_value::<Cursor>(c).ok());
    let today = now.to_zoned(time_zone.clone()).date().to_string();
    let (body, cursor) = match fetch::fetch(&link, cursor.as_ref(), &today) {
        Ok(Fetched::Unchanged) => {
            // Nothing sent, so nothing complete: the core must not remove anything.
            session.done(DoneSummary::default())?;
            return session.finish();
        }
        Ok(Fetched::Calendar(body, cursor)) => (body, cursor),
        Err(failure) => return fail(session, &failure, french),
    };

    let mut import = Import::new(time_zone, now);
    let mut items = Items::new(body);
    let mut sent = 0_u64;
    for item in items.by_ref() {
        match item {
            Ok(item) => import.add(item),
            Err(_) => return fail(session, &CUT_SHORT, french),
        }
        if import.records.len() >= MAX_BATCH_RECORDS {
            sent += send(&mut session, &mut import.records, None)?;
        }
    }
    if !items.whole() {
        let failure = if items.begun() {
            CUT_SHORT
        } else {
            NOT_A_CALENDAR
        };
        return fail(session, &failure, french);
    }
    import.finish();
    let cursor = serde_json::to_value(cursor).ok();
    sent += send(&mut session, &mut import.records, cursor)?;
    import.done(session, sent, french, true)
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

/// Sends `records` in batches; returns how many the core accepted. A `cursor` goes
/// with the last batch, alone if there is nothing to send: the core saves it only once
/// everything before it is on disk.
fn send<R: BufRead, W: Write>(
    session: &mut Session<R, W>,
    records: &mut Vec<Record>,
    mut cursor: Option<Value>,
) -> SessionResult<u64> {
    let mut accepted = 0;
    while !records.is_empty() || cursor.is_some() {
        let rest = records.split_off(records.len().min(MAX_BATCH_RECORDS));
        let batch = std::mem::replace(records, rest);
        let count = batch.len() as u64;
        let last = if records.is_empty() {
            cursor.take()
        } else {
            None
        };
        let rejected = session.batch(batch, last)?;
        accepted += count - rejected.len() as u64;
    }
    Ok(accepted)
}

fn bad_input<R: BufRead, W: Write>(mut session: Session<R, W>, french: bool) -> SessionResult<()> {
    session.error(
        ErrorCode::BadInput,
        say(
            french,
            "One of the picked files could not be read. Check that it is a calendar file (.ics).",
            "Un des fichiers choisis n'a pas pu être lu. Vérifiez que c'est un fichier d'agenda (.ics).",
        ),
    )?;
    session.finish()
}

fn say(french: bool, en: &str, fr: &str) -> String {
    if french { fr } else { en }.to_owned()
}

/// The period repeating events are expanded in.
#[derive(Debug, Clone)]
struct Window {
    from: Zoned,
    to: Zoned,
}

impl Window {
    fn around(now: Timestamp, time_zone: &TimeZone) -> Self {
        let now = now.to_zoned(time_zone.clone());
        let from = now
            .checked_sub(DAYS_BACK.days())
            .unwrap_or_else(|_| now.clone());
        let to = now
            .checked_add(DAYS_AHEAD.days())
            .unwrap_or_else(|_| now.clone());
        Self { from, to }
    }

    /// Whether an occurrence starting at `key` is before, in or after the window.
    fn place(&self, key: Key) -> std::cmp::Ordering {
        use std::cmp::Ordering;
        let (from, to) = match key {
            Key::Day(day) => (day < self.from.date(), day > self.to.date()),
            Key::At(ts) => (ts < self.from.timestamp(), ts > self.to.timestamp()),
        };
        match (from, to) {
            (true, _) => Ordering::Less,
            (_, true) => Ordering::Greater,
            _ => Ordering::Equal,
        }
    }
}

/// One import: turns what the file holds into records.
struct Import {
    zones: Zones,
    observed_at: Timestamp,
    window: Window,
    /// Records ready to send.
    records: Vec<Record>,
    /// Repeating events, expanded at the end of the file.
    repeating: Vec<Event>,
    /// For each `UID`, the occurrences the file moved or changed (`RECURRENCE-ID`).
    changed: HashMap<String, HashSet<Key>>,
    /// Events waiting for a time zone the file may define further down.
    waiting: Vec<Event>,
    upserts: u64,
    deletes: u64,
    unreadable: u64,
    not_expanded: u64,
}

impl Import {
    fn new(time_zone: TimeZone, now: Timestamp) -> Self {
        Self {
            window: Window::around(now, &time_zone),
            zones: Zones::new(time_zone),
            observed_at: now,
            records: Vec::with_capacity(MAX_BATCH_RECORDS),
            repeating: Vec::new(),
            changed: HashMap::new(),
            waiting: Vec::new(),
            upserts: 0,
            deletes: 0,
            unreadable: 0,
            not_expanded: 0,
        }
    }

    fn add(&mut self, item: Item) {
        match item {
            Item::TimeZone(definition) => self.zones.define(&definition),
            Item::Unreadable => self.unreadable += 1,
            Item::Event(event) => {
                if self.zones_known(&event) {
                    self.event(event);
                } else {
                    self.waiting.push(event);
                }
            }
        }
    }

    /// Ends the import: events that waited for a time zone (read in the vault's time
    /// zone if the file never defined it), then the occurrences of repeating events.
    fn finish(&mut self) {
        for event in std::mem::take(&mut self.waiting) {
            self.event(event);
        }
        for event in std::mem::take(&mut self.repeating) {
            self.expand(&event);
        }
    }

    /// Ends the session: a word on what was left out, then `done`. `complete` when
    /// every event the source holds was sent.
    fn done<R: BufRead, W: Write>(
        &self,
        mut session: Session<R, W>,
        sent: u64,
        french: bool,
        complete: bool,
    ) -> SessionResult<()> {
        if let Some(message) = self.report(french) {
            session.progress(sent, Some(sent), Some(message))?;
        }
        session.done(DoneSummary {
            upserts: self.upserts,
            deletes: self.deletes,
            complete,
            ..DoneSummary::default()
        })?;
        session.finish()
    }

    /// A message for the person when something was left out.
    fn report(&self, french: bool) -> Option<String> {
        let mut parts = Vec::new();
        if self.unreadable > 0 {
            let n = self.unreadable;
            parts.push(if french {
                format!("{n} événement(s) illisible(s) ignoré(s).")
            } else {
                format!("{n} unreadable event(s) skipped.")
            });
        }
        if self.not_expanded > 0 {
            let n = self.not_expanded;
            parts.push(if french {
                format!(
                    "{n} événement(s) répété(s) selon une règle qu'Ipse ne sait pas encore lire : seule la première date est importée."
                )
            } else {
                format!(
                    "{n} repeating event(s) use a rule Ipse cannot read yet: only the first date is imported."
                )
            });
        }
        (!parts.is_empty()).then(|| parts.join(" "))
    }

    fn zones_known(&mut self, event: &Event) -> bool {
        let stamps = [&event.start, &event.end, &event.recurrence_id]
            .into_iter()
            .flatten()
            .chain(&event.rdates)
            .chain(&event.exdates);
        let tzids: Vec<String> = stamps.filter_map(|s| s.tzid.clone()).collect();
        tzids.iter().all(|t| self.zones.knows(Some(t)))
    }

    /// One event: on its own, a change to one occurrence, or a repeating event.
    fn event(&mut self, event: Event) {
        let Some(base) = self.base(&event) else {
            self.unreadable += 1;
            return;
        };
        match (&event.recurrence_id, &event.uid) {
            (Some(stamp), Some(uid)) => {
                let Some(key) = self.zones.when(stamp).map(|w| w.key()) else {
                    self.unreadable += 1;
                    return;
                };
                self.changed.entry(uid.clone()).or_default().insert(key);
                let record = occurrence(uid, key, base.data, self.observed_at);
                self.push(record);
            }
            _ if event.rule.is_some() => {
                let id = base.id.clone();
                self.push(upsert(id, base.data, self.observed_at));
                self.repeating.push(event);
            }
            _ => self.push(upsert(base.id, base.data, self.observed_at)),
        }
    }

    /// The occurrences of a repeating event inside the window, from its rule and its
    /// `RDATE`s, without those moved or changed elsewhere in the file. Those removed
    /// with `EXDATE` are sent as deletions, so that a re-import removes them.
    fn expand(&mut self, event: &Event) {
        let Some(base) = self.base(event) else {
            return;
        };
        let uid = event.uid.clone().unwrap_or_else(|| base.id.clone());
        let mut data = base.data;
        data.remove("rule");
        let changed = self.changed.get(&uid).cloned().unwrap_or_default();
        let excluded: HashSet<Key> = event
            .exdates
            .iter()
            .filter_map(|s| self.zones.when(s))
            .map(|w| w.key())
            .collect();
        let excluded_days: HashSet<_> = excluded.iter().map(Key::date).collect();
        let is_excluded = |key: Key| match key {
            Key::Day(day) => excluded_days.contains(&day),
            Key::At(_) => excluded.contains(&key),
        };

        let rule = event.rule.as_deref().map(Rule::parse);
        let mut all: BTreeMap<Key, When> = BTreeMap::new();
        let expanded = if let Some(Ok(rule)) = rule {
            let starts = self.starts(event, &base.start, &rule);
            let extra = event.rdates.iter().filter_map(|s| self.zones.when(s));
            all.extend(starts.into_iter().chain(extra).map(|w| (w.key(), w)));
            true
        } else {
            // Only the first date is certain.
            self.not_expanded += 1;
            all.insert(base.start.key(), base.start.clone());
            false
        };
        for (key, start) in all {
            if expanded && self.window.place(key) != std::cmp::Ordering::Equal {
                continue;
            }
            if changed.contains(&key) {
                continue;
            }
            if is_excluded(key) {
                self.deletes += 1;
                self.records.push(Record {
                    kind: KIND.to_owned(),
                    op: Op::Delete,
                    external_id: Some(occurrence_id(&uid, key)),
                    observed_at: self.observed_at,
                    data: None,
                });
                continue;
            }
            let mut data = data.clone();
            let end = base.length.as_ref().and_then(|l| l.after(&start));
            data.insert("start".into(), json!(start.text()));
            match end {
                Some(end) => data.insert("end".into(), json!(end.text())),
                None => data.remove("end"),
            };
            let record = occurrence(&uid, key, data, self.observed_at);
            self.push(record);
        }
    }

    /// The starts the rule gives inside the window, at the start's time of day and in
    /// its time zone, up to `UNTIL`.
    fn starts(&mut self, event: &Event, start: &When, rule: &Rule) -> Vec<When> {
        let Some(stamp) = &event.start else {
            return Vec::new();
        };
        // A UTC start repeats at the same UTC time; others at the same local time.
        let utc = stamp.value.trim().ends_with(['Z', 'z']);
        let zone = if utc {
            TimeZone::UTC
        } else {
            self.zones.zone_of(stamp)
        };
        let until = rule.until.as_ref().and_then(|u| Self::until(u, &zone));
        let last = self
            .window
            .to
            .date()
            .tomorrow()
            .unwrap_or(self.window.to.date());
        let first_day = match start {
            When::Day(day) => *day,
            When::At(zoned) => zoned.with_time_zone(zone.clone()).date(),
        };
        let mut starts = Vec::new();
        for day in rule.days(first_day, last) {
            let when = match start {
                When::Day(_) => When::Day(day),
                When::At(zoned) => {
                    let local = zoned.with_time_zone(zone.clone()).time();
                    let Ok(at) = day.to_datetime(local).to_zoned(zone.clone()) else {
                        continue;
                    };
                    let shown = if utc {
                        at.with_time_zone(self.zones.fallback().clone())
                    } else {
                        at
                    };
                    When::At(shown)
                }
            };
            let past_until = match (&until, when.key()) {
                (Some(Limit::Day(limit)), key) => key_day(key, &zone) > *limit,
                (Some(Limit::At(limit)), Key::At(ts)) => ts > *limit,
                (Some(Limit::At(limit)), Key::Day(day)) => {
                    day > limit.to_zoned(zone.clone()).date()
                }
                (None, _) => false,
            };
            if past_until || self.window.place(when.key()) == std::cmp::Ordering::Greater {
                break;
            }
            starts.push(when);
        }
        starts
    }

    /// `UNTIL` as a day or an instant; a floating date-time is read in `zone`.
    fn until(until: &Until, zone: &TimeZone) -> Option<Limit> {
        match until {
            Until::Date(day) => Some(Limit::Day(*day)),
            Until::DateTime(text) => {
                let (datetime, utc) = time::parse_datetime(text)?;
                let zone = if utc { TimeZone::UTC } else { zone.clone() };
                datetime
                    .to_zoned(zone)
                    .ok()
                    .map(|z| Limit::At(z.timestamp()))
            }
        }
    }

    /// The `data` of an event as it stands, its identifier, start and length. `None`
    /// when it has no readable start.
    fn base(&mut self, event: &Event) -> Option<Base> {
        let start = self.zones.when(event.start.as_ref()?)?;
        let end = match (&event.end, &event.duration) {
            (Some(stamp), _) => self.zones.when(stamp),
            (None, Some(duration)) => duration
                .parse::<Span>()
                .ok()
                .and_then(|span| Length::Span(span).after(&start)),
            (None, None) => match &start {
                When::Day(day) => day.tomorrow().ok().map(When::Day),
                When::At(_) => None,
            },
        };
        let length = end.as_ref().and_then(|end| Length::between(&start, end));
        let data = data(event, &start, end.as_ref());
        let id = event
            .uid
            .clone()
            .unwrap_or_else(|| data_hash(&Value::Object(data.clone())));
        Some(Base {
            id,
            data,
            start,
            length,
        })
    }

    fn push(&mut self, record: Record) {
        self.upserts += 1;
        self.records.push(record);
    }
}

/// `UNTIL`, resolved.
enum Limit {
    Day(jiff::civil::Date),
    At(Timestamp),
}

/// The day of an occurrence, in the time zone its rule is expanded in.
fn key_day(key: Key, zone: &TimeZone) -> jiff::civil::Date {
    match key {
        Key::Day(day) => day,
        Key::At(ts) => ts.to_zoned(zone.clone()).date(),
    }
}

/// An event's data, identifier, start and length.
struct Base {
    id: String,
    data: Map<String, Value>,
    start: When,
    length: Option<Length>,
}

/// How long an event lasts, to place the end of each occurrence.
#[derive(Debug, Clone)]
enum Length {
    Days(i64),
    Span(Span),
}

impl Length {
    fn between(start: &When, end: &When) -> Option<Self> {
        match (start, end) {
            (When::Day(a), When::Day(b)) => a
                .until(*b)
                .ok()
                .map(|s| Self::Days(i64::from(s.get_days()))),
            (When::At(a), When::At(b)) => a.until((Unit::Day, b)).ok().map(Self::Span),
            _ => None,
        }
    }

    fn after(&self, start: &When) -> Option<When> {
        match (self, start) {
            (Self::Days(days), When::Day(day)) => day.checked_add(days.days()).ok().map(When::Day),
            (Self::Span(span), When::Day(day)) => {
                day.checked_add(span.get_days().days()).ok().map(When::Day)
            }
            (Self::Span(span), When::At(at)) => at.checked_add(*span).ok().map(When::At),
            (Self::Days(days), When::At(at)) => at.checked_add(days.days()).ok().map(When::At),
        }
    }
}

/// `UID#2026-11-14T19:00:00Z`.
fn occurrence_id(uid: &str, key: Key) -> String {
    format!("{uid}#{}", key.text())
}

fn upsert(external_id: String, data: Map<String, Value>, observed_at: Timestamp) -> Record {
    Record {
        kind: KIND.to_owned(),
        op: Op::Upsert,
        external_id: Some(external_id),
        observed_at,
        data: Some(Value::Object(data)),
    }
}

fn occurrence(uid: &str, key: Key, mut data: Map<String, Value>, observed_at: Timestamp) -> Record {
    data.remove("rule");
    data.insert("uid".into(), json!(uid));
    data.insert("recurrence_id".into(), json!(key.text()));
    upsert(occurrence_id(uid, key), data, observed_at)
}

/// The `data` of a `calendar.event` record. Empty fields are left out.
fn data(event: &Event, start: &When, end: Option<&When>) -> Map<String, Value> {
    let mut map = Map::new();
    let text = [
        ("uid", &event.uid),
        ("title", &event.summary),
        ("location", &event.location),
        ("description", &event.description),
        ("status", &event.status),
        ("rule", &event.rule),
    ];
    map.insert("start".into(), json!(start.text()));
    if let Some(end) = end {
        map.insert("end".into(), json!(end.text()));
    }
    if let Some(name) = event
        .start
        .as_ref()
        .and_then(|s| s.tzid.as_deref())
        .and_then(|_| match start {
            When::At(zoned) => zoned.time_zone().iana_name(),
            When::Day(_) => None,
        })
    {
        map.insert("time_zone".into(), json!(name));
    }
    for (key, value) in text {
        if let Some(value) = value {
            map.insert(key.into(), json!(value));
        }
    }
    if let Some(organizer) = &event.organizer {
        map.insert("organizer".into(), person(organizer));
    }
    if !event.attendees.is_empty() {
        let attendees: Vec<Value> = event.attendees.iter().map(person).collect();
        map.insert("attendees".into(), Value::Array(attendees));
    }
    map
}

fn person(p: &Person) -> Value {
    let mut map = Map::new();
    for (key, value) in [
        ("name", &p.name),
        ("email", &p.email),
        ("response", &p.response),
    ] {
        if let Some(value) = value {
            map.insert(key.into(), json!(value));
        }
    }
    Value::Object(map)
}

#[cfg(test)]
mod tests;
