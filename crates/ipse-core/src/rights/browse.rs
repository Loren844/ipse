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

/// The kinds of entity a memory holds, in the order the interface lists them.
pub const ENTITY_KINDS: [FileType; 7] = [
    FileType::Person,
    FileType::Place,
    FileType::Organization,
    FileType::Project,
    FileType::Work,
    FileType::Thing,
    FileType::Topic,
];

/// Something that links to a file.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Backlink {
    /// The file that links.
    pub from: LinkOut,
    /// For a relation, its word as written in `from`: `sœur` when `from` says
    /// "sœur : [[this file]]". `None` for an event this file takes part in.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub word: Option<String>,
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

/// Most files a graph shows.
pub const MAX_GRAPH: usize = 300;

/// One file of a graph.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct NodeOut {
    /// Its id.
    pub id: String,
    /// Its name, or an event's title.
    pub name: String,
    /// `person`, `place`… or `event`.
    #[serde(rename = "type")]
    pub file_type: String,
    /// Its category.
    pub category: String,
    /// True when its category is closed to AIs unless the person opens it.
    pub sensitive: bool,
    /// Steps from the file at the center: 0 for that file.
    pub depth: u8,
    /// For an event, when it starts: `YYYY-MM-DD` or RFC 3339.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub start: Option<String>,
}

/// A link between two files of a graph.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct EdgeOut {
    /// The place in the nodes of the file the link is written in.
    pub from: usize,
    /// The place of the file it links to.
    pub to: usize,
    /// The relation's word; `None` for an event's place or people.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub word: Option<String>,
}

/// The files around one file, for the graph view.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
pub struct GraphOut {
    /// The file at the center first, then one step away, then two.
    pub nodes: Vec<NodeOut>,
    /// The links between them.
    pub edges: Vec<EdgeOut>,
    /// Files left out to keep the graph readable.
    pub more: usize,
}

impl Access<'_> {
    /// The files around the entity or event `id`, `depth` steps away (1 or 2), among
    /// this client's categories that `categories` keeps (all when `None`) and the types
    /// `kinds` keeps (all entities and events when empty); at most `limit` files
    /// (clamped to 1–[`MAX_GRAPH`]). `None` when there is no such file, or this client
    /// may not read it.
    #[must_use]
    pub fn graph(
        &self,
        id: &str,
        depth: u8,
        kinds: &[FileType],
        categories: Option<&[String]>,
        limit: usize,
    ) -> Option<GraphOut> {
        let id = Id::parse(id)?;
        let kept = |t: FileType, c: &crate::vault::Category| {
            let kind = if kinds.is_empty() {
                t.is_entity() || t == FileType::Event
            } else {
                kinds.contains(&t)
            };
            kind && categories.is_none_or(|keep| keep.iter().any(|k| k == c.as_str()))
        };
        let found = self.gateway.index.neighbourhood(
            &id,
            depth,
            limit.clamp(1, MAX_GRAPH),
            |c| self.reads(c.as_str()),
            kept,
        )?;
        Some(GraphOut {
            nodes: found
                .nodes
                .into_iter()
                .map(|n| NodeOut {
                    id: n.id.to_string(),
                    name: n.name.to_owned(),
                    file_type: n.file_type.as_str().to_owned(),
                    category: n.category.as_str().to_owned(),
                    sensitive: self.sensitive(n.category.as_str()),
                    depth: n.depth,
                    start: n.start.map(ToString::to_string),
                })
                .collect(),
            edges: found
                .edges
                .into_iter()
                .map(|e| EdgeOut {
                    from: e.from,
                    to: e.to,
                    word: e.word.map(str::to_owned),
                })
                .collect(),
            more: found.more,
        })
    }

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
        self.entities(FileType::Person, query, limit, offset)
    }

    /// The entities of `file_type` this client may read: by name, `limit` (clamped to
    /// 1–200) from `offset`; or, with a `query`, those matching it, best first.
    #[must_use]
    pub fn entities(
        &self,
        file_type: FileType,
        query: Option<&str>,
        limit: usize,
        offset: usize,
    ) -> Lines {
        let limit = limit.clamp(1, MAX_LISTED);
        let query = query.map(str::trim).filter(|q| !q.is_empty());
        if let Some(query) = query {
            let found = self.search(query, crate::search::MAX_LIMIT);
            let items: Vec<Line> = found
                .hits
                .into_iter()
                .filter(|h| h.file_type == file_type)
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
            .entities(file_type, |c| self.reads(c.as_str()));
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

    /// How many files of each kind of entity this client may read, in the order of
    /// [`ENTITY_KINDS`].
    #[must_use]
    pub fn kinds(&self) -> Vec<(FileType, usize)> {
        ENTITY_KINDS
            .iter()
            .map(|&t| (t, self.gateway.index.count(t, |c| self.reads(c.as_str()))))
            .collect()
    }

    /// What links to the entity or event with this id, among the files this client
    /// may read: other entities by a relation (with its word), events by their people
    /// or place. Entities first, by name; then events, most recent first.
    #[must_use]
    pub fn backlinks(&self, id: &str) -> Vec<Backlink> {
        let Some(parsed) = Id::parse(id) else {
            return Vec::new();
        };
        let Some(path) = self
            .gateway
            .index
            .path_of(&parsed, |c| self.reads(c.as_str()))
        else {
            return Vec::new();
        };
        let Some(stem) = path.file_stem().map(|s| s.to_string_lossy().into_owned()) else {
            return Vec::new();
        };
        self.gateway
            .index
            .linking_to(&stem, |c| self.reads(c.as_str()))
            .into_iter()
            .map(|(listed, word)| {
                self.returned(listed.category.as_str());
                Backlink {
                    from: LinkOut {
                        name: listed.name.to_owned(),
                        id: Some(listed.id.to_string()),
                        file_type: type_of(listed.path),
                    },
                    word: word.map(str::to_owned),
                }
            })
            .collect()
    }

    /// The file of the entity or event with this id, relative to the vault with `/`,
    /// if this client may read it: for the person to edit it.
    #[must_use]
    pub fn file_of(&self, id: &str) -> Option<String> {
        let parsed = Id::parse(id)?;
        let path = self
            .gateway
            .index
            .path_of(&parsed, |c| self.reads(c.as_str()))?;
        Some(
            path.components()
                .map(|c| c.as_os_str().to_string_lossy())
                .collect::<Vec<_>>()
                .join("/"),
        )
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

    /// The file at `file`, relative to the vault with `/`, resolved like a link: its
    /// name, id and type; `None` when there is none this client may read.
    #[must_use]
    pub fn at(&self, file: &str) -> Option<LinkOut> {
        let listed = self.gateway.index.at(file, |c| self.reads(c.as_str()))?;
        Some(LinkOut {
            name: listed.name.to_owned(),
            id: Some(listed.id.to_string()),
            file_type: type_of(listed.path),
        })
    }

    /// The places linked from the events this client may read (issue #135), the most
    /// visited first, with the readable people seen there most.
    #[must_use]
    pub fn places_visited(&self) -> Vec<crate::search::Visited<'_>> {
        let index = &self.gateway.index;
        index.places_visited(
            |c| self.reads(c.as_str()),
            |stem| {
                index
                    .linked(stem, |c| self.reads(c.as_str()))
                    .is_some_and(|l| l.path.starts_with("people"))
            },
        )
    }

    /// The places this client may read, with their files: for the map.
    #[must_use]
    pub fn places(&self) -> Vec<crate::search::Listed<'_>> {
        self.gateway
            .index
            .entities(FileType::Place, |c| self.reads(c.as_str()))
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
                        file_type: type_of(listed.path),
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

/// The type of the file at `path`, relative to the vault, from its first folder
/// (`events/2026/…` is an event).
fn type_of(path: &std::path::Path) -> Option<String> {
    let folder = path.components().next()?.as_os_str().to_string_lossy();
    FileType::ALL
        .into_iter()
        .find(|t| t.folder() == folder)
        .map(|t| t.as_str().to_owned())
}
