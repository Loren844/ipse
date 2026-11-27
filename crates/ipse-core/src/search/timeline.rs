//! A life over time, for the timeline view (issue #133): for each year and month, how
//! many events, the people seen most and what marked the period (trips, celebrations).
//! One pass over the events the index read once per file, so that years of agenda
//! are summed up as fast as a week.

use std::collections::{BTreeMap, HashMap};
use std::path::Path;

use super::SearchIndex;
use crate::vault::{Category, EventKind, EventTime, Id};

/// People listed for a month.
const MONTH_PEOPLE: usize = 3;

/// People listed for a year.
const YEAR_PEOPLE: usize = 5;

/// Events that marked a month, at most.
const MONTH_MARKS: usize = 4;

/// The kinds of events that mark a period.
const MARKING: [EventKind; 2] = [EventKind::Trip, EventKind::Celebration];

/// An event that marked a period.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Mark<'i> {
    /// Its id.
    pub id: &'i Id,
    /// Its title.
    pub title: &'i str,
    /// When it starts.
    pub start: &'i EventTime,
    /// What kind of event it is.
    pub kind: EventKind,
}

/// Someone seen in a period: the file stem of their entity, and in how many events.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Seen {
    /// The person's file stem, as linked from events, in lower case.
    pub stem: String,
    /// Events of the period they are in.
    pub events: u32,
}

/// One month that has events.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MonthLine<'i> {
    /// 1 to 12.
    pub month: i8,
    /// Its events.
    pub events: u32,
    /// The people in most of its events, most first.
    pub people: Vec<Seen>,
    /// Its trips and celebrations, in order.
    pub marks: Vec<Mark<'i>>,
}

/// One year that has events.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct YearLine<'i> {
    /// The year.
    pub year: i16,
    /// Its events.
    pub events: u32,
    /// The people in most of its events, most first.
    pub people: Vec<Seen>,
    /// Its months that have events, in order.
    pub months: Vec<MonthLine<'i>>,
}

/// Every year that has events, the most recent first.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Timeline<'i> {
    /// The years.
    pub years: Vec<YearLine<'i>>,
}

/// Counts for one period while the events are read.
#[derive(Default)]
struct Tally<'i> {
    events: u32,
    people: HashMap<&'i str, u32>,
    marks: Vec<Mark<'i>>,
}

impl<'i> Tally<'i> {
    fn add(&mut self, participants: &'i [String], mark: Option<&Mark<'i>>) {
        self.events += 1;
        for person in participants {
            *self.people.entry(person.as_str()).or_default() += 1;
        }
        if let Some(mark) = mark {
            self.marks.push(mark.clone());
        }
    }

    /// The `n` people seen most, ties by stem, so that the order never changes.
    fn top(&self, n: usize, is_person: impl Fn(&str) -> bool) -> Vec<Seen> {
        let mut people: Vec<(&str, u32)> = self
            .people
            .iter()
            .filter(|(stem, _)| is_person(stem))
            .map(|(stem, count)| (*stem, *count))
            .collect();
        people.sort_unstable_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.cmp(b.0)));
        people
            .into_iter()
            .take(n)
            .map(|(stem, events)| Seen {
                stem: stem.to_ascii_lowercase(),
                events,
            })
            .collect()
    }
}

impl SearchIndex {
    /// The timeline of the events among the allowed categories: per year and month,
    /// counts, the people seen most (among those `is_person` keeps, the readable
    /// people) and the trips and celebrations. An event counts in the month it
    /// starts.
    #[must_use]
    pub fn timeline(
        &self,
        allowed: impl Fn(&Category) -> bool,
        is_person: impl Fn(&str) -> bool,
    ) -> Timeline<'_> {
        self.timeline_with(allowed, is_person, None)
    }

    /// The timeline of [`SearchIndex::timeline`], of the events that link to the file
    /// `with` (a stem) when one is given (issue #244): a life with someone, or at a
    /// place.
    #[must_use]
    pub fn timeline_with(
        &self,
        allowed: impl Fn(&Category) -> bool,
        is_person: impl Fn(&str) -> bool,
        with: Option<&str>,
    ) -> Timeline<'_> {
        let with = with.map(str::to_ascii_lowercase);
        let mut years: BTreeMap<i16, (Tally<'_>, BTreeMap<i8, Tally<'_>>)> = BTreeMap::new();
        for &slot in &self.events {
            let Some(d) = self.document(slot) else {
                continue;
            };
            let Some(span) = d.event.as_ref() else {
                continue;
            };
            if !allowed(&d.category)
                || with
                    .as_deref()
                    .is_some_and(|w| !d.links.iter().any(|l| l.target == w))
            {
                continue;
            }
            let date = span.start.date();
            let mark = span.kind.filter(|k| MARKING.contains(k)).map(|kind| Mark {
                id: &d.id,
                title: &d.name.text,
                start: &span.start,
                kind,
            });
            let (year, months) = years.entry(date.year()).or_default();
            year.add(&span.participants, None);
            months
                .entry(date.month())
                .or_default()
                .add(&span.participants, mark.as_ref());
        }
        Timeline {
            years: years
                .into_iter()
                .rev()
                .map(|(year, (tally, months))| YearLine {
                    year,
                    events: tally.events,
                    people: tally.top(YEAR_PEOPLE, &is_person),
                    months: months
                        .into_iter()
                        .map(|(month, mut tally)| {
                            tally.marks.sort_by_key(|m| sort_start(m.start));
                            tally.marks.truncate(MONTH_MARKS);
                            MonthLine {
                                month,
                                events: tally.events,
                                people: tally.top(MONTH_PEOPLE, &is_person),
                                marks: tally.marks,
                            }
                        })
                        .collect(),
                })
                .collect(),
        }
    }

    /// The file of `stem` when it is a person among the allowed categories.
    #[must_use]
    pub fn person_path(&self, stem: &str, allowed: impl Fn(&Category) -> bool) -> Option<&Path> {
        self.by_stem
            .get(&stem.to_ascii_lowercase())?
            .iter()
            .filter_map(|&slot| self.document(slot))
            .find(|d| d.file_type == crate::vault::FileType::Person && allowed(&d.category))
            .map(|d| d.path.as_path())
    }
}

/// A start that sorts: the date, then the instant when there is one.
fn sort_start(start: &EventTime) -> (jiff::civil::Date, Option<jiff::Timestamp>) {
    match start {
        EventTime::Day(day) => (*day, None),
        EventTime::At(at) => (start.date(), Some(at.timestamp())),
    }
}
