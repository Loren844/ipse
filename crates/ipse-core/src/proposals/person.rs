//! The person's own changes, made in the interface (decision 0016): checked and
//! written like a proposal's changes, by the same code, at once, as one change of the
//! history by the person. A proposal an AI made from the old text no longer applies
//! and becomes stale; a source that changes the file again proposes instead of writing
//! over the person.

use std::collections::BTreeMap;
use std::path::Path;

use serde::Deserialize;
use serde_json::Value;

use super::Proposals;
use super::apply::{self, Pending};
use super::change::{self, Change, NewRelation, existing, memory_path};
use super::key;
use crate::slug::to_file_stem;
use crate::vault::body::{Fact, Relation};
use crate::vault::{MemoryFile, Source};
use crate::{Error, Result};

/// What the person wants a file to say, from the interface's form.
#[derive(Debug, Clone, Default, PartialEq, Deserialize)]
pub struct Draft {
    /// The front-matter keys the form shows, with their new values (lists as arrays,
    /// `null` for none). Keys left out are not changed.
    #[serde(default)]
    pub keys: BTreeMap<String, Value>,
    /// Its facts, in order; `None` leaves them as they are.
    #[serde(default)]
    pub facts: Option<Vec<DraftFact>>,
    /// Its relations, in order; `None` leaves them as they are.
    #[serde(default)]
    pub relations: Option<Vec<NewRelation>>,
    /// Its notes, empty for none; `None` leaves them as they are.
    #[serde(default)]
    pub notes: Option<String>,
}

/// One fact of a [`Draft`].
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct DraftFact {
    /// Its position among the facts of the file as it was read, or `None` for a new
    /// fact.
    #[serde(default)]
    pub was: Option<usize>,
    /// What it says.
    pub text: String,
}

/// A file the person created.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Created {
    /// Its path, relative to the vault, with `/`.
    pub file: String,
    /// Its identifier.
    pub id: String,
    /// The change of the history it made.
    pub change: Option<String>,
}

/// The version of the file at `path`, as the interface read it: a short hash of its
/// bytes. Saving with an older version is refused: the file changed meanwhile.
///
/// # Errors
///
/// [`Error::Io`] when the file cannot be read.
pub fn version(path: &Path) -> Result<String> {
    let bytes = std::fs::read(path).map_err(|source| Error::Io {
        action: "read",
        path: path.to_owned(),
        source,
    })?;
    Ok(blake3::hash(&bytes).to_hex()[..16].to_owned())
}

fn changed_meanwhile() -> Error {
    Error::Refused(
        "This file changed since you opened it: open it again to see what it says now.".to_owned(),
    )
}

impl Proposals<'_> {
    /// Writes what the person changed in `file` (relative to the vault), read at
    /// `version`, as one change of the history by the person. Returns that change;
    /// `None` when the file already said so.
    ///
    /// # Errors
    ///
    /// [`Error::Refused`] with a plain-language message when the file changed since
    /// `version`, or a value is not valid; [`Error::Io`], [`Error::Invalid`] or
    /// [`Error::History`] when the file or the history cannot be written.
    pub fn edit(&self, file: &str, version_read: &str, draft: &Draft) -> Result<Option<String>> {
        let path = memory_path(self.vault.root(), file)?;
        let mut current = existing(self.vault, file)?;
        if version(&path)? != version_read {
            return Err(changed_meanwhile());
        }
        let history = self.vault.history()?;
        let mut record = history.change(Source::Person);
        // Facts written in an editor may have no identifier yet: they get one first,
        // in the same change, so that each can be edited.
        if draft.facts.is_some() && current.sections().facts.iter().any(|f| f.id.is_none()) {
            current.write()?;
            record.touched(&path);
            current = MemoryFile::read(&path)?;
        }
        let mut changes = changes_for(&current, file, draft);
        if changes.is_empty() {
            return Ok(record.commit("Edit a file")?.map(|e| e.id));
        }
        for change in &mut changes {
            change::check(self.vault, change, &[])?;
        }
        let plan = apply::plan_changes(self.vault, &changes, None, false)?;
        if let Some(reason) = plan.preview.stale {
            return Err(Error::Refused(reason));
        }
        apply::write(self.vault, plan.files, &mut record)?;
        Ok(record.commit("Edit a file")?.map(|e| e.id))
    }

    /// Creates the file `change` describes (a [`Change::Create`]) as the person's, in
    /// one change of the history. With `stem`, the file is named so when that name is
    /// free: to create the file a link already points to.
    ///
    /// # Errors
    ///
    /// [`Error::Refused`] when `change` is not a creation or is not valid; errors
    /// writing the file or the history.
    pub fn create(&self, mut change: Change, stem: Option<&str>) -> Result<Created> {
        if !matches!(change, Change::Create { .. }) {
            return Err(Error::Refused("Only a new file can be created.".to_owned()));
        }
        change::check(self.vault, &mut change, &[])?;
        if let (
            Some(stem),
            Change::Create {
                file: Some(file),
                file_type,
                ..
            },
        ) = (stem, &mut change)
            && file_type != "event"
            && to_file_stem(stem).as_deref() == Some(stem)
        {
            let folder = file.split('/').next().unwrap_or_default().to_owned();
            let wanted = format!("{folder}/{stem}.md");
            if !self.vault.root().join(&wanted).exists() {
                *file = wanted;
            }
        }
        let rel = change.file().unwrap_or_default().to_owned();
        let plan = apply::plan_changes(self.vault, std::slice::from_ref(&change), None, false)?;
        if let Some(reason) = plan.preview.stale {
            return Err(Error::Refused(reason));
        }
        let history = self.vault.history()?;
        let mut record = history.change(Source::Person);
        apply::write(self.vault, plan.files, &mut record)?;
        let entry = record.commit("Create a file")?;
        let created = MemoryFile::read(self.vault.root().join(&rel))?;
        Ok(Created {
            file: rel,
            id: created.common().id.to_string(),
            change: entry.map(|e| e.id),
        })
    }

    /// Deletes `file`, read at `version`, in one change of the history by the person,
    /// which can be undone. Links to it are left as they are: they show its name.
    ///
    /// # Errors
    ///
    /// [`Error::Refused`] for the owner's own file, or when the file changed since
    /// `version`; errors removing it or writing the history.
    pub fn delete(&self, file: &str, version_read: &str) -> Result<Option<String>> {
        let path = memory_path(self.vault.root(), file)?;
        let found = existing(self.vault, file)?;
        if found.entity().is_some_and(|e| e.owner) || path == self.vault.owner_file() {
            return Err(Error::Refused(
                "Your own file cannot be deleted.".to_owned(),
            ));
        }
        if version(&path)? != version_read {
            return Err(changed_meanwhile());
        }
        let history = self.vault.history()?;
        let mut record = history.change(Source::Person);
        apply::write(self.vault, vec![Pending::removal(found)], &mut record)?;
        Ok(record.commit("Delete a file")?.map(|e| e.id))
    }
}

/// The changes that make `current` (at `file`) say what `draft` says. Keys first,
/// the name last (setting it keeps the old one among the other names); then facts,
/// relations and notes.
fn changes_for(current: &MemoryFile, file: &str, draft: &Draft) -> Vec<Change> {
    let mut changes = Vec::new();
    let mut keys: Vec<(&String, &Value)> = draft.keys.iter().collect();
    keys.sort_by_key(|(k, _)| k.as_str() == "name");
    for (name, to) in keys {
        let from = key::get(current, name).unwrap_or(Value::Null);
        let to = trimmed(to);
        if key::normalized(name, &from) != key::normalized(name, &to) {
            changes.push(Change::SetKey {
                file: file.to_owned(),
                key: name.clone(),
                from,
                to,
            });
        }
    }
    let sections = current.sections();
    if let Some(facts) = &draft.facts {
        fact_changes(&sections.facts, file, facts, &mut changes);
    }
    if let Some(relations) = &draft.relations {
        relation_changes(&sections.relations, file, relations, &mut changes);
    }
    let Some(notes) = &draft.notes else {
        return changes;
    };
    let notes = Some(notes.trim_matches('\n').trim_end())
        .filter(|n| !n.trim().is_empty())
        .map(str::to_owned);
    if notes != sections.notes {
        changes.push(Change::SetNotes {
            file: file.to_owned(),
            from: sections.notes,
            to: notes,
        });
    }
    changes
}

/// The changes that turn the facts `facts_now` of `file` into `facts`.
fn fact_changes(facts_now: &[Fact], file: &str, facts: &[DraftFact], changes: &mut Vec<Change>) {
    let kept: Vec<usize> = facts.iter().filter_map(|f| f.was).collect();
    for (at, fact) in facts_now.iter().enumerate() {
        let Some(id) = &fact.id else { continue };
        if !kept.contains(&at) {
            changes.push(Change::RemoveFact {
                file: file.to_owned(),
                fact: id.as_str().to_owned(),
                text: fact.text.clone(),
            });
        }
    }
    for draft_fact in facts {
        let text = draft_fact
            .text
            .split_whitespace()
            .collect::<Vec<_>>()
            .join(" ");
        let old = draft_fact.was.and_then(|at| facts_now.get(at));
        match old {
            Some(old) if text.is_empty() => {
                if let Some(id) = &old.id {
                    changes.push(Change::RemoveFact {
                        file: file.to_owned(),
                        fact: id.as_str().to_owned(),
                        text: old.text.clone(),
                    });
                }
            }
            Some(old) if old.text != text => {
                if let Some(id) = &old.id {
                    changes.push(Change::EditFact {
                        file: file.to_owned(),
                        fact: id.as_str().to_owned(),
                        from: old.text.clone(),
                        to: text,
                    });
                }
            }
            Some(_) => {}
            None if text.is_empty() => {}
            None => changes.push(Change::AddFact {
                file: file.to_owned(),
                text,
            }),
        }
    }
}

/// The changes that turn the relations `relations_now` of `file` into `relations`.
fn relation_changes(
    relations_now: &[Relation],
    file: &str,
    relations: &[NewRelation],
    changes: &mut Vec<Change>,
) {
    let same =
        |word: &str, target: &str, r: &NewRelation| r.word.trim() == word && r.target == target;
    for old in relations_now {
        if !relations
            .iter()
            .any(|r| same(&old.word, &old.link.target, r))
        {
            changes.push(Change::RemoveRelation {
                file: file.to_owned(),
                word: old.word.clone(),
                target: old.link.target.clone(),
            });
        }
    }
    for new in relations {
        let word = new.word.split_whitespace().collect::<Vec<_>>().join(" ");
        if word.is_empty() && new.target.is_empty() {
            continue;
        }
        if !relations_now
            .iter()
            .any(|r| r.word == word && r.link.target == new.target)
        {
            changes.push(Change::AddRelation {
                file: file.to_owned(),
                word,
                target: new.target.clone(),
                display: new.display.clone(),
            });
        }
    }
}

/// `value` with its text trimmed; empty text and empty items are none.
fn trimmed(value: &Value) -> Value {
    match value {
        Value::String(s) if s.trim().is_empty() => Value::Null,
        Value::String(s) => Value::String(s.trim().to_owned()),
        Value::Array(items) => {
            Value::Array(items.iter().map(trimmed).filter(|v| !v.is_null()).collect())
        }
        other => other.clone(),
    }
}
