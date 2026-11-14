//! The latest record of each item of a source (`docs/spec/journal.md`, section 7):
//! item → latest record id and hash, to skip unchanged upserts without reading the
//! journal. Derived and disposable: saved in `.ipse/index/journal/<source>.bin`, read
//! again from the journal when it is missing, damaged or behind it.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use ipse_protocol::Op;
use serde::{Deserialize, Serialize};

use super::read::Reader;
use super::segment;
use crate::Result;
use crate::vault::write_atomic_unsynced;

/// Version of the saved table.
const FORMAT: u32 = 1;

/// The latest record of one item.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct Latest {
    pub(crate) id: String,
    /// `None` when the latest record is a removal.
    pub(crate) hash: Option<String>,
}

/// The latest records of one source, by `(kind, external_id)`.
#[derive(Debug, Default)]
pub(crate) struct Table {
    items: HashMap<(String, String), Latest>,
    /// The id of the last record the table has seen.
    last_id: Option<String>,
    changed: bool,
}

#[derive(Serialize, Deserialize)]
struct Saved {
    format: u32,
    last_id: Option<String>,
    items: Vec<(String, String, Latest)>,
}

/// Where the table of `source` is saved.
fn path(vault_root: &Path, source: &str) -> PathBuf {
    vault_root
        .join(".ipse/index/journal")
        .join(format!("{source}.bin"))
}

impl Table {
    /// The table of the source whose segments are in `dir`: the saved one when it is
    /// as far as the journal, otherwise one read from the journal.
    pub(crate) fn load(vault_root: &Path, source: &str, dir: &Path) -> Result<Self> {
        let journal_last = segment::last_id(dir);
        if let Some(saved) = std::fs::read(path(vault_root, source))
            .ok()
            .and_then(|bytes| postcard::from_bytes::<Saved>(&bytes).ok())
            .filter(|s| s.format == FORMAT && s.last_id == journal_last)
        {
            return Ok(Self {
                items: saved
                    .items
                    .into_iter()
                    .map(|(kind, id, latest)| ((kind, id), latest))
                    .collect(),
                last_id: saved.last_id,
                changed: false,
            });
        }
        let mut table = Self {
            changed: true,
            ..Self::default()
        };
        for record in Reader::open(dir, None, None)? {
            let record = record?;
            table.note(
                &record.kind,
                record.external_id.as_deref(),
                record.op,
                &record.id,
                record.hash.as_deref(),
            );
        }
        Ok(table)
    }

    /// True when an upsert of this item with this hash would change nothing.
    pub(crate) fn unchanged(&self, kind: &str, external_id: &str, hash: &str) -> bool {
        self.items
            .get(&(kind.to_owned(), external_id.to_owned()))
            .and_then(|l| l.hash.as_deref())
            == Some(hash)
    }

    /// Remembers a record written.
    pub(crate) fn note(
        &mut self,
        kind: &str,
        external_id: Option<&str>,
        op: Op,
        id: &str,
        hash: Option<&str>,
    ) {
        self.last_id = Some(id.to_owned());
        self.changed = true;
        let Some(external_id) = external_id else {
            return;
        };
        let hash = match op {
            Op::Upsert => hash.map(str::to_owned),
            _ => None,
        };
        if op == Op::Observe {
            return;
        }
        self.items.insert(
            (kind.to_owned(), external_id.to_owned()),
            Latest {
                id: id.to_owned(),
                hash,
            },
        );
    }

    /// Saves the table if it changed. A failure only means reading the journal again
    /// next time.
    pub(crate) fn save(&mut self, vault_root: &Path, source: &str) {
        if !self.changed {
            return;
        }
        let saved = Saved {
            format: FORMAT,
            last_id: self.last_id.clone(),
            items: self
                .items
                .iter()
                .map(|((kind, id), latest)| (kind.clone(), id.clone(), latest.clone()))
                .collect(),
        };
        if let Ok(bytes) = postcard::to_stdvec(&saved)
            && write_atomic_unsynced(&path(vault_root, source), &bytes).is_ok()
        {
            self.changed = false;
        }
    }
}
