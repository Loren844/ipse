//! Where one has been, for the map (issue #135): for each place linked from events,
//! how many events happened there, the last one, and the people seen there most. One
//! pass over the events the index read once per file.

use std::collections::HashMap;

use super::SearchIndex;
use crate::vault::{Category, EventTime};

/// People listed for a place.
const PLACE_PEOPLE: usize = 5;

/// Someone met at a place: their file stem, and in how many of its events.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Met {
    /// The person's file stem, as linked from events, in lower case.
    pub stem: String,
    /// Events there they are in.
    pub events: u32,
}

/// What happened at one place.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Visited<'i> {
    /// The place's file stem, as events link it, in lower case.
    pub stem: String,
    /// Events there.
    pub events: u32,
    /// When the last one started.
    pub last: &'i EventTime,
    /// The people in most of its events, most first.
    pub people: Vec<Met>,
}

impl SearchIndex {
    /// The places linked from the events among the allowed categories, the most
    /// visited first; people among those `is_person` keeps.
    #[must_use]
    pub fn places_visited(
        &self,
        allowed: impl Fn(&Category) -> bool,
        is_person: impl Fn(&str) -> bool,
    ) -> Vec<Visited<'_>> {
        struct Tally<'i> {
            events: u32,
            last: &'i EventTime,
            people: HashMap<&'i str, u32>,
        }
        let mut places: HashMap<String, Tally<'_>> = HashMap::new();
        for &slot in &self.events {
            let Some(d) = self.document(slot) else {
                continue;
            };
            let Some(span) = d.event.as_ref() else {
                continue;
            };
            let Some(place) = span.place.as_deref() else {
                continue;
            };
            if !allowed(&d.category) {
                continue;
            }
            let tally = places
                .entry(place.to_ascii_lowercase())
                .or_insert_with(|| Tally {
                    events: 0,
                    last: &span.start,
                    people: HashMap::new(),
                });
            tally.events += 1;
            if span.start.date() > tally.last.date() {
                tally.last = &span.start;
            }
            for person in &span.participants {
                *tally.people.entry(person.as_str()).or_default() += 1;
            }
        }
        let mut found: Vec<Visited<'_>> = places
            .into_iter()
            .map(|(stem, tally)| {
                let mut people: Vec<(&str, u32)> = tally
                    .people
                    .into_iter()
                    .filter(|(p, _)| is_person(p))
                    .collect();
                people.sort_unstable_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.cmp(b.0)));
                Visited {
                    stem,
                    events: tally.events,
                    last: tally.last,
                    people: people
                        .into_iter()
                        .take(PLACE_PEOPLE)
                        .map(|(p, events)| Met {
                            stem: p.to_ascii_lowercase(),
                            events,
                        })
                        .collect(),
                }
            })
            .collect();
        found.sort_unstable_by(|a, b| b.events.cmp(&a.events).then_with(|| a.stem.cmp(&b.stem)));
        found
    }
}
