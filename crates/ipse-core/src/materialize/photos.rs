//! `photos.person` records → the `photos` state log (issue #226,
//! `docs/connectors/immich.md`).
//!
//! `states/photos/<year>.md`, in the year of the last photo, holds one row per person
//! of the photos: the days of the first and the last photo that shows them, and how many
//! photos. Who they are in the memory is never guessed (decision 0011):
//!
//! - a person of `people/` whose name or an alias is the name Immich gives, and only
//!   one, is linked;
//! - otherwise, when Immich gives a single word that is the first name of one person
//!   only, the row stays plain text, and Ipse proposes to add that word to the person's
//!   aliases: accepted, the next sync links them;
//! - anyone else stays plain text.
//!
//! The owner, on most of their own photos, is no row. The files are in the `people`
//! category, and are rewritten from every person at each sync.

use std::collections::{BTreeMap, HashMap};
use std::path::PathBuf;

use jiff::Timestamp;
use serde_json::Value;

use super::states::write_state_log;
use super::{entity_files, relative, versions};
use crate::journal::Journal;
use crate::proposals::{Author, Change as Proposed, Proposals};
use crate::vault::state::{StateRow, Subject};
use crate::vault::{
    Change, FileType, Language, MemoryFile, Seen, Vault, WikiLink, naming, sync_files,
};
use crate::{Result, vault::state::Topic};

/// The record kind this module reads.
pub const PHOTOS: &str = "photos.person";

/// The topic and the category of what this module writes.
const TOPIC: &str = "photos";
const CATEGORY: &str = "people";

/// What the `photos` state log holds after a sync.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct PhotosSummary {
    /// People of the photos read.
    pub people: u64,
    /// Of them, linked to a person of the memory.
    pub linked: u64,
    /// Proposals waiting for the person: a name to add to someone's aliases.
    pub proposals: u64,
    /// State logs rewritten, one per year.
    pub logs: u64,
}

/// A person of the memory, to find again by name.
struct Known {
    stem: String,
    path: PathBuf,
    name: Option<String>,
    aliases: Vec<String>,
    owner: bool,
}

/// The people of the memory, by their names and aliases, and by their first names, in
/// lower case.
struct People {
    known: Vec<Known>,
    by_name: HashMap<String, Vec<usize>>,
    by_first: HashMap<String, Vec<usize>>,
}

impl People {
    fn read(vault: &Vault) -> Result<Self> {
        let mut known = Vec::new();
        for path in entity_files(&vault.root().join(FileType::Person.folder()), true)? {
            let Ok(file) = MemoryFile::read(&path) else {
                continue;
            };
            let Some(entity) = file.entity() else {
                continue;
            };
            known.push(Known {
                stem: path
                    .file_stem()
                    .map(|s| s.to_string_lossy().into_owned())
                    .unwrap_or_default(),
                path,
                name: entity.name.clone(),
                aliases: entity.aliases.clone(),
                owner: entity.owner,
            });
        }
        let mut by_name: HashMap<String, Vec<usize>> = HashMap::new();
        let mut by_first: HashMap<String, Vec<usize>> = HashMap::new();
        for (at, person) in known.iter().enumerate() {
            for name in person.name.iter().chain(&person.aliases) {
                let entry = by_name.entry(fold(name)).or_default();
                if !entry.contains(&at) {
                    entry.push(at);
                }
            }
            if let Some(first) = person
                .name
                .as_deref()
                .and_then(|n| n.split_whitespace().next())
            {
                by_first.entry(fold(first)).or_default().push(at);
            }
        }
        Ok(Self {
            known,
            by_name,
            by_first,
        })
    }
}

/// What a name of Immich is in the memory.
enum Match<'a> {
    /// This person, for sure.
    Person(&'a Known),
    /// Maybe this person: their first name only.
    Maybe(&'a Known),
    /// Nobody, or several.
    None,
}

impl People {
    fn find(&self, name: &str) -> Match<'_> {
        let key = fold(name);
        if let Some(found) = self.by_name.get(&key) {
            return match found.as_slice() {
                [only] => Match::Person(&self.known[*only]),
                _ => Match::None,
            };
        }
        if !name.trim().contains(char::is_whitespace)
            && let Some([only]) = self.by_first.get(&key).map(Vec::as_slice)
        {
            return Match::Maybe(&self.known[*only]);
        }
        Match::None
    }
}

fn fold(name: &str) -> String {
    name.split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .to_lowercase()
}

/// Rewrites `states/photos/<year>.md` from every person `instance` sent, proposes the
/// aliases that would link the ones Ipse is not sure of, and records each file written
/// in `change`.
///
/// # Errors
///
/// Journal, vault and proposal errors: a file that cannot be read, parsed or written.
pub fn photos_to_vault(
    vault: &Vault,
    journal: &Journal,
    instance: &str,
    change: &mut Change<'_>,
) -> Result<PhotosSummary> {
    let (items, _) = versions(journal, instance, PHOTOS, None)?;
    let zone = vault.settings().time_zone().clone();
    let people = People::read(vault)?;
    let mut summary = PhotosSummary::default();
    let mut years: BTreeMap<i16, Vec<StateRow>> = BTreeMap::new();
    let mut maybes: Vec<(&Known, String, u64)> = Vec::new();
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
        let moment = |key: &str| text(key).and_then(|t| t.parse::<Timestamp>().ok());
        let (Some(name), Some(last)) = (text("name"), moment("last")) else {
            continue;
        };
        let first = moment("first").unwrap_or(last).min(last);
        let photos = data.get("photos").and_then(Value::as_u64).unwrap_or(1);
        let subject = match people.find(name) {
            Match::Person(person) if person.owner => continue,
            Match::Person(person) => {
                summary.linked += 1;
                Subject::Link(WikiLink {
                    target: person.stem.clone(),
                    display: None,
                })
            }
            Match::Maybe(person) => {
                if !person.owner {
                    maybes.push((person, name.to_owned(), photos));
                }
                Subject::Text(name.replace('|', "/"))
            }
            Match::None => Subject::Text(name.replace('|', "/")),
        };
        summary.people += 1;
        let day = |t: Timestamp| Seen::Day(t.to_zoned(zone.clone()).date());
        years
            .entry(last.to_zoned(zone.clone()).year())
            .or_default()
            .push(StateRow {
                subject,
                first_seen: day(first),
                last_seen: day(last),
                count: photos,
                source: instance.to_owned(),
            });
    }
    // A year whose log is there but has nobody now is emptied: its people moved to a
    // later year.
    if let Some(topic) = Topic::new(TOPIC) {
        let dir = naming::state_log_path(vault.root(), &topic, 2000)
            .parent()
            .map(std::path::Path::to_path_buf);
        if let Some(Ok(entries)) = dir.map(std::fs::read_dir) {
            for entry in entries.flatten() {
                if let Some(year) = entry
                    .path()
                    .file_stem()
                    .and_then(|s| s.to_str())
                    .and_then(|s| s.parse::<i16>().ok())
                {
                    years.entry(year).or_default();
                }
            }
        }
    }
    let mut written = Vec::new();
    for (year, rows) in &years {
        if let Some(path) = write_state_log(vault, TOPIC, CATEGORY, *year, rows, &zone)? {
            written.push(path);
        }
    }
    sync_files(&written)?;
    summary.logs = written.len() as u64;
    for path in &written {
        change.touched(path);
    }
    summary.proposals = propose_aliases(vault, instance, &maybes)?;
    Ok(summary)
}

/// Proposes, for each person Immich may name by their first name only, to add that
/// name to their aliases. Returns how many proposals wait for the person.
fn propose_aliases(vault: &Vault, instance: &str, maybes: &[(&Known, String, u64)]) -> Result<u64> {
    if maybes.is_empty() {
        return Ok(0);
    }
    let proposals = Proposals::new(vault);
    let by = Author::Connector(instance.to_owned());
    let mut known: Vec<_> = proposals
        .list()?
        .into_iter()
        .filter(|p| p.by == by)
        .collect();
    let french = vault.settings().language() == Language::Fr;
    let mut waiting = 0;
    for (person, name, photos) in maybes {
        let full = person.name.clone().unwrap_or_else(|| person.stem.clone());
        let reason = if french {
            format!(
                "Immich nomme « {name} » une personne de {photos} photos. Est-ce {full} ? Accepter ajoute « {name} » à ses autres noms, et ses photos lui seront liées."
            )
        } else {
            format!(
                "Immich names someone \"{name}\" on {photos} photos. Is it {full}? Accepting adds \"{name}\" to their other names, and links their photos to them."
            )
        };
        let mut aliases = person.aliases.clone();
        let from = Value::Array(aliases.iter().cloned().map(Value::String).collect());
        aliases.push(name.clone());
        let to = Value::Array(aliases.into_iter().map(Value::String).collect());
        let changes = vec![Proposed::SetKey {
            file: relative(vault.root(), &person.path),
            key: "aliases".to_owned(),
            from,
            to,
        }];
        if proposals.propose_for_source(instance, &reason, changes, &mut known)? {
            waiting += 1;
        }
    }
    Ok(waiting)
}
