//! Everything around one file, for the view around someone (issue #244): the events
//! that link to it, the places and the people of those events, the reminders that
//! link to it or name it, and the other files that link to it. One pass over the
//! files the index read once each, as for the graph, so that a memory of tens of
//! thousands of files opens as fast as a small one.

use std::collections::HashMap;

use super::graph::node_of;
use super::{GraphNode, SearchIndex, fold, sort_key};
use crate::vault::{Category, EventKind, EventTime, FileType, Id};

/// An event that links to the center.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Happened<'i> {
    /// The event.
    pub node: GraphNode<'i>,
    /// Its place, when it links to a file that may be read and is not the center.
    pub place: Option<GraphNode<'i>>,
    /// Its kind, when someone said.
    pub kind: Option<EventKind>,
}

/// A file found in the center's events: a place they happened at, someone in them.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Shared<'i> {
    /// The file.
    pub node: GraphNode<'i>,
    /// In how many of the center's events.
    pub events: u32,
    /// When the last of them started.
    pub last: &'i EventTime,
}

/// What the index holds around one file.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Surroundings<'i> {
    /// The file at the center.
    pub center: GraphNode<'i>,
    /// Its facts, as written.
    pub facts: Vec<&'i str>,
    /// Its notes.
    pub notes: Option<&'i str>,
    /// The events that link to it, by their people or their place, the latest first.
    pub events: Vec<Happened<'i>>,
    /// The places of those events, the most frequent first.
    pub places: Vec<Shared<'i>>,
    /// The people of those events, but the center and the owner, the most frequent
    /// first.
    pub people: Vec<Shared<'i>>,
    /// The reminders that link to it or whose name names it, by name.
    pub reminders: Vec<GraphNode<'i>>,
    /// The other files that link to it, with the relation's word, by name.
    pub linked: Vec<(GraphNode<'i>, Option<&'i str>)>,
}

/// A tally of one file met in the center's events.
struct Tally<'i> {
    events: u32,
    last: &'i EventTime,
}

impl Tally<'_> {
    fn add<'i>(map: &mut HashMap<usize, Tally<'i>>, slot: usize, start: &'i EventTime) {
        let tally = map.entry(slot).or_insert(Tally {
            events: 0,
            last: start,
        });
        tally.events += 1;
        if sort_key(start) > sort_key(tally.last) {
            tally.last = start;
        }
    }
}

impl SearchIndex {
    /// Everything around the entity `center` among the allowed categories (see
    /// [`Surroundings`]); `owner`, the owner's own file, is left out of the people.
    /// `None` when there is no such entity, it is an event, or its category is not
    /// allowed.
    #[must_use]
    pub fn surroundings<'i>(
        &'i self,
        center: &Id,
        owner: Option<&Id>,
        allowed: impl Fn(&Category) -> bool,
    ) -> Option<Surroundings<'i>> {
        let start = *self.by_id.get(center)?;
        let doc = self.document(start).filter(|d| allowed(&d.category))?;
        if doc.file_type == FileType::Event {
            return None;
        }
        let stem = doc
            .path
            .file_stem()
            .map(|s| s.to_string_lossy().to_ascii_lowercase())
            .unwrap_or_default();
        let owner_slot = owner.and_then(|id| self.by_id.get(id).copied());
        // The center's names as matched, to find the reminders that name it.
        let names: Vec<String> = doc.names().into_iter().map(|n| format!(" {n} ")).collect();

        let mut events: Vec<(usize, Option<usize>)> = Vec::new();
        let mut places: HashMap<usize, Tally<'_>> = HashMap::new();
        let mut people: HashMap<usize, Tally<'_>> = HashMap::new();
        let mut reminders: Vec<usize> = Vec::new();
        let mut linked: Vec<(usize, Option<&str>)> = Vec::new();
        for (slot, item) in self.slots.iter().enumerate() {
            let Some(d) = item.as_ref().and_then(|s| s.document.as_ref()) else {
                continue;
            };
            if slot == start || !allowed(&d.category) {
                continue;
            }
            let mut links_here = d.links.iter().filter(|l| l.target == stem);
            if let Some(span) = &d.event {
                if links_here.next().is_none() {
                    continue;
                }
                let place = span
                    .place
                    .as_deref()
                    .and_then(|p| self.resolve(&p.to_ascii_lowercase(), &allowed))
                    .filter(|&p| p != start);
                if let Some(place) = place {
                    Tally::add(&mut places, place, &span.start);
                }
                for person in &span.participants {
                    let Some(other) = self.resolve(&person.to_ascii_lowercase(), &allowed) else {
                        continue;
                    };
                    let is_person = self
                        .document(other)
                        .is_some_and(|o| o.file_type == FileType::Person);
                    if other != start && Some(other) != owner_slot && is_person {
                        Tally::add(&mut people, other, &span.start);
                    }
                }
                events.push((slot, place));
                continue;
            }
            let link = links_here.next();
            if d.file_type == FileType::Reminder {
                let words = format!(" {} ", d.name.words.join(" "));
                if link.is_some() || names.iter().any(|n| words.contains(n.as_str())) {
                    reminders.push(slot);
                }
                continue;
            }
            if let Some(link) = link {
                linked.push((slot, link.word.as_deref()));
            }
        }

        let node = |slot: usize| self.document(slot).map(|d| node_of(d, 1));
        let mut events: Vec<Happened<'_>> = events
            .into_iter()
            .filter_map(|(slot, place)| {
                let d = self.document(slot)?;
                Some(Happened {
                    node: node_of(d, 1),
                    place: place.and_then(node),
                    kind: d.event.as_ref().and_then(|e| e.kind),
                })
            })
            .collect();
        events.sort_by(|a, b| {
            let at = |h: &Happened<'_>| h.node.start.map(sort_key);
            at(b).cmp(&at(a)).then_with(|| a.node.id.cmp(b.node.id))
        });
        let mut reminders: Vec<GraphNode<'_>> = reminders.into_iter().filter_map(node).collect();
        reminders.sort_by(by_name);
        let mut linked: Vec<(GraphNode<'_>, Option<&str>)> = linked
            .into_iter()
            .filter_map(|(slot, word)| Some((node(slot)?, word)))
            .collect();
        linked.sort_by(|a, b| by_name(&a.0, &b.0));
        Some(Surroundings {
            center: node_of(doc, 0),
            facts: doc.facts.iter().map(|f| f.text.as_str()).collect(),
            notes: doc.notes.as_ref().map(|n| n.text.as_str()),
            events,
            places: self.shared(places),
            people: self.shared(people),
            reminders,
            linked,
        })
    }

    /// The files tallied in the center's events, the most frequent first, then the
    /// latest, then by name.
    fn shared<'i>(&'i self, map: HashMap<usize, Tally<'i>>) -> Vec<Shared<'i>> {
        let mut found: Vec<Shared<'i>> = map
            .into_iter()
            .filter_map(|(slot, t)| {
                Some(Shared {
                    node: node_of(self.document(slot)?, 1),
                    events: t.events,
                    last: t.last,
                })
            })
            .collect();
        found.sort_by(|a, b| {
            b.events
                .cmp(&a.events)
                .then_with(|| sort_key(b.last).cmp(&sort_key(a.last)))
                .then_with(|| by_name(&a.node, &b.node))
        });
        found
    }
}

/// Two files by name, as matched.
fn by_name(a: &GraphNode<'_>, b: &GraphNode<'_>) -> std::cmp::Ordering {
    fold(a.name).cmp(&fold(b.name))
}
