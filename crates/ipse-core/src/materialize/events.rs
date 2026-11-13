//! `calendar.event` records → `events/` files.
//!
//! - An event is found again by its calendar identifier, whatever the calendar it came
//!   from: the same invitation in two calendars is one file with two sources (decision
//!   0011, events).
//! - A repeating series is not a file: each of its occurrences is.
//! - Guests are linked to a person only when their email matches exactly one person
//!   file; otherwise they stay plain text. No person file is ever changed here.
//! - A removed event that has not happened yet is marked cancelled; past events and
//!   person files are left alone.
//! - Ipse refreshes only the keys it took from the calendar; the body is written once.

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};

use jiff::Timestamp;
use serde_json::Value;

use super::{Ledger, Merge, Outcome, entity_files, versions};
use crate::Result;
use crate::journal::Journal;
use crate::vault::{
    Body, Change, Common, EventFields, EventStatus, EventTime, FileType, LinkOrText, MemoryFile,
    Vault, body, naming, sync_files,
};

/// The record kind this module reads.
pub const KIND: &str = "calendar.event";

/// What an import changed in the vault.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct EventsSummary {
    /// Event files created.
    pub added: u64,
    /// Event files updated, including a source added from another calendar.
    pub updated: u64,
    /// Events already up to date.
    pub unchanged: u64,
    /// Events removed from the calendar before they happened, marked cancelled.
    pub cancelled: u64,
    /// Events whose title, times, place, guests or status the person changed, left as
    /// they are although the calendar changed.
    pub conflicts: u64,
    /// Event files the person deleted, not created again.
    pub deleted: u64,
    /// Records that cannot become an event (no title or no start), ignored.
    pub skipped: u64,
}

/// Creates or updates one `events/` file per event of `instance`, and records every
/// file written in `change`. `now` decides which removed events have not happened yet.
///
/// # Errors
///
/// Journal and vault errors: a file that cannot be read, parsed or written.
pub fn calendar_to_events(
    vault: &Vault,
    journal: &Journal,
    instance: &str,
    change: &mut Change<'_>,
    now: Timestamp,
) -> Result<EventsSummary> {
    let ledger = Ledger::read(vault.root())?;
    let (items, last) = versions(journal, instance, KIND, ledger.applied(instance))?;
    let existing = events_by_external_id(vault.root())?;
    let people = People::read(vault)?;
    let mut summary = EventsSummary::default();
    let mut written = Vec::new();

    for (external_id, versions) in items {
        let source = format!("{instance}:{external_id}");
        let path = existing.get(&external_id);
        let previous = versions
            .applied
            .as_ref()
            .and_then(|data| EventRecord::from_data(data, &people));
        let previous = previous.as_ref().map(|p| (p, versions.applied_removal));
        let Some(data) = &versions.latest else {
            if let Some(path) = path {
                let outcome = cancel(path, previous, vault, now)?;
                if outcome.written {
                    change.touched(path);
                    written.push(path.clone());
                    summary.cancelled += 1;
                }
                if outcome.conflict {
                    summary.conflicts += 1;
                }
            }
            continue;
        };
        if data.get("rule").is_some() {
            // A series: its occurrences come as their own records.
            continue;
        }
        let Some(record) = EventRecord::from_data(data, &people) else {
            summary.skipped += 1;
            continue;
        };
        if let Some(path) = path {
            let outcome = update(path, &record, previous, &source, vault)?;
            if outcome.written {
                change.touched(path);
                written.push(path.clone());
                summary.updated += 1;
            } else if !outcome.conflict {
                summary.unchanged += 1;
            }
            if outcome.conflict {
                summary.conflicts += 1;
            }
        } else if versions.materialized {
            // The person deleted the file Ipse made: it stays deleted.
            summary.deleted += 1;
        } else {
            let path = create(&record, source, vault)?;
            change.touched(&path);
            written.push(path.clone());
            summary.added += 1;
        }
    }
    // Made durable before the ledger says they are in the vault.
    sync_files(&written)?;
    ledger.save(instance, last)?;
    Ok(summary)
}

/// Marks a removed event cancelled when it has not happened yet, unless the person
/// gave it a status of their own.
fn cancel(
    path: &Path,
    previous: Option<(&EventRecord, bool)>,
    vault: &Vault,
    now: Timestamp,
) -> Result<Outcome> {
    let mut file = MemoryFile::read(path)?;
    let Some(event) = file.event() else {
        return Ok(Outcome::default());
    };
    if event.status == Some(EventStatus::Cancelled) || !is_future(&event.start, now, vault) {
        return Ok(Outcome::default());
    }
    let applied = previous.map(|(p, removal)| p.status(removal, event.status));
    if applied.is_some_and(|a| a != event.status) {
        return Ok(Outcome {
            written: false,
            conflict: true,
        });
    }
    if let Some(event) = file.event_mut() {
        event.status = Some(EventStatus::Cancelled);
    }
    file.common_mut().updated = crate::vault::DateTime::now(vault.settings().time_zone());
    file.write_unsynced()?;
    Ok(Outcome {
        written: true,
        conflict: false,
    })
}

fn is_future(start: &EventTime, now: Timestamp, vault: &Vault) -> bool {
    match start {
        EventTime::At(at) => at.timestamp() > now,
        EventTime::Day(day) => *day > now.to_zoned(vault.settings().time_zone().clone()).date(),
    }
}

/// Refreshes the keys taken from the calendar, one by one (a key the person changed
/// keeps their value), and adds `source` if missing.
fn update(
    path: &Path,
    record: &EventRecord,
    previous: Option<(&EventRecord, bool)>,
    source: &str,
    vault: &Vault,
) -> Result<Outcome> {
    let mut file = MemoryFile::read(path)?;
    let Some(current) = file.event().cloned() else {
        return Ok(Outcome::default());
    };
    let (wanted, conflict) = record.merge(&current, previous);
    let has_source = file.common().sources.iter().any(|s| s == source);
    if wanted == current && has_source {
        return Ok(Outcome {
            written: false,
            conflict,
        });
    }
    if let Some(event) = file.event_mut() {
        *event = wanted;
    }
    let common = file.common_mut();
    if !has_source {
        common.sources.push(source.to_owned());
    }
    common.updated = crate::vault::DateTime::now(vault.settings().time_zone());
    file.write_unsynced()?;
    Ok(Outcome {
        written: true,
        conflict,
    })
}

/// Writes a new event file. Returns its path.
fn create(record: &EventRecord, source: String, vault: &Vault) -> Result<PathBuf> {
    let settings = vault.settings();
    let mut common = Common::new(FileType::Event, settings.time_zone())
        .ok_or_else(|| crate::Error::Refused("cannot create an event file".to_owned()))?;
    common.sources = vec![source];
    let event = record.fields();
    let path = naming::event_path(vault.root(), &record.title, &record.start, &common.id)?;
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir).map_err(|source| crate::Error::Io {
            action: "create",
            path: dir.to_owned(),
            source,
        })?;
    }
    let body = Body {
        facts: Vec::new(),
        relations: Vec::new(),
        notes: record.description.clone(),
    };
    let body = body::render(&body, settings.language());
    MemoryFile::new_event(&path, common, event, body).write_unsynced()?;
    Ok(path)
}

/// Event files by the calendar identifier in their sources (`calendar-2:<id>` gives
/// `<id>`), whatever the calendar.
fn events_by_external_id(root: &Path) -> Result<HashMap<String, PathBuf>> {
    let mut by_id = HashMap::new();
    for path in entity_files(&root.join(FileType::Event.folder()), true)? {
        // A file that cannot be read is skipped, and never overwritten.
        let Ok(file) = MemoryFile::read(&path) else {
            continue;
        };
        for source in &file.common().sources {
            if let Some((connector, id)) = source.split_once(':')
                && is_calendar(connector)
            {
                by_id.insert(id.to_owned(), path.clone());
            }
        }
    }
    Ok(by_id)
}

/// `calendar`, `calendar-2`…
fn is_calendar(instance: &str) -> bool {
    instance == "calendar"
        || instance
            .strip_prefix("calendar-")
            .is_some_and(|n| !n.is_empty() && n.bytes().all(|b| b.is_ascii_digit()))
}

/// Person files by email, to link guests; the owner's emails, to leave them out.
struct People {
    by_email: HashMap<String, Vec<String>>,
    owner: HashSet<String>,
}

impl People {
    fn read(vault: &Vault) -> Result<Self> {
        let mut by_email: HashMap<String, Vec<String>> = HashMap::new();
        let mut owner = HashSet::new();
        for path in entity_files(&vault.root().join(FileType::Person.folder()), false)? {
            let Ok(file) = MemoryFile::read(&path) else {
                continue;
            };
            let (Some(entity), Some(stem)) = (file.entity(), path.file_stem()) else {
                continue;
            };
            let stem = stem.to_string_lossy().into_owned();
            for email in &entity.emails {
                let email = email.trim().to_lowercase();
                if entity.owner {
                    owner.insert(email);
                } else {
                    by_email.entry(email).or_default().push(stem.clone());
                }
            }
        }
        Ok(Self { by_email, owner })
    }

    /// A guest: linked when their email matches exactly one person, left out when it
    /// is the owner's, plain text otherwise.
    fn guest(&self, name: Option<&str>, email: Option<&str>) -> Option<LinkOrText> {
        let email = email.map(|e| e.trim().to_lowercase());
        if let Some(email) = &email {
            if self.owner.contains(email) {
                return None;
            }
            if let Some([stem]) = self.by_email.get(email).map(Vec::as_slice) {
                return LinkOrText::parse(&format!("[[{stem}]]")).into();
            }
        }
        name.map(str::to_owned)
            .or(email)
            .filter(|t| !t.is_empty())
            .map(LinkOrText::Text)
    }
}

/// What Ipse takes from a `calendar.event` record.
struct EventRecord {
    title: String,
    start: EventTime,
    end: Option<EventTime>,
    location: Option<String>,
    participants: Vec<LinkOrText>,
    /// The same guests as plain text, as written before anyone was linked.
    guests: Vec<LinkOrText>,
    cancelled: bool,
    description: Option<String>,
}

impl EventRecord {
    fn from_data(data: &Value, people: &People) -> Option<Self> {
        let text = |key: &str| {
            data.get(key)
                .and_then(Value::as_str)
                .map(str::trim)
                .filter(|s| !s.is_empty())
                .map(str::to_owned)
        };
        let start = EventTime::parse(&text("start")?)?;
        let mut participants: Vec<LinkOrText> = Vec::new();
        let mut guests: Vec<LinkOrText> = Vec::new();
        let all = data.get("organizer").into_iter().chain(
            data.get("attendees")
                .and_then(Value::as_array)
                .into_iter()
                .flatten(),
        );
        for guest in all {
            let field = |key: &str| guest.get(key).and_then(Value::as_str);
            if let Some(p) = people.guest(field("name"), field("email"))
                && !participants.contains(&p)
            {
                let text = field("name")
                    .or(field("email"))
                    .map_or_else(|| p.clone(), |t| LinkOrText::Text(t.trim().to_owned()));
                participants.push(p);
                guests.push(text);
            }
        }
        Some(Self {
            title: text("title")?,
            start,
            end: text("end").as_deref().and_then(EventTime::parse),
            location: text("location"),
            participants,
            guests,
            cancelled: text("status").as_deref() == Some("cancelled"),
            description: text("description"),
        })
    }

    /// A new event's keys.
    fn fields(&self) -> EventFields {
        let mut event = EventFields::new(self.title.clone(), self.start);
        event.end = self.end;
        event.place = self.location.clone().map(LinkOrText::Text);
        event.participants.clone_from(&self.participants);
        event.status = self.status(false, None);
        event
    }

    /// The status Ipse writes: cancelled when the calendar says so or the event was
    /// removed; otherwise none. After a removal, `current` may be either, depending on
    /// whether the event had happened.
    fn status(&self, removal: bool, current: Option<EventStatus>) -> Option<EventStatus> {
        if self.cancelled {
            Some(EventStatus::Cancelled)
        } else if removal {
            current.filter(|s| *s == EventStatus::Cancelled)
        } else {
            None
        }
    }

    /// `current` with the keys taken from the calendar merged in, one by one (see
    /// [`Merge`]), and whether the calendar changed a key the person had changed. A
    /// place the person linked to a file stays.
    fn merge(&self, current: &EventFields, previous: Option<(&Self, bool)>) -> (EventFields, bool) {
        let mut merge = Merge::default();
        let prev = previous.map(|(p, _)| p);
        let mut out = current.clone();
        out.title = merge.key(&current.title, prev.map(|p| &p.title), &self.title);
        out.start = merge.key(&current.start, prev.map(|p| &p.start), &self.start);
        out.end = merge.key(&current.end, prev.map(|p| &p.end), &self.end);
        if current.place.as_ref().and_then(LinkOrText::link).is_none() {
            let text = |l: &Option<String>| l.clone().map(LinkOrText::Text);
            out.place = merge.key(
                &current.place,
                prev.map(|p| text(&p.location)).as_ref(),
                &text(&self.location),
            );
        }
        // A guest written as text, then linked once their contact was imported, is not
        // a change by the person.
        let untouched = |p: &Self| {
            current.participants.len() == p.participants.len()
                && current
                    .participants
                    .iter()
                    .zip(p.participants.iter().zip(&p.guests))
                    .all(|(c, (linked, text))| c == linked || c == text)
        };
        if prev.is_none_or(untouched) {
            out.participants.clone_from(&self.participants);
        } else if !untouched(self) {
            merge.conflict = true;
        }
        let applied = previous.map(|(p, removal)| p.status(removal, current.status));
        out.status = merge.key(&current.status, applied.as_ref(), &self.status(false, None));
        (out, merge.conflict)
    }
}
