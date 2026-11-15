//! Browsing the memory, for the interface: lists of entities, one entity in full with
//! its links resolved. Through the same query layer as the tools: a client sees only
//! what its categories allow (the owner, every category).

use serde::Serialize;

use super::{Access, Entry};
use crate::vault::{FileType, Id, LinkOrText, MemoryFile, WikiLink};

/// Most entities one page of a list holds.
pub const MAX_LISTED: usize = 200;

/// One entity in a list.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Line {
    /// Its id.
    pub id: String,
    /// Its name.
    pub name: String,
    /// Its other names.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub aliases: Vec<String>,
    /// True when its category is closed to AIs unless the person opens it.
    pub sensitive: bool,
}

/// One page of a list.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
pub struct Lines {
    /// The entities, by name, or best first for a search.
    pub items: Vec<Line>,
    /// How many there are in all (for a search, those found, at most 50).
    pub total: usize,
    /// True when there are more after this page.
    pub more: bool,
}

/// A link of an entity to another, resolved for display.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct LinkOut {
    /// The text to show: the target's name, or the link's own text.
    pub name: String,
    /// The target's id, when it is a file this client may read.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub id: Option<String>,
    /// The target's type: `person`, `place`…
    #[serde(rename = "type", skip_serializing_if = "Option::is_none")]
    pub file_type: Option<String>,
}

/// A relation, resolved.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct RelationLink {
    /// The relation word, as written: `sœur`.
    pub word: String,
    /// What it points to.
    pub to: LinkOut,
}

/// One entity or event in full, for the interface.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Details {
    /// What an AI with the same rights would get.
    pub entry: Entry,
    /// Its category.
    pub category: String,
    /// True when its category is closed to AIs unless the person opens it.
    pub sensitive: bool,
    /// Where it comes from, by source: `contacts`, `calendar-2`, `proposal`, `person`.
    pub sources: Vec<String>,
    /// Its relations, resolved; those to hidden files are left out.
    pub relations: Vec<RelationLink>,
    /// For an event: its people, resolved.
    pub participants: Vec<LinkOut>,
    /// For an event: its place, resolved.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub place: Option<LinkOut>,
}

impl Access<'_> {
    /// True when `category` is closed to AIs unless the person opens it to one.
    #[must_use]
    pub fn sensitive(&self, category: &str) -> bool {
        let effective = self.gateway.categories.effective(category);
        self.gateway.categories.sensitive().contains(&effective)
    }

    /// The people this client may read: by name, `limit` (clamped to 1–200) from
    /// `offset`; or, with a `query`, those matching it, best first.
    #[must_use]
    pub fn people(&self, query: Option<&str>, limit: usize, offset: usize) -> Lines {
        let limit = limit.clamp(1, MAX_LISTED);
        let query = query.map(str::trim).filter(|q| !q.is_empty());
        if let Some(query) = query {
            let found = self.search(query, crate::search::MAX_LIMIT);
            let items: Vec<Line> = found
                .hits
                .into_iter()
                .filter(|h| h.file_type == FileType::Person)
                .map(|h| Line {
                    sensitive: self.sensitive(h.category.as_str()),
                    id: h.id.to_string(),
                    name: h.name,
                    aliases: Vec::new(),
                })
                .collect();
            let total = items.len();
            let more = total > offset + limit;
            let items = items.into_iter().skip(offset).take(limit).collect();
            return Lines { items, total, more };
        }
        let all = self
            .gateway
            .index
            .entities(FileType::Person, |c| self.reads(c.as_str()));
        let total = all.len();
        let items: Vec<Line> = all
            .into_iter()
            .skip(offset)
            .take(limit)
            .map(|l| {
                self.returned(l.category.as_str());
                Line {
                    id: l.id.to_string(),
                    name: l.name.to_owned(),
                    aliases: l.aliases.iter().map(|a| (*a).to_owned()).collect(),
                    sensitive: self.sensitive(l.category.as_str()),
                }
            })
            .collect();
        Lines {
            more: total > offset + items.len(),
            items,
            total,
        }
    }

    /// The entity or event with this id in full, its links resolved, if this client
    /// may read it.
    #[must_use]
    pub fn details(&self, id: &str) -> Option<Details> {
        let parsed = Id::parse(id)?;
        let path = self
            .gateway
            .index
            .path_of(&parsed, |c| self.reads(c.as_str()))?
            .to_owned();
        let entry = self.get(id)?;
        let file = MemoryFile::read(self.gateway.vault.root().join(&path)).ok()?;
        let common = file.common();
        let mut sources: Vec<String> = common
            .sources
            .iter()
            .map(|s| s.split(':').next().unwrap_or(s).to_owned())
            .collect();
        sources.dedup();
        let relations = entry
            .relations
            .iter()
            .filter_map(|r| {
                let link = WikiLink::parse(&r.target)?;
                Some(RelationLink {
                    word: r.word.clone(),
                    to: self.link(&LinkOrText::Link(link)),
                })
            })
            .collect();
        let participants = entry
            .participants
            .iter()
            .map(|p| self.link(&LinkOrText::parse(p)))
            .collect();
        let place = entry
            .place
            .as_deref()
            .map(|p| self.link(&LinkOrText::parse(p)));
        Some(Details {
            category: common.category.as_str().to_owned(),
            sensitive: self.sensitive(common.category.as_str()),
            sources,
            relations,
            participants,
            place,
            entry,
        })
    }

    /// A link or a plain text, resolved: the linked file's name and id when this client
    /// may read it, otherwise the text as written.
    #[must_use]
    pub fn link(&self, value: &LinkOrText) -> LinkOut {
        match value {
            LinkOrText::Text(text) => LinkOut {
                name: text.clone(),
                id: None,
                file_type: None,
            },
            LinkOrText::Link(link) => {
                let found = self
                    .gateway
                    .index
                    .linked(&link.target, |c| self.reads(c.as_str()));
                match found {
                    Some(listed) => LinkOut {
                        name: link
                            .display
                            .clone()
                            .unwrap_or_else(|| listed.name.to_owned()),
                        id: Some(listed.id.to_string()),
                        file_type: listed
                            .path
                            .parent()
                            .and_then(|p| p.file_name())
                            .and_then(|f| {
                                FileType::ALL
                                    .into_iter()
                                    .find(|t| t.folder() == f.to_string_lossy())
                            })
                            .map(|t| t.as_str().to_owned()),
                    },
                    None => LinkOut {
                        name: link.display.clone().unwrap_or_else(|| link.target.clone()),
                        id: None,
                        file_type: None,
                    },
                }
            }
        }
    }
}
