//! `contact.card` records → `people/` files.
//!
//! A person is found again by its source (`contacts:<external_id>`), never by name,
//! email or phone (decision 0011). On re-import Ipse refreshes only the keys it took from
//! the card; everything the person wrote (other keys, facts, notes) stays as it is.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use serde_json::Value;

use crate::Result;
use crate::journal::Journal;
use crate::vault::{
    Birthday, Body, Change, Common, DateTime, EntityFields, Fact, FileType, Language, MemoryFile,
    Vault, body, naming,
};

/// The record kind this module reads.
pub const KIND: &str = "contact.card";

/// What an import changed in the vault.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct PeopleSummary {
    /// People files created.
    pub added: u64,
    /// People files updated.
    pub updated: u64,
    /// People already up to date.
    pub unchanged: u64,
    /// Records without a name, ignored.
    pub skipped: u64,
}

/// Creates or updates one `people/` file per contact from `instance`'s records.
///
/// # Errors
///
/// Journal and vault errors: a file that cannot be read, parsed or written. A person
/// file that cannot be parsed is never overwritten: the error says which one.
pub fn contacts_to_people(
    vault: &Vault,
    journal: &Journal,
    instance: &str,
    change: &mut Change<'_>,
) -> Result<PeopleSummary> {
    let latest = latest_cards(journal, instance)?;
    let existing = people_by_source(vault.root(), instance)?;
    let mut summary = PeopleSummary::default();

    for (external_id, data) in latest {
        let Some(card) = CardFields::from_data(&data) else {
            summary.skipped += 1;
            continue;
        };
        let source = format!("{instance}:{external_id}");
        if let Some(path) = existing.get(&source) {
            if update_person(path, &card, vault)? {
                change.touched(path);
                summary.updated += 1;
            } else {
                summary.unchanged += 1;
            }
        } else if let Some(path) = create_person(source, &card, vault)? {
            change.touched(&path);
            summary.added += 1;
        }
    }
    Ok(summary)
}

/// Refreshes the keys taken from the card. Returns whether the file changed.
fn update_person(path: &Path, card: &CardFields, vault: &Vault) -> Result<bool> {
    let mut file = MemoryFile::read(path)?;
    let Some(current) = file.entity().cloned() else {
        return Ok(false);
    };
    let wanted = card.apply_to(current.clone());
    if wanted == current {
        return Ok(false);
    }
    if let Some(entity) = file.entity_mut() {
        *entity = wanted;
    }
    file.common_mut().updated = DateTime::now(vault.settings().time_zone());
    file.write()
}

/// Writes a new person file for a card seen for the first time. Returns its path.
fn create_person(source: String, card: &CardFields, vault: &Vault) -> Result<Option<PathBuf>> {
    let settings = vault.settings();
    let Some(mut common) = Common::new(FileType::Person, settings.time_zone()) else {
        return Ok(None);
    };
    common.sources = vec![source];
    let entity = card.apply_to(EntityFields::default());
    let path = naming::entity_path(
        vault.root(),
        FileType::Person,
        entity.name.as_deref(),
        &common.id,
    )?;
    let body = body::render(&card.body(settings.language()), settings.language());
    MemoryFile::new(&path, common, Some(entity), body).write()?;
    Ok(Some(path))
}

/// The latest `data` of every card of `instance`, in the order the cards first appeared.
fn latest_cards(journal: &Journal, instance: &str) -> Result<Vec<(String, Value)>> {
    let mut order: Vec<String> = Vec::new();
    let mut latest: HashMap<String, Value> = HashMap::new();
    for record in journal.read(instance, Some(&[KIND]))? {
        let record = record?;
        let (Some(id), Some(data)) = (record.external_id, record.data) else {
            continue;
        };
        if latest.insert(id.clone(), data).is_none() {
            order.push(id);
        }
    }
    Ok(order
        .into_iter()
        .filter_map(|id| latest.remove(&id).map(|data| (id, data)))
        .collect())
}

/// People files whose `sources` name a card of `instance`, by source.
fn people_by_source(root: &Path, instance: &str) -> Result<HashMap<String, PathBuf>> {
    let prefix = format!("{instance}:");
    let mut by_source = HashMap::new();
    let entries = match std::fs::read_dir(root.join(FileType::Person.folder())) {
        Ok(entries) => entries,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(by_source),
        Err(source) => {
            return Err(crate::Error::Io {
                action: "read",
                path: root.join(FileType::Person.folder()),
                source,
            });
        }
    };
    for entry in entries.flatten() {
        let path = entry.path();
        let is_note = path.extension().is_some_and(|e| e == "md")
            && !entry.file_name().to_string_lossy().starts_with('.');
        if !is_note {
            continue;
        }
        // A file that cannot be read is not ours to fix here: it is skipped, and never
        // overwritten, since its source cannot be known.
        let Ok(file) = MemoryFile::read(&path) else {
            continue;
        };
        for source in &file.common().sources {
            if source.starts_with(&prefix) {
                by_source.insert(source.clone(), path.clone());
            }
        }
    }
    Ok(by_source)
}

/// What Ipse takes from a card.
struct CardFields {
    name: String,
    nicknames: Vec<String>,
    emails: Vec<String>,
    phones: Vec<String>,
    birthday: Option<Birthday>,
    organization: Option<String>,
    note: Option<String>,
}

impl CardFields {
    fn from_data(data: &Value) -> Option<Self> {
        let text = |key: &str| {
            data.get(key)
                .and_then(Value::as_str)
                .map(str::trim)
                .filter(|s| !s.is_empty())
                .map(str::to_owned)
        };
        let list = |key: &str| -> Vec<String> {
            data.get(key)
                .and_then(Value::as_array)
                .map(|items| {
                    items
                        .iter()
                        .filter_map(Value::as_str)
                        .map(str::trim)
                        .filter(|s| !s.is_empty())
                        .map(str::to_owned)
                        .collect()
                })
                .unwrap_or_default()
        };
        Some(Self {
            name: text("full_name")?,
            nicknames: list("nicknames"),
            emails: list("emails"),
            phones: list("phones"),
            birthday: text("birthday").as_deref().and_then(Birthday::parse),
            organization: text("organization"),
            note: text("note"),
        })
    }

    /// `entity` with the keys taken from the card replaced; other keys kept.
    fn apply_to(&self, mut entity: EntityFields) -> EntityFields {
        entity.name = Some(self.name.clone());
        entity.aliases.clone_from(&self.nicknames);
        entity.emails.clone_from(&self.emails);
        entity.phones.clone_from(&self.phones);
        entity.birthday = self.birthday;
        entity
    }

    /// The body of a new person file. Written once: later imports leave the body alone.
    fn body(&self, language: Language) -> Body {
        let facts = self
            .organization
            .iter()
            .map(|org| Fact {
                text: match language {
                    Language::Fr => format!("Organisation : {org}."),
                    _ => format!("Organization: {org}."),
                },
                id: None,
            })
            .collect();
        Body {
            facts,
            relations: Vec::new(),
            notes: self.note.clone(),
        }
    }
}
