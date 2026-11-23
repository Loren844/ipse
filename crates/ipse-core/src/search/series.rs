//! Repeating events, to show them once (issue #175): the occurrences of one calendar
//! event, or, failing that, events with the same title at the same weekday and time
//! at least three weeks in a row. One pass over the events the index read once per
//! file.

use std::collections::HashMap;

use jiff::civil::{Date, Weekday};
use jiff::tz::TimeZone;

use super::SearchIndex;
use crate::vault::{Category, EventTime, Id};

/// Weeks in a row that make a series of events not from a calendar's rule.
const WEEKS_IN_A_ROW: usize = 3;

/// A series an event belongs to.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Series {
    /// The same for every occurrence; says nothing about the events.
    pub key: String,
    /// The weekday every occurrence falls on, when they all do: 1 for Monday to 7.
    pub weekday: Option<i8>,
    /// The time every occurrence starts at, `HH:MM` in its own offset, when they all
    /// do.
    pub time: Option<String>,
    /// How many occurrences it has.
    pub occurrences: u32,
}

/// One occurrence, as the pass reads it.
struct Occurrence<'i> {
    id: &'i Id,
    date: Date,
    /// Weekday and `HH:MM`, for an event at a time.
    at: Option<(Weekday, String)>,
}

impl SearchIndex {
    /// The series of the events among the allowed categories, by event id. An event
    /// that repeats nothing is not in the map.
    #[must_use]
    pub fn series(&self, allowed: impl Fn(&Category) -> bool) -> HashMap<Id, Series> {
        // Calendar series by their `UID`; the rest by title, weekday and time.
        let mut by_uid: HashMap<&str, Vec<Occurrence<'_>>> = HashMap::new();
        let mut by_habit: HashMap<(String, Weekday, String), Vec<Occurrence<'_>>> = HashMap::new();
        for &slot in &self.events {
            let Some(d) = self.document(slot) else {
                continue;
            };
            let Some(span) = d.event.as_ref() else {
                continue;
            };
            if !allowed(&d.category) {
                continue;
            }
            let at = match &span.start {
                EventTime::Day(_) => None,
                EventTime::At(at) => {
                    let local = at.timestamp().to_zoned(TimeZone::fixed(at.offset()));
                    Some((
                        local.weekday(),
                        format!("{:02}:{:02}", local.hour(), local.minute()),
                    ))
                }
            };
            let occurrence = Occurrence {
                id: &d.id,
                date: span.start.date(),
                at: at.clone(),
            };
            if let Some(uid) = span.series.as_deref() {
                by_uid.entry(uid).or_default().push(occurrence);
            } else if let Some((weekday, time)) = at {
                let title = d.name.words.join(" ");
                if !title.is_empty() {
                    by_habit
                        .entry((title, weekday, time))
                        .or_default()
                        .push(occurrence);
                }
            }
        }
        let mut out = HashMap::new();
        for (uid, occurrences) in by_uid {
            if occurrences.len() > 1 {
                let members: Vec<&Occurrence<'_>> = occurrences.iter().collect();
                add(&mut out, &format!("c{uid}"), &members);
            }
        }
        for ((title, weekday, time), mut occurrences) in by_habit {
            if occurrences.len() < WEEKS_IN_A_ROW {
                continue;
            }
            occurrences.sort_by_key(|o| o.date);
            // A habit that pauses (a summer without yoga) stays one series; a single
            // time outside its runs is not part of it.
            let members: Vec<&Occurrence<'_>> =
                weekly_runs(&occurrences).into_iter().flatten().collect();
            if !members.is_empty() {
                let key = blake3::hash(format!("{title}|{weekday:?}|{time}").as_bytes()).to_hex();
                add(&mut out, &format!("h{}", &key[..16]), &members);
            }
        }
        out
    }
}

/// The runs of `occurrences` (by date) one week apart, of at least
/// [`WEEKS_IN_A_ROW`]. Two on the same day count once for the run.
fn weekly_runs<'a, 'i>(occurrences: &'a [Occurrence<'i>]) -> Vec<&'a [Occurrence<'i>]> {
    let mut runs = Vec::new();
    let mut start = 0;
    let mut weeks = 1;
    for i in 1..=occurrences.len() {
        let gap = occurrences
            .get(i)
            .map(|o| (o.date - occurrences[i - 1].date).get_days());
        match gap {
            Some(0) => {}
            Some(7) => weeks += 1,
            _ => {
                if weeks >= WEEKS_IN_A_ROW {
                    runs.push(&occurrences[start..i]);
                }
                start = i;
                weeks = 1;
            }
        }
    }
    runs
}

/// Puts every occurrence of one series in `out`.
fn add(out: &mut HashMap<Id, Series>, key: &str, occurrences: &[&Occurrence<'_>]) {
    let first = occurrences[0].at.as_ref();
    let same = |f: &dyn Fn(&(Weekday, String)) -> bool| {
        occurrences.iter().all(|o| o.at.as_ref().is_some_and(f))
    };
    let weekday = first
        .filter(|(w, _)| same(&|(x, _)| x == w))
        .map(|(w, _)| w.to_monday_one_offset());
    let time = first
        .filter(|(_, t)| same(&|(_, x)| x == t))
        .map(|(_, t)| t.clone());
    let series = Series {
        key: key.to_owned(),
        weekday,
        time,
        occurrences: u32::try_from(occurrences.len()).unwrap_or(u32::MAX),
    };
    for o in occurrences {
        out.insert(o.id.clone(), series.clone());
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn at(day: &str) -> Occurrence<'static> {
        static ID: std::sync::OnceLock<Id> = std::sync::OnceLock::new();
        Occurrence {
            id: ID.get_or_init(Id::generate),
            date: day.parse().unwrap(),
            at: None,
        }
    }

    #[test]
    fn three_weeks_in_a_row_make_a_run() {
        let days = [
            "2026-10-06",
            "2026-10-13",
            "2026-10-20",
            "2026-11-03",
            "2026-11-10",
            "2026-12-01",
            "2026-12-08",
            "2026-12-08",
            "2026-12-15",
        ]
        .map(at);
        let runs = weekly_runs(&days);
        let lengths: Vec<usize> = runs.iter().map(|r| r.len()).collect();
        assert_eq!(
            lengths,
            [3, 4],
            "two weeks are not enough; a day twice counts once"
        );
    }
}
