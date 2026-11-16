//! The neighbourhood of a file, for the graph view (issue #130): what it links to and
//! what links to it, one or two steps away, among the files a client may read. Only
//! the neighbourhood is computed, from the links the index read once per file, so that
//! a memory of tens of thousands of files is drawn as fast as a small one.

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
