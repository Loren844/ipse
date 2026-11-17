//! The keys of an event file (`docs/spec/vault.md`, section 4).

use std::fmt;

use jiff::civil::Date;
use jiff::tz::TimeZone;

use super::fields::{DateTime, LinkOrText};

/// When an event starts or ends: a whole day (`2026-11-14`), or a date and time with
/// its offset (`2026-11-14T20:00:00+01:00`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EventTime {
    /// A whole day: an all-day event.
    Day(Date),
    /// A moment, with the offset it was written with.
    At(DateTime),
}

impl EventTime {
    /// Reads `YYYY-MM-DD` or an RFC 3339 date and time with its offset. Returns `None`
    /// for any other form.
    #[must_use]
    pub fn parse(text: &str) -> Option<Self> {
        if text.len() == 10 {
            let date: Date = text.parse().ok()?;
            // jiff accepts other forms of a date; only `YYYY-MM-DD` is the format.
            return (date.to_string() == text).then_some(Self::Day(date));
        }
        DateTime::parse(text).map(Self::At)
    }

    /// The day it falls on, in its own offset: the day in an event's path.
    #[must_use]
    pub fn date(&self) -> Date {
        match self {
            Self::Day(date) => *date,
            Self::At(at) => at.timestamp().to_zoned(TimeZone::fixed(at.offset())).date(),
        }
    }

    /// True for a whole day.
    #[must_use]
    pub fn is_day(&self) -> bool {
        matches!(self, Self::Day(_))
    }
}

impl fmt::Display for EventTime {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Day(date) => date.fmt(f),
            Self::At(at) => at.fmt(f),
        }
    }
}

/// Where an event stands.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EventStatus {
    /// It will happen; the default for future events.
    Planned,
    /// It happened.
    Done,
    /// It was called off.
    Cancelled,
}

impl EventStatus {
    /// `planned`, `done` or `cancelled`.
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Planned => "planned",
            Self::Done => "done",
            Self::Cancelled => "cancelled",
        }
    }

    /// Reads `planned`, `done` or `cancelled`.
    #[must_use]
    pub fn parse(text: &str) -> Option<Self> {
        Some(match text {
            "planned" => Self::Planned,
            "done" => Self::Done,
            "cancelled" => Self::Cancelled,
            _ => return None,
        })
    }
}

impl fmt::Display for EventStatus {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// What kind of event it is, from a fixed vocabulary, so that the interface can show
/// meals, trips or appointments apart (issue #131). Set by the person, or proposed by
/// their AI and accepted.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum EventKind {
    /// A meal: lunch, dinner, a coffee.
    Meal,
    /// A celebration: a birthday, a party, a wedding.
    Celebration,
    /// An appointment that is not about health: the bank, the hairdresser, a repair,
    /// a paper to file.
    Appointment,
    /// Health: a doctor, a dentist, a therapist, a test.
    Health,
    /// Work: a meeting, a work trip day, a conference.
    Work,
    /// Learning: a class, an exam, a training.
    Study,
    /// Sport, played or watched.
    Sport,
    /// Culture: a concert, a film, a play, a museum, a book club.
    Culture,
    /// Going out with others, without a meal at its heart: drinks, a walk, a visit.
    Outing,
    /// A stay away from home: holidays, a weekend somewhere.
    Trip,
    /// A journey: a flight, a train, a long drive.
    Travel,
    /// None of the others.
    Other,
}

impl EventKind {
    /// Every kind, in the order the interface lists them.
    pub const ALL: [Self; 12] = [
        Self::Meal,
        Self::Celebration,
        Self::Outing,
        Self::Culture,
        Self::Sport,
        Self::Trip,
        Self::Travel,
        Self::Work,
        Self::Study,
        Self::Appointment,
        Self::Health,
        Self::Other,
    ];

    /// Its name in files: `meal`, `celebration`…
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Meal => "meal",
            Self::Celebration => "celebration",
            Self::Appointment => "appointment",
            Self::Health => "health",
            Self::Work => "work",
            Self::Study => "study",
            Self::Sport => "sport",
            Self::Culture => "culture",
            Self::Outing => "outing",
            Self::Trip => "trip",
            Self::Travel => "travel",
            Self::Other => "other",
        }
    }

    /// Reads a kind by its name in files.
    #[must_use]
    pub fn parse(text: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|k| k.as_str() == text)
    }

    /// The names of every kind, comma-separated: for messages.
    #[must_use]
    pub fn names() -> String {
        Self::ALL
            .iter()
            .map(|k| k.as_str())
            .collect::<Vec<_>>()
            .join(", ")
    }
}

impl fmt::Display for EventKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// The keys of an event.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EventFields {
    /// What it is: `Dîner chez Julie`.
    pub title: String,
    /// When it starts. A day alone means an all-day event.
    pub start: EventTime,
    /// When it ends, exclusive. Absent for a point in time ("film seen").
    pub end: Option<EventTime>,
    /// Where it happens: a place file, or the address as the calendar wrote it.
    pub place: Option<LinkOrText>,
    /// Who takes part, without the vault's owner, who is implied: a person file, or
    /// the name (else the email) as the calendar wrote it.
    pub participants: Vec<LinkOrText>,
    /// Where it stands; absent means planned for a future event.
    pub status: Option<EventStatus>,
    /// Who sent the invitation, when it is not the vault's owner: a person file, or
    /// the name (else the email) as the calendar wrote it. The title, place and notes
    /// were written by them (decision 0014).
    pub organizer: Option<LinkOrText>,
    /// What kind of event it is; absent until someone says.
    pub kind: Option<EventKind>,
}

impl EventFields {
    /// An event with a title and a start, and nothing else.
    #[must_use]
    pub fn new(title: impl Into<String>, start: EventTime) -> Self {
        Self {
            title: title.into(),
            start,
            end: None,
            place: None,
            participants: Vec::new(),
            status: None,
            organizer: None,
            kind: None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn times_are_days_or_moments() {
        let day = EventTime::parse("2026-11-14").unwrap();
        assert!(day.is_day());
        assert_eq!(day.to_string(), "2026-11-14");
        let at = EventTime::parse("2026-11-14T23:30:00-05:00").unwrap();
        assert_eq!(at.to_string(), "2026-11-14T23:30:00-05:00");
        assert_eq!(
            at.date().to_string(),
            "2026-11-14",
            "the day in its own offset, not in UTC"
        );
        for bad in [
            "2026-11-14T20:00:00",
            "20261114",
            "2026-1-14",
            "2026-02-30",
            "tomorrow",
            "",
        ] {
            assert!(EventTime::parse(bad).is_none(), "{bad:?}");
        }
    }

    #[test]
    fn statuses() {
        for s in [
            EventStatus::Planned,
            EventStatus::Done,
            EventStatus::Cancelled,
        ] {
            assert_eq!(EventStatus::parse(s.as_str()), Some(s));
        }
        assert_eq!(EventStatus::parse("Cancelled"), None);
    }
}
