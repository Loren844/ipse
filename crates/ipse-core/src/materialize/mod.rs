//! Turning journal records into vault files (decision 0005: data a connector produces
//! deterministically is written directly).
//!
//! The person always wins (`docs/spec/vault.md`, section 9). Ipse writes only the keys
//! it takes from a source, and only while they are as it last wrote them: the journal
//! keeps every version, and a small ledger remembers which one is in the vault. If the
//! person changed one of those keys, the update is skipped and counted, to become a
//! proposal in milestone 2; what they added (facts, notes, other keys) never stops an
//! update. A file the person deleted is not created again.

mod contacts;
mod events;

use std::collections::{BTreeMap, HashMap};
use std::fmt::Write as _;
use std::path::{Path, PathBuf};

pub use contacts::{PeopleSummary, contacts_to_people};
pub use events::{EventsSummary, calendar_to_events};
use serde_json::Value;

use crate::journal::Journal;
use crate::vault::write_atomic;
use crate::{Error, Result};

/// What happened to one existing file.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Outcome {
    Written,
    Unchanged,
    /// The person changed a key Ipse would write: the file is left as it is.
    Conflict,
}

/// Where Ipse remembers, per journal source, the last record it turned into files.
pub const LEDGER_FILE: &str = ".ipse/materialized.toml";

/// The versions of one item of the journal that matter to an import.
#[derive(Debug, Clone, Default)]
pub(crate) struct Versions {
    /// The latest data; `None` once the item was removed at its source.
    pub(crate) latest: Option<Value>,
    /// The data Ipse last wrote to the vault, if it ever did.
    pub(crate) applied: Option<Value>,
    /// True when the last record Ipse applied was a removal.
    pub(crate) applied_removal: bool,
    /// True when Ipse already turned a record of this item into a file.
    pub(crate) materialized: bool,
}

/// Items with their versions, in the order they first appeared, and the id of the
/// journal's last record.
pub(crate) type Items = (Vec<(String, Versions)>, Option<String>);

/// The items of `instance` of `kind`, in the order they first appeared, with their
/// versions, and the id of the journal's last record.
pub(crate) fn versions(
    journal: &Journal,
    instance: &str,
    kind: &str,
    applied_up_to: Option<&str>,
) -> Result<Items> {
    let mut order: Vec<String> = Vec::new();
    let mut items: HashMap<String, Versions> = HashMap::new();
    let mut last = None;
    for record in journal.read(instance, Some(&[kind]))? {
        let record = record?;
        let applied = applied_up_to.is_some_and(|up_to| record.id.as_str() <= up_to);
        last = Some(record.id);
        let Some(id) = record.external_id else {
            continue;
        };
        let item = items.entry(id.clone()).or_insert_with(|| {
            order.push(id);
            Versions::default()
        });
        if applied {
            item.materialized = true;
            item.applied_removal = record.data.is_none();
            if record.data.is_some() {
                item.applied.clone_from(&record.data);
            }
        }
        item.latest = record.data;
    }
    let items = order
        .into_iter()
        .filter_map(|id| items.remove(&id).map(|v| (id, v)))
        .collect();
    Ok((items, last))
}

/// The last record turned into files, per journal source.
pub(crate) struct Ledger {
    path: PathBuf,
    applied: BTreeMap<String, String>,
}

impl Ledger {
    pub(crate) fn read(root: &Path) -> Result<Self> {
        let path = root.join(LEDGER_FILE);
        let applied = match std::fs::read_to_string(&path) {
            Ok(text) => toml::from_str(&text).map_err(|_| Error::Invalid {
                path: path.clone(),
                line: None,
                problem: "this file is Ipse's own record of imports; delete it and import again"
                    .to_owned(),
            })?,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => BTreeMap::new(),
            Err(source) => {
                return Err(Error::Io {
                    action: "read",
                    path,
                    source,
                });
            }
        };
        Ok(Self { path, applied })
    }

    pub(crate) fn applied(&self, instance: &str) -> Option<&str> {
        self.applied.get(instance).map(String::as_str)
    }

    /// Remembers that every record of `instance` up to `last` is in the vault.
    pub(crate) fn save(mut self, instance: &str, last: Option<String>) -> Result<()> {
        let Some(last) = last else {
            return Ok(());
        };
        if self.applied.get(instance) == Some(&last) {
            return Ok(());
        }
        self.applied.insert(instance.to_owned(), last);
        let mut text =
            String::from("# The last journal record of each source that Ipse turned into files.\n");
        for (instance, id) in &self.applied {
            let _ = writeln!(
                text,
                "{} = {}",
                serde_json::to_string(instance).unwrap_or_default(),
                serde_json::to_string(id).unwrap_or_default()
            );
        }
        write_atomic(&self.path, text.as_bytes())
    }
}

/// The `.md` files of `dir` (and of its subfolders when `recursive`), hidden files
/// aside. A missing folder has none.
pub(crate) fn entity_files(dir: &Path, recursive: bool) -> Result<Vec<PathBuf>> {
    let mut files = Vec::new();
    let mut stack = vec![dir.to_owned()];
    while let Some(dir) = stack.pop() {
        let entries = match std::fs::read_dir(&dir) {
            Ok(entries) => entries,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => continue,
            Err(source) => {
                return Err(Error::Io {
                    action: "read",
                    path: dir,
                    source,
                });
            }
        };
        for entry in entries.flatten() {
            if entry.file_name().to_string_lossy().starts_with('.') {
                continue;
            }
            let path = entry.path();
            match entry.file_type() {
                Ok(t) if t.is_dir() && recursive => stack.push(path),
                Ok(t) if t.is_file() && path.extension().is_some_and(|e| e == "md") => {
                    files.push(path);
                }
                _ => {}
            }
        }
    }
    files.sort();
    Ok(files)
}
