//! One kind of event at a glance (issue #134): how many there were, the next and the
//! last ones, the people and the places they are with most. One pass over the events
//! the index read once per file. And the birthdays the person files give.

use std::collections::HashMap;

use jiff::civil::Date;

use super::SearchIndex;
use crate::vault::{Birthday, Category, EventKind, EventTime, FileType, Id};

/// A file stem, and in how many events of the kind it is linked.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Often {
    /// The file stem, as events link it, in lower case.
    pub stem: String,
    /// Events of the kind it is in.
    pub events: u32,
}

/// The events of one kind.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct KindSummary<'i> {
    /// How many there are.
    pub events: u32,
    /// The first ones starting on or after the day given, by start.
    pub next: Vec<(&'i Id, &'i EventTime)>,
    /// The last ones starting before it, the most recent first.
    pub last: Vec<(&'i Id, &'i EventTime)>,
    /// The people in most of them, most first.
    pub people: Vec<Often>,
    /// The places most of them happen at, most first.
    pub places: Vec<Often>,
}

/// A person's birthday, from their file.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Born<'i> {
    /// The person's id.
    pub id: &'i Id,
    /// Their name.
    pub name: &'i str,
    /// Their birthday.
    pub birthday: Birthday,
}

impl SearchIndex {
    /// The events of `kind` among the allowed categories, around `today`: at most
    /// `shown` next and last ones, people (among those `is_person` keeps) and places.
    #[must_use]
    pub fn kind_summary(
        &self,
        kind: EventKind,
        today: Date,
        shown: usize,
        allowed: impl Fn(&Category) -> bool,
        is_person: impl Fn(&str) -> bool,
    ) -> KindSummary<'_> {
        let mut events = 0;
        let mut found: Vec<(&Id, &EventTime)> = Vec::new();
        let mut people: HashMap<String, u32> = HashMap::new();
        let mut places: HashMap<String, u32> = HashMap::new();
        for &slot in &self.events {
            let Some(d) = self.document(slot) else {
                continue;
            };
            let Some(span) = d.event.as_ref() else {
                continue;
            };
            if span.kind != Some(kind) || !allowed(&d.category) {
                continue;
            }
            events += 1;
            found.push((&d.id, &span.start));
            for person in &span.participants {
                *people.entry(person.to_ascii_lowercase()).or_default() += 1;
            }
            if let Some(place) = &span.place {
                *places.entry(place.to_ascii_lowercase()).or_default() += 1;
            }
        }
        found.sort_by_key(|&(id, start)| (start.date(), super::sort_key(start), id));
        let split = found.partition_point(|(_, start)| start.date() < today);
        let next = found[split..].iter().take(shown).copied().collect();
        let last = found[..split].iter().rev().take(shown).copied().collect();
        KindSummary {
            events,
            next,
            last,
            people: most(people.into_iter().filter(|(p, _)| is_person(p)), shown),
            places: most(places.into_iter(), shown),
        }
    }

    /// The people among the allowed categories whose file gives a birthday.
    #[must_use]
    pub fn birthdays(&self, allowed: impl Fn(&Category) -> bool) -> Vec<Born<'_>> {
        self.documents()
            .filter(|d| d.file_type == FileType::Person && allowed(&d.category))
            .filter_map(|d| {
                Some(Born {
                    id: &d.id,
                    name: &d.name.text,
                    birthday: d.birthday?,
                })
            })
            .collect()
    }
}

/// The `shown` stems counted most, most first, then by name.
fn most(counts: impl Iterator<Item = (String, u32)>, shown: usize) -> Vec<Often> {
    let mut counts: Vec<(String, u32)> = counts.collect();
    counts.sort_unstable_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.cmp(&b.0)));
    counts
        .into_iter()
        .take(shown)
        .map(|(stem, events)| Often { stem, events })
        .collect()
}

/// The next day, from `today` on, that the birthday falls on; February 29 is kept
/// on February 28 in other years.
#[must_use]
pub fn next_birthday(birthday: Birthday, today: Date) -> Date {
    let on = |year: i16| {
        Date::new(year, birthday.month(), birthday.day())
            .or_else(|_| Date::new(year, birthday.month(), birthday.day() - 1))
            .unwrap_or(today)
    };
    let this_year = on(today.year());
    if this_year >= today {
        this_year
    } else {
        on(today.year() + 1)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_next_birthday_is_this_year_or_the_next() {
        let today = jiff::civil::date(2026, 11, 23);
        let day = |text: &str| next_birthday(Birthday::parse(text).unwrap(), today);
        assert_eq!(day("1990-12-02"), jiff::civil::date(2026, 12, 2));
        assert_eq!(day("--11-23"), today, "today counts");
        assert_eq!(day("1985-03-12"), jiff::civil::date(2027, 3, 12));
        assert_eq!(day("2000-02-29"), jiff::civil::date(2027, 2, 28));
    }
}
