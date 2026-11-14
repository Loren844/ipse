//! `contact.card` records → `people/` files.
//!
//! A person is found again by its source (`contacts:<external_id>`), never by name,
//! email or phone (decision 0011). On re-import Ipse refreshes only the keys it took from
//! the card; everything the person wrote (other keys, facts, notes) stays as it is.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use serde_json::Value;

use super::relations::{OwnerCandidate, Related, add_owner_relations};
use super::{Ledger, Merge, Outcome, versions};
use crate::Result;
use crate::journal::Journal;
use crate::vault::{
    Birthday, Body, Change, Common, DateTime, EntityFields, Fact, FileType, Language, MemoryFile,
    Vault, body, naming, sync_files,
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
    /// People whose name, emails, phones or birthday the person changed, left as they
    /// are although the card changed.
    pub conflicts: u64,
    /// People files the person deleted, not created again.
    pub deleted: u64,
    /// Relations added to the owner's file from their own card.
    pub relations: u64,
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
    let ledger = Ledger::read(vault.root())?;
    let (items, last) = versions(journal, instance, KIND, ledger.applied(instance))?;
    let existing = people_by_source(vault.root(), instance)?;
    let mut summary = PeopleSummary::default();
    let mut written = Vec::new();
    let mut cards = Vec::with_capacity(items.len());

    for (external_id, versions) in items {
        // A contact removed from the address book leaves its person file alone.
        let Some(data) = &versions.latest else {
            continue;
        };
        let Some(card) = CardFields::from_data(data) else {
            summary.skipped += 1;
            continue;
        };
        cards.push(OwnerCandidate {
            emails: card.emails.clone(),
            phones: card.phones.clone(),
            related: Related::list(data),
        });
        let source = format!("{instance}:{external_id}");
        if let Some(path) = existing.get(&source) {
            let previous = versions.applied.as_ref().and_then(CardFields::from_data);
            let outcome = update_person(path, &card, previous.as_ref(), vault)?;
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
        } else if let Some(path) = create_person(source, &card, vault)? {
            change.touched(&path);
            written.push(path.clone());
            summary.added += 1;
        }
    }
    // Made durable before the ledger says they are in the vault.
    sync_files(&written)?;
    summary.relations = add_owner_relations(vault, instance, &cards, change)?;
    ledger.save(instance, last)?;
    Ok(summary)
}

/// Refreshes the keys taken from the card, one by one: a key the person changed since
/// Ipse last wrote it from `previous` keeps the person's value.
fn update_person(
    path: &Path,
    card: &CardFields,
    previous: Option<&CardFields>,
    vault: &Vault,
) -> Result<Outcome> {
    let mut file = MemoryFile::read(path)?;
    let Some(current) = file.entity().cloned() else {
        return Ok(Outcome::default());
    };
    let (wanted, conflict) = card.merge(&current, previous);
    if wanted == current {
        return Ok(Outcome {
            written: false,
            conflict,
        });
    }
    if let Some(entity) = file.entity_mut() {
        *entity = wanted;
    }
    file.common_mut().updated = DateTime::now(vault.settings().time_zone());
    file.write_unsynced()?;
    Ok(Outcome {
        written: true,
        conflict,
    })
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
    MemoryFile::new(&path, common, Some(entity), body).write_unsynced()?;
    Ok(Some(path))
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

    /// `current` with the keys taken from the card merged in, one by one (see
    /// [`Merge`]), and whether the card changed a key the person had changed.
    ///
    /// A name the person gave is theirs: the card's name, often a nickname such as
    /// "Maman", then becomes one of the person's other names, so that it is still
    /// understood.
    fn merge(&self, current: &EntityFields, previous: Option<&CardFields>) -> (EntityFields, bool) {
        let mut merge = Merge::default();
        let mut out = current.clone();
        let card_name = Some(self.name.clone());
        let previous_name = previous.map(|p| Some(p.name.clone()));
        let renamed = previous_name.as_ref().is_some_and(|p| *p != current.name);
        out.name = if renamed {
            current.name.clone()
        } else {
            card_name.clone()
        };

        // The card's names Ipse adds as other names are left out of the comparison.
        let names: Vec<&str> = [Some(self.name.as_str()), previous.map(|p| p.name.as_str())]
            .into_iter()
            .flatten()
            .collect();
        let without_names = |list: &[String]| -> Vec<String> {
            list.iter()
                .filter(|a| !names.contains(&a.as_str()))
                .cloned()
                .collect()
        };
        let mut aliases = merge.key(
            &without_names(&current.aliases),
            previous.map(|p| without_names(&p.nicknames)).as_ref(),
            &without_names(&self.nicknames),
        );
        if renamed && !aliases.contains(&self.name) {
            aliases.push(self.name.clone());
        }
        out.aliases = aliases;
        out.emails = merge.key(&current.emails, previous.map(|p| &p.emails), &self.emails);
        out.phones = merge.key(&current.phones, previous.map(|p| &p.phones), &self.phones);
        out.birthday = merge.key(
            &current.birthday,
            previous.map(|p| &p.birthday),
            &self.birthday,
        );
        (out, merge.conflict)
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
