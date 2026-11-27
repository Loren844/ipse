//! Who the person spends their life with (issue #264): "most often with" counts the
//! events that say something about it. A repeating work meeting and an event with a
//! crowd fill it with the same colleagues every month and say nothing of a life, so
//! they are left out; ties go to whoever was seen in more different events and
//! places. The same rule for the years and months of the timeline, the kinds of
//! events, the home screen's people seen recently and the AI's answer.

use std::collections::{HashMap, HashSet};

use jiff::civil::Weekday;
use jiff::tz::TimeZone;

use super::document::{Document, EventSpan};
use super::{EventRef, SearchIndex};
use crate::vault::{Category, EventKind, EventTime, Id};

/// More people than this in one event make a crowd: a meeting, a party, a class.
pub const CROWD: usize = 6;

/// Times an event not from a calendar's series repeats, same title, weekday and time,
/// to be a repeating event: as many as the agenda's series (issue #175).
const WORK_REPEATS: usize = 3;

/// The rule in words, for an AI that counts who the person sees most.
pub const COMPANY_RULE: &str = "Who the person sees most leaves out the occurrences of a \
repeating work event (a weekly team meeting) and events with more than 6 people; ties go to \
whoever was seen in more different events and places.";

/// The events left out of "most often with", by id.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct LeftOut {
    ids: HashSet<Id>,
}

impl LeftOut {
    /// True when the event `id` does not count in "most often with".
    #[must_use]
    pub fn contains(&self, id: &Id) -> bool {
        self.ids.contains(id)
    }

    /// [`LeftOut::contains`] for an id as text, as an [`crate::rights::Entry`] gives
    /// it; false for text that is not an id.
    #[must_use]
    pub fn contains_text(&self, id: &str) -> bool {
        Id::parse(id).is_some_and(|id| self.ids.contains(&id))
    }

    /// How many events are left out.
    #[must_use]
    pub fn len(&self) -> usize {
        self.ids.len()
    }

    /// True when no event is left out.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.ids.is_empty()
    }
}

impl SearchIndex {
    /// The events among the allowed categories that do not count in "most often
    /// with": every event with more than [`CROWD`] people, and every occurrence of a
    /// repeating event one of whose occurrences is of the `work` kind (the recent ones
    /// of a weekly meeting often have no kind yet). A repeating event is a calendar's
    /// series, or the same title on the same weekday at the same time, at least
    /// 3 times. Two passes over the events, the second only when some
    /// event is of the `work` kind; local times are worked out only for the events
    /// titled as one of those.
    #[must_use]
    pub fn left_out_of_company(&self, allowed: impl Fn(&Category) -> bool) -> LeftOut {
        let mut ids: HashSet<Id> = HashSet::new();
        // The work events' calendar series, and their titles with when they start.
        let mut uids: HashSet<&str> = HashSet::new();
        let mut habits: HashMap<&str, Vec<Habit>> = HashMap::new();
        for (d, span) in self.allowed_events(&allowed) {
            if span.participants.len() > CROWD {
                ids.insert(d.id.clone());
            }
            if span.kind != Some(EventKind::Work) {
                continue;
            }
            match span.series.as_deref() {
                Some(uid) => {
                    uids.insert(uid);
                }
                None => {
                    if let Some(habit) = habit(&span.start) {
                        let known = habits.entry(d.name.text.as_str()).or_default();
                        if !known.contains(&habit) {
                            known.push(habit);
                        }
                    }
                }
            }
        }
        if uids.is_empty() && habits.is_empty() {
            return LeftOut { ids };
        }
        let mut repeats: HashMap<(&str, Habit), Vec<&Id>> = HashMap::new();
        for (d, span) in self.allowed_events(&allowed) {
            if let Some(uid) = span.series.as_deref() {
                if uids.contains(uid) {
                    ids.insert(d.id.clone());
                }
                continue;
            }
            let title = d.name.text.as_str();
            let Some(known) = habits.get(title) else {
                continue;
            };
            if let Some(habit) = habit(&span.start).filter(|h| known.contains(h)) {
                repeats.entry((title, habit)).or_default().push(&d.id);
            }
        }
        for occurrences in repeats.into_values() {
            if occurrences.len() >= WORK_REPEATS {
                ids.extend(occurrences.into_iter().cloned());
            }
        }
        LeftOut { ids }
    }

    /// The `n` people seen most in the events among the allowed categories that
    /// `keep` accepts and are not `left_out`, among those `is_person` keeps: their stem
    /// in lower case and how many events, as the timeline ranks them.
    #[must_use]
    pub fn seen_most(
        &self,
        allowed: impl Fn(&Category) -> bool,
        is_person: impl Fn(&str) -> bool,
        left_out: &LeftOut,
        keep: impl Fn(&EventRef<'_>) -> bool,
        n: usize,
    ) -> Vec<(String, u32)> {
        let mut company = Company::default();
        for (d, span) in self.allowed_events(&allowed) {
            let event = EventRef {
                path: &d.path,
                start: &span.start,
                end: span.end.as_ref(),
                participants: &span.participants,
                place: span.place.as_deref(),
            };
            if !left_out.contains(&d.id) && keep(&event) {
                company.add(d, span);
            }
        }
        company.top(n, is_person)
    }

    /// The events among the allowed categories, with their span.
    fn allowed_events<'i>(
        &'i self,
        allowed: &'i impl Fn(&Category) -> bool,
    ) -> impl Iterator<Item = (&'i Document, &'i EventSpan)> + 'i {
        self.events.iter().filter_map(move |&slot| {
            let d = self.document(slot)?;
            let span = d.event.as_ref()?;
            allowed(&d.category).then_some((d, span))
        })
    }
}

/// The people of the events counted in one period. Each keeps its events' titles and
/// places, a pointer each, so that how varied they were is worked out only for the
/// people who tie at the top.
#[derive(Debug, Default)]
pub(super) struct Company<'i> {
    people: HashMap<&'i str, Vec<SeenAt<'i>>>,
}

/// One event someone was in: its title and its place.
type SeenAt<'i> = (&'i str, Option<&'i str>);

impl<'i> Company<'i> {
    /// Counts the people of the event `d`.
    pub(super) fn add(&mut self, d: &'i Document, span: &'i EventSpan) {
        let seen = (d.name.text.as_str(), span.place.as_deref());
        for person in &span.participants {
            self.people.entry(person.as_str()).or_default().push(seen);
        }
    }

    /// The `n` people seen most among those `is_person` keeps, by their stem in lower
    /// case and how many events: most events first, then the most varied (different
    /// titles and places), then by stem, so that the order never changes.
    pub(super) fn top(&self, n: usize, is_person: impl Fn(&str) -> bool) -> Vec<(String, u32)> {
        let mut people: Vec<(&str, &[SeenAt<'_>])> = self
            .people
            .iter()
            .filter(|(stem, _)| is_person(stem))
            .map(|(stem, seen)| (*stem, seen.as_slice()))
            .collect();
        people.sort_unstable_by(|a, b| b.1.len().cmp(&a.1.len()).then_with(|| a.0.cmp(b.0)));
        // Those seen as often as the last one shown may come before it by variety.
        let Some(last) = people
            .get(n.saturating_sub(1))
            .or(people.last())
            .map(|p| p.1.len())
        else {
            return Vec::new();
        };
        let tied = people.partition_point(|p| p.1.len() >= last);
        let mut contenders: Vec<(&str, usize, usize)> = people[..tied]
            .iter()
            .map(|(stem, seen)| (*stem, seen.len(), variety(seen)))
            .collect();
        contenders.sort_unstable_by(|a, b| {
            b.1.cmp(&a.1)
                .then_with(|| b.2.cmp(&a.2))
                .then_with(|| a.0.cmp(b.0))
        });
        shown(
            contenders
                .into_iter()
                .take(n)
                .map(|(stem, events, _)| (stem, events)),
        )
    }
}

/// When an event starts, as a habit: its weekday and time, in its own offset. `None`
/// for an event of a whole day.
type Habit = (Weekday, i8, i8);

fn habit(start: &EventTime) -> Option<Habit> {
    let EventTime::At(at) = start else {
        return None;
    };
    let local = at.timestamp().to_zoned(TimeZone::fixed(at.offset()));
    Some((local.weekday(), local.hour(), local.minute()))
}

/// How many different titles and places, in someone's events.
fn variety(seen: &[SeenAt<'_>]) -> usize {
    let mut titles: Vec<&str> = seen.iter().map(|s| s.0).collect();
    titles.sort_unstable();
    titles.dedup();
    let mut places: Vec<&str> = seen.iter().filter_map(|s| s.1).collect();
    places.sort_unstable();
    places.dedup();
    titles.len() + places.len()
}

/// The people shown: their stem in lower case and how many events.
fn shown<'a>(people: impl Iterator<Item = (&'a str, usize)>) -> Vec<(String, u32)> {
    people
        .map(|(stem, events)| {
            (
                stem.to_ascii_lowercase(),
                u32::try_from(events).unwrap_or(u32::MAX),
            )
        })
        .collect()
}
