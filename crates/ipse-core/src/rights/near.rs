//! Everything around one file, for the view around someone (issue #244), within a
//! client's categories: what the index holds around it, and the rows of the state logs
//! about it (the photos that show someone, the stays at a place).

use super::Access;
use super::browse::NodeOut;
use crate::search::GraphNode;
use crate::vault::state::{Subject, Topic, parse_table};
use crate::vault::{FileType, Id, MemoryFile};

/// An event that links to the center of [`SurroundingsOut`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HappenedOut {
    /// The event.
    pub node: NodeOut,
    /// Its place, when it is a file this client may read and not the center.
    pub place: Option<NodeOut>,
    /// Its kind, when someone said: `meal`, `trip`…
    pub kind: Option<String>,
}

/// A place or someone found in the center's events.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SharedOut {
    /// The file.
    pub node: NodeOut,
    /// In how many of the center's events.
    pub events: u32,
    /// When the last of them started: `YYYY-MM-DD` or RFC 3339.
    pub last: String,
}

/// Everything around one entity, as this client may read it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SurroundingsOut {
    /// The entity.
    pub center: NodeOut,
    /// Its facts.
    pub facts: Vec<String>,
    /// Its notes.
    pub notes: Option<String>,
    /// The events that link to it, the latest first.
    pub events: Vec<HappenedOut>,
    /// The places of those events, the most frequent first.
    pub places: Vec<SharedOut>,
    /// The people of those events, but the center and the owner, the most frequent
    /// first.
    pub people: Vec<SharedOut>,
    /// The reminders that link to it or name it, by name.
    pub reminders: Vec<NodeOut>,
    /// The other files that link to it, with the relation's word, by name.
    pub linked: Vec<(NodeOut, Option<String>)>,
}

/// What the rows of one topic's state logs say about one file.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SeenOut {
    /// How many observations: photos, stays.
    pub count: u64,
    /// When it was first seen, as written: `YYYY-MM-DD` or RFC 3339.
    pub first: String,
    /// When it was last seen, as written.
    pub last: String,
    /// The category of the log, and whether it is sensitive.
    pub category: String,
    /// True when that category is closed to AIs unless the person opens it.
    pub sensitive: bool,
}

impl Access<'_> {
    /// Everything around the entity `id` among this client's categories (see
    /// [`crate::search::Surroundings`]). `None` when there is no such entity, it is an
    /// event, or this client may not read it.
    #[must_use]
    pub fn surroundings(&self, id: &str) -> Option<SurroundingsOut> {
        let id = Id::parse(id)?;
        let me = self.about_me().and_then(|me| Id::parse(&me.id));
        let found = self
            .gateway
            .index
            .surroundings(&id, me.as_ref(), |c| self.reads(c.as_str()))?;
        let node = |n: GraphNode<'_>| {
            self.returned(n.category.as_str());
            NodeOut {
                id: n.id.to_string(),
                name: n.name.to_owned(),
                file_type: n.file_type.as_str().to_owned(),
                category: n.category.as_str().to_owned(),
                sensitive: self.sensitive(n.category.as_str()),
                depth: n.depth,
                start: n.start.map(ToString::to_string),
            }
        };
        let shared = |s: crate::search::Shared<'_>| SharedOut {
            node: node(s.node),
            events: s.events,
            last: s.last.to_string(),
        };
        Some(SurroundingsOut {
            center: node(found.center),
            facts: found.facts.into_iter().map(str::to_owned).collect(),
            notes: found.notes.map(str::to_owned),
            events: found
                .events
                .into_iter()
                .map(|h| HappenedOut {
                    node: node(h.node),
                    place: h.place.map(node),
                    kind: h.kind.map(|k| k.as_str().to_owned()),
                })
                .collect(),
            places: found.places.into_iter().map(shared).collect(),
            people: found.people.into_iter().map(shared).collect(),
            reminders: found.reminders.into_iter().map(node).collect(),
            linked: found
                .linked
                .into_iter()
                .map(|(n, word)| (node(n), word.map(str::to_owned)))
                .collect(),
        })
    }

    /// What the state logs of `topic` (`photos`, `location`, `home`), every year, say
    /// about the file `stem`, among this client's categories: their rows that link to
    /// it, summed. `None` when no row does.
    #[must_use]
    pub fn seen_in(&self, topic: &str, stem: &str) -> Option<SeenOut> {
        let topic = Topic::new(topic)?;
        let dir = self
            .gateway
            .vault
            .root()
            .join(FileType::StateLog.folder())
            .join(topic.as_str());
        let mut years: Vec<std::path::PathBuf> = std::fs::read_dir(dir)
            .into_iter()
            .flatten()
            .flatten()
            .map(|e| e.path())
            .filter(|p| p.extension().is_some_and(|x| x == "md"))
            .collect();
        years.sort();
        let mut seen: Option<(SeenOut, String, String)> = None;
        for path in years {
            let Ok(file) = MemoryFile::read(&path) else {
                continue;
            };
            let category = file.common().category.as_str().to_owned();
            if !self.reads(&category) || file.state_log().is_none() {
                continue;
            }
            let Ok(table) = parse_table(file.body()) else {
                continue;
            };
            for row in table {
                let Subject::Link(link) = &row.subject else {
                    continue;
                };
                if !link.target.eq_ignore_ascii_case(stem) {
                    continue;
                }
                self.returned(&category);
                let first = row.first_seen.to_string();
                let last = row.last_seen.to_string();
                let (first_key, last_key) = (sortable(&first), sortable(&last));
                match &mut seen {
                    None => {
                        seen = Some((
                            SeenOut {
                                count: row.count,
                                first,
                                last,
                                sensitive: self.sensitive(&category),
                                category: category.clone(),
                            },
                            first_key,
                            last_key,
                        ));
                    }
                    Some((out, low, high)) => {
                        out.count += row.count;
                        if first_key < *low {
                            out.first = first;
                            *low = first_key;
                        }
                        if last_key > *high {
                            out.last = last;
                            *high = last_key;
                        }
                    }
                }
            }
        }
        seen.map(|(out, _, _)| out)
    }
}

/// A day or a moment as text that sorts by its day: its first ten characters.
fn sortable(text: &str) -> String {
    text.chars().take(10).collect()
}
