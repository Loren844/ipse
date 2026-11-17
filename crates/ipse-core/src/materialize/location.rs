//! `location.visit` records → places, visits and the `location` state log (issue #106,
//! `docs/connectors/location.md`).
//!
//! - **Places.** A visited place is found again by Google's identifier (in the place
//!   file's `sources`), else by its position: an existing place within [`NEAR`] meters
//!   is the same one, whoever wrote it. Otherwise a `places/` file is created, in the
//!   `location` category (closed to AIs by default), named as the export says, or
//!   "Domicile" / "Travail", or by its position.
//! - **Visits.** A stay at home or at work is not an event: it only counts in the state
//!   log. Another stay that overlaps an event of the agenda gives that event its place,
//!   when it has none or only an address. Any other stay becomes an event of the
//!   `location` category, titled with the place's name.
//! - **State log.** `states/location/<year>.md` holds one row per place: first and last
//!   day seen and how many stays, rewritten from every visit of the year.
//! - Points stay in the journal: they are never written to the vault.

use std::collections::{BTreeMap, HashMap};
use std::path::{Path, PathBuf};

use jiff::Timestamp;
use jiff::tz::TimeZone;
use serde_json::Value;

use super::{Ledger, entity_files, versions};
use crate::journal::Journal;
use crate::vault::state::{StateLogFields, StateRow, Subject, Topic, render_table};
use crate::vault::{
    Category, Change, Common, DateTime, EntityFields, EventFields, EventTime, FileType, Geo, Id,
    Language, LinkOrText, MemoryFile, Seen, Vault, WikiLink, naming, sync_files,
};
use crate::{Error, Result};

/// The record kinds this module reads.
pub const VISIT: &str = "location.visit";
/// Home and work, as Google marked them.
pub const PLACE: &str = "location.place";

/// Two positions this close, in meters, are the same place.
pub const NEAR: f64 = 75.0;

/// The category of what location history adds.
const CATEGORY: &str = "location";

/// What an import changed in the vault.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct LocationSummary {
    /// Visits read from the journal that were new or changed.
    pub visits: u64,
    /// Place files created.
    pub places_added: u64,
    /// Existing places a visit was found at.
    pub places_found: u64,
    /// Visit events created or updated.
    pub events: u64,
    /// Agenda events that got the place they happened at.
    pub linked: u64,
    /// Stays at home or at work, counted in the state log only.
    pub home_or_work: u64,
}

/// A visit, read from its record.
#[derive(Debug, Clone)]
struct Visit {
    start: Timestamp,
    end: Timestamp,
    lat: f64,
    lon: f64,
    place_id: Option<String>,
    name: Option<String>,
    address: Option<String>,
    semantic: Option<String>,
}

impl Visit {
    fn of(data: &Value) -> Option<Self> {
        let text = |k: &str| data.get(k).and_then(Value::as_str).map(str::to_owned);
        let start: Timestamp = text("start")?.parse().ok()?;
        let end: Timestamp = text("end")?.parse().ok()?;
        Some(Self {
            start,
            end: end.max(start),
            lat: data.get("lat")?.as_f64()?,
            lon: data.get("lon")?.as_f64()?,
            place_id: text("place_id"),
            name: text("name"),
            address: text("address"),
            semantic: text("semantic"),
        })
    }
}

/// A place file of the vault, as far as places are matched.
#[derive(Debug, Clone)]
struct Place {
    path: PathBuf,
    stem: String,
    name: String,
    /// Its position, when its file has one.
    at: Option<(f64, f64)>,
    ids: Vec<String>,
}

/// An event file of the vault, as far as visits are matched.
#[derive(Debug, Clone)]
struct Event {
    path: PathBuf,
    start: Timestamp,
    end: Timestamp,
    /// The visit it was made from, for a visit event.
    visit: Option<String>,
    /// True when it already links to a place.
    linked: bool,
    /// Its place as the calendar wrote it, when it does not link to a file.
    address: Option<String>,
    /// The place file it links to.
    place: Option<String>,
}

/// Distance in meters between two positions (haversine).
#[must_use]
pub fn meters(a: (f64, f64), b: (f64, f64)) -> f64 {
    let (lat1, lon1) = (a.0.to_radians(), a.1.to_radians());
    let (lat2, lon2) = (b.0.to_radians(), b.1.to_radians());
    let h = ((lat2 - lat1) / 2.0).sin().powi(2)
        + lat1.cos() * lat2.cos() * ((lon2 - lon1) / 2.0).sin().powi(2);
    2.0 * 6_371_000.0 * h.sqrt().asin()
}

/// Turns the new or changed visits of `instance` into places, events and the state
/// log, and records every file written in `change`.
///
/// # Errors
///
/// Journal and vault errors: a file that cannot be read, parsed or written.
pub fn location_to_vault(
    vault: &Vault,
    journal: &Journal,
    instance: &str,
    change: &mut Change<'_>,
) -> Result<LocationSummary> {
    let ledger = Ledger::read(vault.root())?;
    let applied = ledger.applied(instance).map(str::to_owned);
    let labels = labels(journal, instance)?;
    let (items, last) = versions(journal, instance, VISIT, applied.as_deref())?;
    let mut places = Places::read(vault, instance)?;
    let mut events = read_events(vault.root(), instance)?;
    let zone = vault.settings().time_zone().clone();
    let language = vault.settings().language();
    let mut summary = LocationSummary::default();
    let mut written: Vec<PathBuf> = Vec::new();
    // Every visit of the vault, for the state log: place stem → stays.
    let mut stays: BTreeMap<i16, HashMap<String, Vec<Timestamp>>> = BTreeMap::new();

    for (key, versions) in &items {
        let Some(visit) = versions.latest.as_ref().and_then(Visit::of) else {
            continue;
        };
        let fresh = versions.applied != versions.latest;
        let semantic = visit.semantic.clone().or_else(|| {
            visit
                .place_id
                .as_ref()
                .and_then(|id| labels.get(id).cloned())
        });
        // An event of the agenda at the same time: it gets the place, and its address
        // names a place the export does not name.
        let overlapping = events
            .iter()
            .position(|e| e.visit.is_none() && overlaps(e.start, e.end, visit.start, visit.end));
        let said = overlapping.and_then(|at| events[at].address.clone());
        let linked = overlapping.and_then(|at| events[at].place.clone());
        let place = places.find_or_create(
            vault,
            &visit,
            semantic.as_deref(),
            said.as_deref(),
            linked.as_deref(),
            language,
            &mut summary,
            fresh,
            &mut written,
        )?;
        let Some(place) = place else {
            continue;
        };
        let year = visit.start.to_zoned(zone.clone()).year();
        stays
            .entry(year)
            .or_default()
            .entry(place.stem.clone())
            .or_default()
            .push(visit.start);
        if !fresh {
            continue;
        }
        summary.visits += 1;
        if matches!(semantic.as_deref(), Some("home" | "work")) {
            summary.home_or_work += 1;
            continue;
        }
        let target = Target {
            stem: &place.stem,
            name: &place.name,
            source: format!("{instance}:{key}"),
        };
        record_visit(
            vault,
            &zone,
            &mut events,
            overlapping,
            &target,
            &visit,
            &mut summary,
            &mut written,
        )?;
    }

    for (year, by_place) in &stays {
        if let Some(path) = state_log(vault, *year, by_place, &zone, instance)? {
            written.push(path);
        }
    }
    sync_files(&written)?;
    for path in &written {
        change.touched(path);
    }
    ledger.save(instance, last)?;
    Ok(summary)
}

/// The instant an event time stands for, to compare times written with other offsets.
trait Instant {
    fn timestamp_like(&self) -> Option<Timestamp>;
}

impl Instant for EventTime {
    fn timestamp_like(&self) -> Option<Timestamp> {
        match self {
            Self::At(at) => Some(at.timestamp()),
            Self::Day(_) => None,
        }
    }
}

fn category() -> Result<Category> {
    Category::new(CATEGORY).ok_or_else(|| Error::Refused("bad category".to_owned()))
}

/// Home and work, by Google's place identifier.
fn labels(journal: &Journal, instance: &str) -> Result<HashMap<String, String>> {
    let mut labels = HashMap::new();
    for record in journal.read(instance, Some(&[PLACE]))? {
        let record = record?;
        let Some(data) = record.data else { continue };
        let (Some(id), Some(label)) = (
            data.get("place_id").and_then(Value::as_str),
            data.get("label").and_then(Value::as_str),
        ) else {
            continue;
        };
        labels.insert(id.to_owned(), label.to_owned());
    }
    Ok(labels)
}

/// True when the two periods share at least half of the shorter one.
fn overlaps(a_start: Timestamp, a_end: Timestamp, b_start: Timestamp, b_end: Timestamp) -> bool {
    let start = a_start.max(b_start);
    let end = a_end.min(b_end);
    if end <= start {
        return false;
    }
    let shared = end.as_second() - start.as_second();
    let shorter =
        (a_end.as_second() - a_start.as_second()).min(b_end.as_second() - b_start.as_second());
    shared * 2 >= shorter.max(1)
}

/// The name of a place made from a visit.
fn place_name(visit: &Visit, semantic: Option<&str>, language: Language) -> String {
    if let Some(name) = visit.name.as_deref().filter(|n| !n.trim().is_empty()) {
        return name.trim().to_owned();
    }
    let french = language == Language::Fr;
    match semantic {
        Some("home") => if french { "Domicile" } else { "Home" }.to_owned(),
        Some("work") => if french { "Travail" } else { "Work" }.to_owned(),
        _ => {
            let at = format!("{:.4}, {:.4}", visit.lat, visit.lon);
            if french {
                format!("Lieu près de {at}")
            } else {
                format!("Place near {at}")
            }
        }
    }
}

fn create_dir(path: &Path) -> Result<()> {
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir).map_err(|source| Error::Io {
            action: "create",
            path: dir.to_owned(),
            source,
        })?;
    }
    Ok(())
}

/// The place files of the vault with a position.
struct Places {
    instance: String,
    places: Vec<Place>,
}

impl Places {
    fn read(vault: &Vault, instance: &str) -> Result<Self> {
        let mut places = Vec::new();
        for path in entity_files(&vault.root().join(FileType::Place.folder()), false)? {
            // A file that cannot be read is skipped, and never overwritten.
            let Ok(file) = MemoryFile::read(&path) else {
                continue;
            };
            let at = file
                .entity()
                .and_then(|e| e.geo)
                .map(|g| (g.lat(), g.lon()));
            let prefix = format!("{instance}:place:");
            places.push(Place {
                name: file
                    .entity()
                    .and_then(|e| e.name.clone())
                    .unwrap_or_else(|| stem(&path)),
                stem: stem(&path),
                at,
                ids: file
                    .common()
                    .sources
                    .iter()
                    .filter_map(|s| s.strip_prefix(&prefix).map(str::to_owned))
                    .collect(),
                path,
            });
        }
        Ok(Self {
            instance: instance.to_owned(),
            places,
        })
    }

    /// The place of `visit`: found by Google's identifier, then by position; created
    /// when `fresh` and none is near.
    #[allow(clippy::too_many_arguments)] // what a visit's place is found or named from
    fn find_or_create(
        &mut self,
        vault: &Vault,
        visit: &Visit,
        semantic: Option<&str>,
        said: Option<&str>,
        linked: Option<&str>,
        language: Language,
        summary: &mut LocationSummary,
        fresh: bool,
        written: &mut Vec<PathBuf>,
    ) -> Result<Option<Place>> {
        let id = visit
            .place_id
            .clone()
            .unwrap_or_else(|| format!("{:.5},{:.5}", visit.lat, visit.lon));
        if let Some(place) = self.places.iter().find(|p| p.ids.contains(&id)) {
            return Ok(Some(place.clone()));
        }
        let near = self
            .places
            .iter()
            .filter_map(|p| Some((meters(p.at?, (visit.lat, visit.lon)), p)))
            .filter(|(d, _)| *d <= NEAR)
            .min_by(|a, b| a.0.total_cmp(&b.0))
            .map(|(_, p)| p.path.clone())
            // The place the agenda event at the same time links to.
            .or_else(|| {
                let stem = linked?;
                self.places
                    .iter()
                    .find(|p| p.stem == stem)
                    .map(|p| p.path.clone())
            });
        if let Some(path) = near {
            // Found: remember Google's identifier, to find it again, and the position
            // when the file had none.
            let mut file = MemoryFile::read(&path)?;
            file.common_mut()
                .sources
                .push(format!("{}:place:{id}", self.instance));
            if let Some(entity) = file.entity_mut()
                && entity.geo.is_none()
            {
                entity.geo = Geo::new(visit.lat, visit.lon);
            }
            file.common_mut().updated = DateTime::now(vault.settings().time_zone());
            if file.write_unsynced()? {
                written.push(path.clone());
            }
            let place = self
                .places
                .iter_mut()
                .find(|p| p.path == path)
                .ok_or_else(|| Error::Refused("a place disappeared".to_owned()))?;
            place.ids.push(id);
            place.at.get_or_insert((visit.lat, visit.lon));
            if fresh {
                summary.places_found += 1;
            }
            return Ok(Some(place.clone()));
        }
        // A place the person deleted is not made again for an old visit.
        if !fresh {
            return Ok(None);
        }
        let zone = vault.settings().time_zone();
        let mut common = Common::new(FileType::Place, zone)
            .ok_or_else(|| Error::Refused("cannot create a place file".to_owned()))?;
        common.category = category()?;
        common.sources = vec![format!("{}:place:{id}", self.instance)];
        let name = match said.filter(|_| visit.name.is_none() && semantic.is_none()) {
            Some(address) => address.to_owned(),
            None => place_name(visit, semantic, language),
        };
        let path = naming::entity_path(vault.root(), FileType::Place, Some(&name), &common.id)?;
        create_dir(&path)?;
        let entity = EntityFields {
            name: Some(name.clone()),
            address: visit.address.clone().or_else(|| said.map(str::to_owned)),
            geo: Geo::new(visit.lat, visit.lon),
            ..EntityFields::default()
        };
        MemoryFile::new(&path, common, Some(entity), String::new()).write_unsynced()?;
        written.push(path.clone());
        summary.places_added += 1;
        let place = Place {
            name,
            stem: stem(&path),
            at: Some((visit.lat, visit.lon)),
            ids: vec![id],
            path,
        };
        self.places.push(place.clone());
        Ok(Some(place))
    }
}

fn stem(path: &Path) -> String {
    path.file_stem()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_default()
}

/// The events of the vault with a start and an end in time (not whole days), and the
/// visit each was made from.
fn read_events(root: &Path, instance: &str) -> Result<Vec<Event>> {
    let mut events = Vec::new();
    let prefix = format!("{instance}:visit:");
    for path in entity_files(&root.join(FileType::Event.folder()), true)? {
        let Ok(file) = MemoryFile::read(&path) else {
            continue;
        };
        let Some(fields) = file.event() else { continue };
        let (EventTime::At(start), end) = (&fields.start, &fields.end) else {
            continue;
        };
        let start = start.timestamp();
        let end = match end {
            Some(EventTime::At(end)) => end.timestamp(),
            // A point in time counts as an hour.
            _ => start
                .checked_add(jiff::SignedDuration::from_hours(1))
                .unwrap_or(start),
        };
        let visit = file
            .common()
            .sources
            .iter()
            .find(|s| s.starts_with(&prefix))
            .cloned();
        events.push(Event {
            linked: fields.place.as_ref().and_then(LinkOrText::link).is_some(),
            address: match &fields.place {
                Some(LinkOrText::Text(text)) if !text.trim().is_empty() => {
                    Some(text.trim().to_owned())
                }
                _ => None,
            },
            place: fields
                .place
                .as_ref()
                .and_then(LinkOrText::link)
                .map(|l| l.target.to_ascii_lowercase()),
            path,
            start,
            end,
            visit,
        });
    }
    Ok(events)
}

/// The place of one visit, as [`record_visit`] needs it.
struct Target<'a> {
    stem: &'a str,
    name: &'a str,
    /// `<instance>:<record key>`, in the `sources` of the visit's event.
    source: String,
}

/// Writes one new or changed visit that is not at home or at work: the overlapping
/// agenda event (`overlapping`, an index in `events`) gets the place, else the visit's
/// own event is updated or created. Files written are pushed to `written`.
#[allow(clippy::too_many_arguments)] // the state of one import, threaded through
fn record_visit(
    vault: &Vault,
    zone: &TimeZone,
    events: &mut Vec<Event>,
    overlapping: Option<usize>,
    target: &Target<'_>,
    visit: &Visit,
    summary: &mut LocationSummary,
    written: &mut Vec<PathBuf>,
) -> Result<()> {
    let link = WikiLink {
        target: target.stem.to_owned(),
        display: None,
    };
    let source = target.source.clone();
    if let Some(event) = overlapping.map(|at| &mut events[at]) {
        if !event.linked {
            let mut file = MemoryFile::read(&event.path)?;
            if let Some(fields) = file.event_mut()
                && fields.place.as_ref().and_then(LinkOrText::link).is_none()
            {
                fields.place = Some(LinkOrText::Link(link));
                file.common_mut().updated = DateTime::now(zone);
                if file.write_unsynced()? {
                    written.push(event.path.clone());
                }
                summary.linked += 1;
            }
            event.linked = true;
        }
        return Ok(());
    }
    let name = target.name.to_owned();
    let at = |t: Timestamp| EventTime::At(DateTime::at(t, zone));
    if let Some(event) = events
        .iter_mut()
        .find(|e| e.visit.as_deref() == Some(source.as_str()))
    {
        let mut file = MemoryFile::read(&event.path)?;
        let Some(fields) = file.event_mut() else {
            return Ok(());
        };
        let place = Some(LinkOrText::Link(link));
        let same = fields.start.timestamp_like() == at(visit.start).timestamp_like()
            && fields.end.map(|e| e.timestamp_like()) == Some(at(visit.end).timestamp_like())
            && fields.place == place;
        if !same {
            fields.start = at(visit.start);
            fields.end = Some(at(visit.end));
            fields.place = place;
            file.common_mut().updated = DateTime::now(zone);
            file.write_unsynced()?;
            written.push(event.path.clone());
            summary.events += 1;
        }
        event.start = visit.start;
        event.end = visit.end;
        return Ok(());
    }
    let mut common = Common::new(FileType::Event, zone)
        .ok_or_else(|| Error::Refused("cannot create an event file".to_owned()))?;
    common.category = category()?;
    common.sources = vec![source.clone()];
    let start = at(visit.start);
    let path = naming::event_path(vault.root(), &name, &start, &common.id)?;
    create_dir(&path)?;
    let mut fields = EventFields::new(name, start);
    fields.end = Some(at(visit.end));
    fields.place = Some(LinkOrText::Link(link));
    MemoryFile::new_event(&path, common, fields, String::new()).write_unsynced()?;
    events.push(Event {
        path: path.clone(),
        start: visit.start,
        end: visit.end,
        visit: Some(source),
        linked: true,
        address: None,
        place: None,
    });
    written.push(path);
    summary.events += 1;
    Ok(())
}

/// Rewrites `states/location/<year>.md` from the stays of that year; returns its path
/// when it changed.
fn state_log(
    vault: &Vault,
    year: i16,
    by_place: &HashMap<String, Vec<Timestamp>>,
    zone: &TimeZone,
    instance: &str,
) -> Result<Option<PathBuf>> {
    let topic = Topic::new("location").ok_or_else(|| Error::Refused("bad topic".to_owned()))?;
    let path = naming::state_log_path(vault.root(), &topic, year);
    let day = |t: &Timestamp| Seen::Day(t.to_zoned(zone.clone()).date());
    let rows: Vec<StateRow> = by_place
        .iter()
        .filter_map(|(stem, stays)| {
            let first = stays.iter().min()?;
            let last = stays.iter().max()?;
            Some(StateRow {
                subject: Subject::Link(WikiLink {
                    target: stem.clone(),
                    display: None,
                }),
                first_seen: day(first),
                last_seen: day(last),
                count: stays.len() as u64,
                source: instance.to_owned(),
            })
        })
        .collect();
    let body = render_table(&rows);
    let mut file = match MemoryFile::read(&path) {
        Ok(file) => file,
        Err(Error::Io { source, .. }) if source.kind() == std::io::ErrorKind::NotFound => {
            let now = DateTime::now(zone);
            let common = Common {
                id: Id::generate(),
                file_type: FileType::StateLog,
                category: category()?,
                created: now,
                updated: now,
                sources: Vec::new(),
            };
            create_dir(&path)?;
            MemoryFile::new_state_log(&path, common, StateLogFields { topic, year }, String::new())
        }
        Err(e) => return Err(e),
    };
    if file.body() == body {
        return Ok(None);
    }
    file.set_body(body);
    file.common_mut().updated = DateTime::now(zone);
    Ok(file.write_unsynced()?.then_some(path))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn distances_and_overlaps() {
        let bastille = (48.8532, 2.3692);
        let nearby = (48.8536, 2.3696);
        assert!(meters(bastille, nearby) < NEAR);
        assert!(meters(bastille, (48.8584, 2.2945)) > 5_000.0);
        let t = |h: i64| Timestamp::from_second(h * 3600).unwrap();
        assert!(overlaps(t(20), t(23), t(20), t(22)));
        assert!(!overlaps(t(20), t(21), t(21), t(22)));
        assert!(!overlaps(t(10), t(20), t(19), t(23)), "an hour of four");
    }
}
