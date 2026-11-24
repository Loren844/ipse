//! `home.presence` records → the `home` state log (issue #225,
//! `docs/connectors/homeassistant.md`).
//!
//! `states/home/<year>.md` holds one row per place the person was: at home, or in a
//! zone of their Home Assistant ("Travail", "Salle de sport"). First seen is when the
//! first stay of the year began, last seen when the last one ended, or the last sync
//! for a stay going on, and the count is how many stays. Being away is not a row. The
//! file is in the `home` category, closed to AIs until the person opens it, and is
//! rewritten from every stay at each sync.

use std::collections::{BTreeMap, HashMap};
use std::path::{Path, PathBuf};

use jiff::Timestamp;
use jiff::tz::TimeZone;
use serde_json::Value;

use super::versions;
use crate::journal::Journal;
use crate::vault::state::{StateLogFields, StateRow, Subject, Topic, render_table};
use crate::vault::{
    Category, Change, Common, DateTime, FileType, Id, Language, MemoryFile, Seen, Vault, naming,
    sync_files,
};
use crate::{Error, Result};

/// The record kind this module reads.
pub const PRESENCE: &str = "home.presence";

/// The category of what this module writes.
const CATEGORY: &str = "home";

/// States of a Home Assistant person that say where they are not.
const NOWHERE: [&str; 3] = ["not_home", "unknown", "unavailable"];

/// What the `home` state log holds after a sync.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct HomeSummary {
    /// Stays read, at home or in a zone.
    pub stays: u64,
    /// State logs rewritten, one per year.
    pub logs: u64,
}

/// One stay, from a record's data.
struct Stay {
    state: String,
    start: Timestamp,
    /// When it ended, or when it was last seen going on.
    last: Timestamp,
}

impl Stay {
    fn of(data: &Value) -> Option<Self> {
        let text = |key: &str| data.get(key).and_then(Value::as_str);
        let start: Timestamp = text("start")?.parse().ok()?;
        let last = text("end")
            .or_else(|| text("seen"))
            .and_then(|t| t.parse().ok())
            .unwrap_or(start);
        Some(Self {
            state: text("state")?.trim().to_owned(),
            start,
            last: last.max(start),
        })
    }
}

/// Rewrites `states/home/<year>.md` from every stay `instance` sent, and records each
/// file written in `change`.
///
/// # Errors
///
/// Journal and vault errors: a file that cannot be read, parsed or written.
pub fn home_to_vault(
    vault: &Vault,
    journal: &Journal,
    instance: &str,
    change: &mut Change<'_>,
) -> Result<HomeSummary> {
    let (items, _) = versions(journal, instance, PRESENCE, None)?;
    let zone = vault.settings().time_zone().clone();
    let language = vault.settings().language();
    // Year → place → stays.
    let mut years: BTreeMap<i16, HashMap<String, Vec<Stay>>> = BTreeMap::new();
    let mut summary = HomeSummary::default();
    for (_, versions) in items {
        let Some(stay) = versions.latest.as_ref().and_then(Stay::of) else {
            continue;
        };
        if stay.state.is_empty() || NOWHERE.contains(&stay.state.as_str()) {
            continue;
        }
        summary.stays += 1;
        let year = stay.start.to_zoned(zone.clone()).year();
        years
            .entry(year)
            .or_default()
            .entry(place(&stay.state, language))
            .or_default()
            .push(stay);
    }
    let mut written = Vec::new();
    for (year, places) in &years {
        if let Some(path) = state_log(vault, *year, places, &zone, instance)? {
            written.push(path);
        }
    }
    sync_files(&written)?;
    summary.logs = written.len() as u64;
    for path in &written {
        change.touched(path);
    }
    Ok(summary)
}

/// The name of the place a state says, in the vault's language: Home Assistant's
/// `home` is the person's home; any other state is the name of a zone.
fn place(state: &str, language: Language) -> String {
    match (state, language) {
        ("home", Language::Fr) => "Domicile".to_owned(),
        ("home", _) => "Home".to_owned(),
        (zone, _) => zone.replace('|', "/"),
    }
}

/// Rewrites `states/home/<year>.md` from the stays of that year; returns its path when
/// it changed.
fn state_log(
    vault: &Vault,
    year: i16,
    places: &HashMap<String, Vec<Stay>>,
    zone: &TimeZone,
    instance: &str,
) -> Result<Option<PathBuf>> {
    let topic = Topic::new(CATEGORY).ok_or_else(|| Error::Refused("bad topic".to_owned()))?;
    let path = naming::state_log_path(vault.root(), &topic, year);
    let moment = |t: Timestamp| Seen::At(DateTime::at(t, zone));
    let rows: Vec<StateRow> = places
        .iter()
        .filter_map(|(name, stays)| {
            let first = stays.iter().map(|s| s.start).min()?;
            let last = stays.iter().map(|s| s.last).max()?;
            Some(StateRow {
                subject: Subject::Text(name.clone()),
                first_seen: moment(first),
                last_seen: moment(last),
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
                category: Category::new(CATEGORY)
                    .ok_or_else(|| Error::Refused("bad category".to_owned()))?,
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_stay_ends_or_was_last_seen() {
        let ended = Stay::of(&serde_json::json!({
            "state": "home", "start": "2026-11-20T18:00:00Z", "end": "2026-11-21T08:00:00Z"
        }))
        .unwrap();
        assert_eq!(
            ended.last,
            "2026-11-21T08:00:00Z".parse::<Timestamp>().unwrap()
        );
        let going_on = Stay::of(&serde_json::json!({
            "state": "Travail", "start": "2026-11-21T08:40:00Z", "seen": "2026-11-21T09:00:00Z"
        }))
        .unwrap();
        assert_eq!(
            going_on.last,
            "2026-11-21T09:00:00Z".parse::<Timestamp>().unwrap()
        );
        assert!(Stay::of(&serde_json::json!({"state": "home"})).is_none());
    }

    #[test]
    fn places_are_named_in_the_vault_s_language() {
        assert_eq!(place("home", Language::Fr), "Domicile");
        assert_eq!(place("home", Language::En), "Home");
        assert_eq!(place("Salle de sport", Language::Fr), "Salle de sport");
        assert_eq!(place("a|b", Language::Fr), "a/b");
    }
}
