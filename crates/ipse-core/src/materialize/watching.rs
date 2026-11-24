//! `media.watched` records → the `watching` state log (issue #227,
//! `docs/connectors/jellyfin.md`).
//!
//! `states/watching/<year>.md` holds one row per series or film: the days of the first
//! and the last play of the year, and how many plays. A series is one row, whatever
//! the season. The subject links to a file of `works/` named so (by its name or an
//! alias), plain text otherwise. The file is in the `media` category, and is rewritten
//! from every play at each sync.

use std::collections::{BTreeMap, HashMap};

use jiff::Timestamp;
use serde_json::Value;

use super::states::write_state_log;
use super::{entity_files, versions};
use crate::Result;
use crate::journal::Journal;
use crate::vault::state::{StateRow, Subject};
use crate::vault::{Change, FileType, MemoryFile, Seen, Vault, WikiLink, sync_files};

/// The record kind this module reads.
pub const WATCHED: &str = "media.watched";

/// The topic and the category of what this module writes.
const TOPIC: &str = "watching";
const CATEGORY: &str = "media";

/// What the `watching` state log holds after a sync.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct WatchingSummary {
    /// Plays read, films and episodes.
    pub plays: u64,
    /// State logs rewritten, one per year.
    pub logs: u64,
}

/// Rewrites `states/watching/<year>.md` from every play `instance` sent, and records
/// each file written in `change`.
///
/// # Errors
///
/// Journal and vault errors: a file that cannot be read, parsed or written.
pub fn watching_to_vault(
    vault: &Vault,
    journal: &Journal,
    instance: &str,
    change: &mut Change<'_>,
) -> Result<WatchingSummary> {
    let (items, _) = versions(journal, instance, WATCHED, None)?;
    let zone = vault.settings().time_zone().clone();
    let works = works(vault)?;
    // Year → subject → days played.
    let mut years: BTreeMap<i16, HashMap<String, Vec<Timestamp>>> = BTreeMap::new();
    let mut summary = WatchingSummary::default();
    for (_, versions) in items {
        let Some(data) = versions.latest.as_ref() else {
            continue;
        };
        let text = |key: &str| {
            data.get(key)
                .and_then(Value::as_str)
                .map(str::trim)
                .filter(|t| !t.is_empty())
        };
        let (Some(subject), Some(at)) = (
            text("series").or_else(|| text("title")),
            text("played_at").and_then(|t| t.parse::<Timestamp>().ok()),
        ) else {
            continue;
        };
        summary.plays += 1;
        years
            .entry(at.to_zoned(zone.clone()).year())
            .or_default()
            .entry(subject.replace('|', "/"))
            .or_default()
            .push(at);
    }
    let mut written = Vec::new();
    for (year, subjects) in &years {
        let day = |t: Timestamp| Seen::Day(t.to_zoned(zone.clone()).date());
        let rows: Vec<StateRow> = subjects
            .iter()
            .filter_map(|(name, plays)| {
                let subject = works.get(&name.to_lowercase()).map_or_else(
                    || Subject::Text(name.clone()),
                    |stem| {
                        Subject::Link(WikiLink {
                            target: stem.clone(),
                            display: None,
                        })
                    },
                );
                Some(StateRow {
                    subject,
                    first_seen: day(*plays.iter().min()?),
                    last_seen: day(*plays.iter().max()?),
                    count: plays.len() as u64,
                    source: instance.to_owned(),
                })
            })
            .collect();
        if let Some(path) = write_state_log(vault, TOPIC, CATEGORY, *year, &rows, &zone)? {
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

/// The files of `works/`, by their name and aliases in lower case.
fn works(vault: &Vault) -> Result<HashMap<String, String>> {
    let mut works = HashMap::new();
    for path in entity_files(&vault.root().join(FileType::Work.folder()), true)? {
        let Ok(file) = MemoryFile::read(&path) else {
            continue;
        };
        let Some(entity) = file.entity() else {
            continue;
        };
        let stem = path
            .file_stem()
            .map(|s| s.to_string_lossy().into_owned())
            .unwrap_or_default();
        for name in entity.name.iter().chain(&entity.aliases) {
            works
                .entry(name.trim().to_lowercase())
                .or_insert_with(|| stem.clone());
        }
    }
    Ok(works)
}
