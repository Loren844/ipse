//! The query layer: the one place data leaves the vault for an AI (`docs/spec/rights.md`,
//! sections 4 and 6).
//!
//! Rules every read follows:
//! - the access code is checked before anything is read for the request;
//! - a file outside the client's categories is never returned, counted or hinted at;
//! - a fact, relation or line of notes that links to such a file is left out entirely,
//!   so that nothing shows that something is hidden;
//! - the category is checked again on the file as it is when read, not only in the index.

use std::path::{Path, PathBuf};

use serde::Serialize;

use super::clients::{Client, Clients};
use crate::search::{Results, SearchIndex};
use crate::vault::{FileType, Id, MemoryFile, Vault};
use crate::{Error, Result};

/// What an AI can ask for, opened on one vault.
#[derive(Debug)]
pub struct Gateway {
    vault: Vault,
    index: SearchIndex,
    clients: Clients,
}

impl Gateway {
    /// Opens the vault in `root`, reads its entities and its connected AIs.
    ///
    /// # Errors
    ///
    /// Errors of [`Vault::open`], [`SearchIndex::load`] and [`Clients::load`].
    pub fn open(root: &Path) -> Result<Self> {
        let vault = Vault::open(root)?;
        let index = SearchIndex::load(vault.root())?;
        let clients = Clients::load(vault.root())?;
        Ok(Self {
            vault,
            index,
            clients,
        })
    }

    /// Reads the entities and the connected AIs again, after the vault changed.
    ///
    /// # Errors
    ///
    /// As [`Gateway::open`].
    pub fn refresh(&mut self) -> Result<()> {
        self.index = SearchIndex::load(self.vault.root())?;
        self.clients = Clients::load(self.vault.root())?;
        Ok(())
    }

    /// Reads the connected AIs again, checks `token`, then reads the entities again if
    /// the vault changed since they were read. For a server that runs as long as the AI
    /// app: rights apply from the next request, and nothing is read for a refused code.
    ///
    /// # Errors
    ///
    /// [`Error::Refused`] when the code is missing or not valid; errors of
    /// [`Gateway::open`] when the vault cannot be read.
    pub fn fresh_access(&mut self, token: Option<&str>) -> Result<Access<'_>> {
        self.clients = Clients::load(self.vault.root())?;
        self.access(token)?;
        if self.index.changed(self.vault.root())? {
            self.index = SearchIndex::load(self.vault.root())?;
        }
        self.access(token)
    }

    /// The reads allowed by `token`. A missing or unknown code is refused before
    /// anything is read for the request.
    ///
    /// # Errors
    ///
    /// [`Error::Refused`] when the code is missing or not valid.
    pub fn access(&self, token: Option<&str>) -> Result<Access<'_>> {
        let client = token
            .filter(|t| !t.is_empty())
            .and_then(|t| self.clients.authenticate(t))
            .ok_or_else(|| {
                Error::Refused(
                    "This access code is not valid. Connect the AI again with `ipse client add`."
                        .to_owned(),
                )
            })?;
        Ok(Access {
            gateway: self,
            client,
        })
    }
}

/// Reads on behalf of one connected AI.
#[derive(Debug, Clone, Copy)]
pub struct Access<'g> {
    gateway: &'g Gateway,
    client: &'g Client,
}

/// One entity, as an AI receives it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Entry {
    /// Stable identifier.
    pub id: String,
    /// What the entity is: `person`, `place`…
    #[serde(rename = "type")]
    pub file_type: String,
    /// Display name.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    /// Other names.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub aliases: Vec<String>,
    /// `YYYY-MM-DD`, or `--MM-DD` without the year.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub birthday: Option<String>,
    /// Email addresses.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub emails: Vec<String>,
    /// Phone numbers.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub phones: Vec<String>,
    /// Postal address.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub address: Option<String>,
    /// The place it is in, as a link; left out when that place is hidden.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub located_in: Option<String>,
    /// Facts; those linking to a hidden file are left out.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub facts: Vec<String>,
    /// Relations; those pointing to a hidden file are left out.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub relations: Vec<RelationOut>,
    /// Notes; lines linking to a hidden file are left out.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub notes: Option<String>,
}

/// A relation as an AI receives it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct RelationOut {
    /// The relation word: `mère`, `works at`.
    pub word: String,
    /// `[[target]]`.
    pub target: String,
}

impl<'g> Access<'g> {
    /// The connected AI these reads are for.
    #[must_use]
    pub fn client(&self) -> &'g Client {
        self.client
    }

    /// Entities matching `query`, best first, among the categories this AI may read.
    /// `limit` is clamped to 1–50.
    #[must_use]
    pub fn search(&self, query: &str, limit: usize) -> Results {
        self.gateway.index.search_with(
            query,
            limit,
            |c| self.client.reads(c.as_str()),
            |text| self.strip(text),
        )
    }

    /// The entity with this id, if this AI may read it.
    #[must_use]
    pub fn get(&self, id: &str) -> Option<Entry> {
        let id = Id::parse(id)?;
        let path = self
            .gateway
            .index
            .path_of(&id, |c| self.client.reads(c.as_str()))?;
        self.entry(path)
    }

    /// The entities named `name` (name or alias, ignoring case and accents) this AI may
    /// read.
    #[must_use]
    pub fn get_by_name(&self, name: &str) -> Vec<Entry> {
        self.gateway
            .index
            .named(name, |c| self.client.reads(c.as_str()))
            .into_iter()
            .filter_map(|path| self.entry(path))
            .collect()
    }

    /// The owner's own file, if this AI may read the `identity` category.
    #[must_use]
    pub fn about_me(&self) -> Option<Entry> {
        let owner = self.gateway.vault.owner_file();
        let relative = owner
            .strip_prefix(self.gateway.vault.root())
            .unwrap_or(owner);
        self.entry(relative)
    }

    /// Reads one entity file and checks its category on the file itself.
    fn entry(&self, relative: &Path) -> Option<Entry> {
        let path: PathBuf = self.gateway.vault.root().join(relative);
        let file = MemoryFile::read(&path).ok()?;
        let common = file.common();
        if !self.client.reads(common.category.as_str()) {
            return None;
        }
        let entity = file.entity()?;
        let sections = file.sections();
        Some(Entry {
            id: common.id.to_string(),
            file_type: type_name(common.file_type),
            name: entity.name.clone(),
            aliases: entity.aliases.clone(),
            birthday: entity.birthday.map(|b| b.to_string()),
            emails: entity.emails.clone(),
            phones: entity.phones.clone(),
            address: entity.address.clone(),
            located_in: entity
                .located_in
                .as_ref()
                .filter(|l| self.follows(&l.target))
                .map(ToString::to_string),
            facts: sections
                .facts
                .iter()
                .filter(|f| !self.hides(&f.text))
                .map(|f| f.text.clone())
                .collect(),
            relations: sections
                .relations
                .iter()
                .filter(|r| self.follows(&r.link.target))
                .map(|r| RelationOut {
                    word: r.word.clone(),
                    target: r.link.to_string(),
                })
                .collect(),
            notes: sections
                .notes
                .as_deref()
                .map(|n| self.strip(n))
                .filter(|n| !n.trim().is_empty()),
        })
    }

    /// `text` without the lines that link to anything this AI may not read. A fact is
    /// one line: it is left out entirely.
    fn strip(&self, text: &str) -> String {
        let mut out = String::with_capacity(text.len());
        for line in text.split_inclusive('\n') {
            if !self.hides(line) {
                out.push_str(line);
            }
        }
        out.trim_end_matches('\n').to_owned()
    }

    /// True when `text` holds a `[[link]]` this AI may not follow.
    fn hides(&self, text: &str) -> bool {
        let mut rest = text;
        while let Some(open) = rest.find("[[") {
            let Some(close) = rest[open..].find("]]").map(|c| open + c) else {
                return false;
            };
            let target = rest[open + 2..close].split('|').next().unwrap_or_default();
            if !self.follows(target.trim()) {
                return true;
            }
            rest = &rest[close + 2..];
        }
        false
    }

    /// True when a link to `target` points to known entities this AI may all read. A
    /// link to nothing known counts as hidden.
    fn follows(&self, target: &str) -> bool {
        let categories = self.gateway.index.categories_of(target);
        !categories.is_empty() && categories.iter().all(|c| self.client.reads(c.as_str()))
    }
}

fn type_name(file_type: FileType) -> String {
    file_type.as_str().to_owned()
}
