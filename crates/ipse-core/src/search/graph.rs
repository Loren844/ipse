//! The neighbourhood of a file, for the graph view (issue #130): what it links to and
//! what links to it, one or two steps away, among the files a client may read. Only
//! the neighbourhood is computed, from the links the index read once per file, so that
//! a memory of tens of thousands of files is drawn as fast as a small one. And a file
//! with the files directly linked to it (issue #213), two files in the same events
//! being linked by how many there are, in one pass.

use std::collections::{HashMap, HashSet};
use std::path::Path;

use super::{SearchIndex, fold, sort_key};
use crate::vault::{Category, EventTime, FileType, Id};

/// One file of a neighbourhood.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GraphNode<'i> {
    /// Its id.
    pub id: &'i Id,
    /// Its name, or an event's title.
    pub name: &'i str,
    /// What it is.
    pub file_type: FileType,
    /// Its rights category.
    pub category: &'i Category,
    /// Its file, relative to the vault's folder.
    pub path: &'i Path,
    /// Steps from the file at the center: 0 for that file.
    pub depth: u8,
    /// For an event, when it starts.
    pub start: Option<&'i EventTime>,
}

/// A link between two files of a neighbourhood, by their places in its nodes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GraphEdge<'i> {
    /// The file the link is written in.
    pub from: usize,
    /// The file it links to.
    pub to: usize,
    /// The relation's word; `None` for an event's place or people.
    pub word: Option<&'i str>,
}

/// The files around one file, and the links between them.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Neighbourhood<'i> {
    /// The file at the center first, then by steps away: at each step, entities by
    /// name, then events, the most recent first.
    pub nodes: Vec<GraphNode<'i>>,
    /// Every link between two of its files, each once per direction.
    pub edges: Vec<GraphEdge<'i>>,
    /// Files left out to keep it under the limit.
    pub more: usize,
}

/// The order of a step's files: entities by name, then events, the most recent first.
#[derive(Debug, PartialEq, Eq, PartialOrd, Ord)]
enum Key {
    Entity(String),
    Event(std::cmp::Reverse<jiff::Timestamp>),
}

impl SearchIndex {
    /// The neighbourhood of the file `center`, `depth` steps away (1 or 2), through
    /// the files of the allowed categories that `keep` keeps (by type and category),
    /// at most `limit` files. `None` when there is no such file, or its category is not
    /// allowed; `keep` does not apply to it.
    #[must_use]
    pub fn neighbourhood(
        &self,
        center: &Id,
        depth: u8,
        limit: usize,
        allowed: impl Fn(&Category) -> bool,
        keep: impl Fn(FileType, &Category) -> bool,
    ) -> Option<Neighbourhood<'_>> {
        let start = *self.by_id.get(center)?;
        if !allowed(&self.document(start)?.category) {
            return None;
        }
        let limit = limit.max(1);
        let mut depth_of: HashMap<usize, u8> = HashMap::from([(start, 0)]);
        let mut order = vec![start];
        let mut frontier = vec![start];
        let mut more = 0;
        for step in 1..=depth.clamp(1, 2) {
            let mut found: HashSet<usize> = HashSet::new();
            let fits = |slot: usize, found: &HashSet<usize>| {
                !depth_of.contains_key(&slot)
                    && !found.contains(&slot)
                    && self
                        .document(slot)
                        .is_some_and(|d| allowed(&d.category) && keep(d.file_type, &d.category))
            };
            // What the frontier links to.
            for &slot in &frontier {
                let Some(doc) = self.document(slot) else {
                    continue;
                };
                for link in &doc.links {
                    if let Some(target) = self.resolve(&link.target, &allowed)
                        && fits(target, &found)
                    {
                        found.insert(target);
                    }
                }
            }
            // What links to the frontier: one pass over every file.
            let stems: HashSet<String> = frontier
                .iter()
                .filter_map(|&slot| self.document(slot))
                .filter_map(|d| d.path.file_stem())
                .map(|s| s.to_string_lossy().to_ascii_lowercase())
                .collect();
            for (slot, entry) in self.slots.iter().enumerate() {
                let Some(doc) = entry.as_ref().and_then(|s| s.document.as_ref()) else {
                    continue;
                };
                if !doc.links.is_empty()
                    && doc.links.iter().any(|l| stems.contains(&l.target))
                    && fits(slot, &found)
                {
                    found.insert(slot);
                }
            }
            let mut next: Vec<usize> = found.into_iter().collect();
            self.sort_for_graph(&mut next);
            let room = limit.saturating_sub(order.len());
            if next.len() > room {
                more += next.len() - room;
                next.truncate(room);
            }
            for &slot in &next {
                depth_of.insert(slot, step);
                order.push(slot);
            }
            frontier = next;
            if frontier.is_empty() {
                break;
            }
        }
        Some(self.graph_of(&order, &depth_of, more))
    }

    /// The file a `[[stem]]` (in lower case) points to among the allowed categories.
    fn resolve(&self, stem: &str, allowed: &impl Fn(&Category) -> bool) -> Option<usize> {
        self.by_stem
            .get(stem)?
            .iter()
            .copied()
            .find(|&slot| self.document(slot).is_some_and(|d| allowed(&d.category)))
    }

    /// Entities by name, then events, the most recent first.
    fn sort_for_graph(&self, slots: &mut [usize]) {
        let mut keys: Vec<(usize, Key)> = slots
            .iter()
            .filter_map(|&slot| {
                let d = self.document(slot)?;
                Some(match &d.event {
                    Some(e) => (slot, Key::Event(std::cmp::Reverse(sort_key(&e.start)))),
                    None => (slot, Key::Entity(fold(&d.name.text))),
                })
            })
            .collect();
        keys.sort_by(|a, b| a.1.cmp(&b.1));
        for (at, (slot, _)) in keys.into_iter().enumerate() {
            slots[at] = slot;
        }
    }

    /// The nodes of `order` and every link between two of them.
    fn graph_of(
        &self,
        order: &[usize],
        depth_of: &HashMap<usize, u8>,
        more: usize,
    ) -> Neighbourhood<'_> {
        let place: HashMap<usize, usize> = order.iter().enumerate().map(|(i, &s)| (s, i)).collect();
        let mut nodes = Vec::with_capacity(order.len());
        let mut edges = Vec::new();
        let mut seen: HashSet<(usize, usize)> = HashSet::new();
        for (from, &slot) in order.iter().enumerate() {
            let Some(d) = self.document(slot) else {
                continue;
            };
            nodes.push(GraphNode {
                id: &d.id,
                name: &d.name.text,
                file_type: d.file_type,
                category: &d.category,
                path: &d.path,
                depth: depth_of.get(&slot).copied().unwrap_or(0),
                start: d.event.as_ref().map(|e| &e.start),
            });
            for link in &d.links {
                let Some(to) = self
                    .by_stem
                    .get(&link.target)
                    .and_then(|slots| slots.iter().find_map(|s| place.get(s).copied()))
                else {
                    continue;
                };
                if to != from && seen.insert((from, to)) {
                    edges.push(GraphEdge {
                        from,
                        to,
                        word: link.word.as_deref(),
                    });
                }
            }
        }
        Neighbourhood { nodes, edges, more }
    }
}

/// A file linked to the file at the center of [`Around`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Neighbour<'i> {
    /// The file.
    pub node: GraphNode<'i>,
    /// The relation's words as the center's file writes them: "mère : [[it]]".
    pub words: Vec<&'i str>,
    /// The relation's words as its own file writes them about the center.
    pub back: Vec<&'i str>,
    /// How many events of the period both are in.
    pub together: u32,
    /// When the last of those events started.
    pub last: Option<&'i EventTime>,
}

impl Neighbour<'_> {
    /// How strongly it is linked: a relation written in either file counts as much as
    /// five events together.
    fn strength(&self) -> u32 {
        let written = !self.words.is_empty() || !self.back.is_empty();
        self.together + if written { 5 } else { 1 }
    }
}

/// A file and the files directly linked to it, for the graph centred on one file (issue
/// #213). Events are not neighbours of an entity: two files in the same events are
/// linked by how many there are.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Around<'i> {
    /// The file at the center.
    pub center: GraphNode<'i>,
    /// Its neighbours, the most strongly linked first, then by name.
    pub neighbours: Vec<Neighbour<'i>>,
}

impl SearchIndex {
    /// The file `center` and the files linked to it, among the allowed categories.
    /// `owner` is the owner's own file: every event counts as theirs. Only the events
    /// that start within `period` (inclusive) are counted. With no `center`, the owner's
    /// file; with no owner either, the entity in the most events. `None` when there is
    /// no such file, or its category is not allowed.
    #[must_use]
    pub fn around<'i>(
        &'i self,
        center: Option<&Id>,
        owner: Option<&Id>,
        period: (jiff::Timestamp, jiff::Timestamp),
        allowed: impl Fn(&Category) -> bool,
    ) -> Option<Around<'i>> {
        let readable = |slot: usize| self.document(slot).filter(|d| allowed(&d.category));
        let owner_slot = owner
            .and_then(|id| self.by_id.get(id).copied())
            .filter(|&s| readable(s).is_some());
        let start = match center {
            Some(id) => *self.by_id.get(id)?,
            None => owner_slot.or_else(|| self.busiest(&allowed))?,
        };
        let doc = readable(start)?;
        let stem = doc
            .path
            .file_stem()
            .map(|s| s.to_string_lossy().to_ascii_lowercase())
            .unwrap_or_default();
        let is_event = doc.file_type == FileType::Event;
        let everything = Some(start) == owner_slot;
        let within = |time: &EventTime| {
            let at = sort_key(time);
            period.0 <= at && at <= period.1
        };

        let mut found: HashMap<usize, Neighbour<'i>> = HashMap::new();
        let entry = |slot: usize, found: &mut HashMap<usize, Neighbour<'i>>| -> bool {
            if slot == start {
                return false;
            }
            let Some(d) = readable(slot) else {
                return false;
            };
            if !is_event && d.file_type == FileType::Event {
                return false;
            }
            found.entry(slot).or_insert_with(|| Neighbour {
                node: node_of(d, 1),
                words: Vec::new(),
                back: Vec::new(),
                together: 0,
                last: None,
            });
            true
        };
        // What the center links to.
        for link in &doc.links {
            if let Some(target) = self.resolve(&link.target, &allowed)
                && entry(target, &mut found)
                && let (Some(word), Some(n)) = (link.word.as_deref(), found.get_mut(&target))
                && !n.words.contains(&word)
            {
                n.words.push(word);
            }
        }
        // What links to it, and the events it is in: one pass over every file.
        for (slot, item) in self.slots.iter().enumerate() {
            let Some(d) = item.as_ref().and_then(|s| s.document.as_ref()) else {
                continue;
            };
            if slot == start || !allowed(&d.category) {
                continue;
            }
            let links_here = d.links.iter().filter(|l| l.target == stem);
            if d.file_type == FileType::Event && !is_event {
                let Some(event) = &d.event else { continue };
                let mine = everything || d.links.iter().any(|l| l.target == stem);
                if !mine || !within(&event.start) {
                    continue;
                }
                // Every event is the owner's, whether it names them or not.
                let others = d
                    .links
                    .iter()
                    .filter_map(|link| self.resolve(&link.target, &allowed))
                    .chain(owner_slot)
                    .collect::<HashSet<usize>>();
                for other in others {
                    if entry(other, &mut found)
                        && let Some(n) = found.get_mut(&other)
                    {
                        n.together += 1;
                        if n.last.is_none_or(|l| sort_key(l) < sort_key(&event.start)) {
                            n.last = Some(&event.start);
                        }
                    }
                }
                continue;
            }
            for link in links_here {
                if entry(slot, &mut found)
                    && let (Some(word), Some(n)) = (link.word.as_deref(), found.get_mut(&slot))
                    && !n.back.contains(&word)
                {
                    n.back.push(word);
                }
            }
        }
        let mut neighbours: Vec<Neighbour<'i>> = found.into_values().collect();
        neighbours.sort_by(|a, b| {
            b.strength()
                .cmp(&a.strength())
                .then_with(|| fold(a.node.name).cmp(&fold(b.node.name)))
        });
        Some(Around {
            center: node_of(doc, 0),
            neighbours,
        })
    }

    /// The entity in the most events, among the allowed categories.
    fn busiest(&self, allowed: &impl Fn(&Category) -> bool) -> Option<usize> {
        let mut count: HashMap<usize, u32> = HashMap::new();
        for item in &self.slots {
            let Some(d) = item.as_ref().and_then(|s| s.document.as_ref()) else {
                continue;
            };
            if d.file_type != FileType::Event || !allowed(&d.category) {
                continue;
            }
            for link in &d.links {
                if let Some(slot) = self.resolve(&link.target, allowed) {
                    *count.entry(slot).or_default() += 1;
                }
            }
        }
        count
            .into_iter()
            .max_by_key(|&(slot, n)| (n, std::cmp::Reverse(slot)))
            .map(|(slot, _)| slot)
    }
}

/// A file of a graph, `depth` steps from its center.
fn node_of(d: &super::document::Document, depth: u8) -> GraphNode<'_> {
    GraphNode {
        id: &d.id,
        name: &d.name.text,
        file_type: d.file_type,
        category: &d.category,
        path: &d.path,
        depth,
        start: d.event.as_ref().map(|e| &e.start),
    }
}
