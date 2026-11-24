//! Reminders (issue #206): files of type `reminder`, with `at`, an optional `every` and
//! `done`. Ipse does not ring: the person's AI reads what is due when it calls `now`,
//! at the start of a conversation or from its own app's scheduled run.

use jiff::civil::Date;
use jiff::tz::TimeZone;
use jiff::{SignedDuration, Span, Timestamp, Zoned};

use crate::vault::{DateTime, EntityFields, EventTime, Every};

/// How far ahead `now` tells of reminders: those of the next 24 hours.
pub const AHEAD: SignedDuration = SignedDuration::from_hours(24);

/// A reminder's occurrence to tell the person about.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Due {
    /// When: a day, or a moment in the vault's time zone.
    pub at: EventTime,
    /// True when that moment has passed.
    pub overdue: bool,
}

/// The occurrence of the reminder `fields` to tell about at `now`: the last one that
/// began, unless it is done; otherwise the next one, when it begins before `now + ahead`.
/// A reminder without `at` is never due. One that comes back (`every`) is done for an
/// occurrence when `done` is that day or later; one that does not, once `done` is set.
#[must_use]
pub fn due(
    fields: &EntityFields,
    now: Timestamp,
    tz: &TimeZone,
    ahead: SignedDuration,
) -> Option<Due> {
    let at = fields.at?;
    let horizon = now.checked_add(ahead).ok()?;
    let begins = |k: i64| {
        let o = occurrence(&at, fields.every, k, tz)?;
        Some((instant(&o, tz)?, o))
    };
    let done = |o: &EventTime| match (fields.every, fields.done) {
        (_, None) => false,
        (None, Some(_)) => true,
        (Some(_), Some(d)) => d >= o.date(),
    };
    let (first, _) = begins(0)?;
    if first > now {
        let (_, o) = begins(0).filter(|(i, _)| *i <= horizon)?;
        return (!done(&o)).then_some(Due {
            at: o,
            overdue: false,
        });
    }
    // The last occurrence that began: a first guess from the days between, then a few
    // steps either way.
    let mut k = 0;
    if let Some(every) = fields.every {
        let days = (now.to_zoned(tz.clone()).date() - at.date()).get_days();
        let per = match every {
            Every::Day => 1,
            Every::Week => 7,
            Every::Month => 31,
            Every::Year => 366,
        };
        k = i64::from(days.max(0)) / per;
        while k > 0 && begins(k).is_none_or(|(i, _)| i > now) {
            k -= 1;
        }
        while begins(k + 1).is_some_and(|(i, _)| i <= now) {
            k += 1;
        }
    }
    let (_, last) = begins(k)?;
    if !done(&last) {
        return Some(Due {
            at: last,
            overdue: true,
        });
    }
    fields.every?;
    let (next_at, next) = begins(k + 1)?;
    (next_at <= horizon && !done(&next)).then_some(Due {
        at: next,
        overdue: false,
    })
}

/// The `k`-th occurrence of `at`, counted from 0, `every` apart; `at` itself without
/// `every`. A day of month missing in a shorter month falls on its last day.
fn occurrence(at: &EventTime, every: Option<Every>, k: i64, tz: &TimeZone) -> Option<EventTime> {
    let Some(every) = every else {
        return Some(*at);
    };
    let span = match every {
        Every::Day => Span::new().try_days(k).ok()?,
        Every::Week => Span::new().try_weeks(k).ok()?,
        Every::Month => Span::new().try_months(k).ok()?,
        Every::Year => Span::new().try_years(k).ok()?,
    };
    match at {
        EventTime::Day(day) => day.checked_add(span).ok().map(EventTime::Day),
        EventTime::At(moment) => {
            // The same hour of the day, whatever the offset that day.
            let zoned: Zoned = moment.timestamp().to_zoned(tz.clone());
            let next = zoned.checked_add(span).ok()?;
            Some(EventTime::At(DateTime::at(next.timestamp(), tz)))
        }
    }
}

/// The moment an occurrence begins: a day at midnight in the vault's time zone.
fn instant(time: &EventTime, tz: &TimeZone) -> Option<Timestamp> {
    match time {
        EventTime::At(at) => Some(at.timestamp()),
        EventTime::Day(day) => day_start(*day, tz),
    }
}

fn day_start(day: Date, tz: &TimeZone) -> Option<Timestamp> {
    day.to_zoned(tz.clone()).ok().map(|z| z.timestamp())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn paris() -> TimeZone {
        TimeZone::get("Europe/Paris").unwrap()
    }

    fn at(text: &str) -> Timestamp {
        DateTime::parse(text).unwrap().timestamp()
    }

    fn reminder(at: &str, every: Option<Every>, done: Option<&str>) -> EntityFields {
        EntityFields {
            at: EventTime::parse(at),
            every,
            done: done.map(|d| d.parse().unwrap()),
            ..EntityFields::default()
        }
    }

    fn when(fields: &EntityFields, now: &str) -> Option<(String, bool)> {
        due(fields, at(now), &paris(), AHEAD).map(|d| (d.at.to_string(), d.overdue))
    }

    #[test]
    fn once_due_ahead_then_overdue_then_done() {
        let thursday = reminder("2026-11-26T18:00:00+01:00", None, None);
        assert_eq!(
            when(&thursday, "2026-11-24T18:00:00+01:00"),
            None,
            "two days ahead"
        );
        assert_eq!(
            when(&thursday, "2026-11-25T19:00:00+01:00"),
            Some(("2026-11-26T18:00:00+01:00".into(), false))
        );
        assert_eq!(
            when(&thursday, "2026-11-28T09:00:00+01:00"),
            Some(("2026-11-26T18:00:00+01:00".into(), true)),
            "still told until done"
        );
        let done = reminder("2026-11-26T18:00:00+01:00", None, Some("2026-11-26"));
        assert_eq!(when(&done, "2026-11-28T09:00:00+01:00"), None);
        assert_eq!(
            when(&EntityFields::default(), "2026-11-28T09:00:00+01:00"),
            None
        );
    }

    #[test]
    fn every_day_comes_back_the_next_day() {
        let sport = |done| reminder("2026-11-01", Some(Every::Day), done);
        // Today's, until it is done; then tomorrow's, within 24 hours.
        assert_eq!(
            when(&sport(None), "2026-11-24T08:00:00+01:00"),
            Some(("2026-11-24".into(), true))
        );
        assert_eq!(
            when(&sport(Some("2026-11-24")), "2026-11-24T08:00:00+01:00"),
            Some(("2026-11-25".into(), false))
        );
        assert_eq!(
            when(&sport(Some("2026-11-24")), "2026-11-24T23:30:00+01:00"),
            Some(("2026-11-25".into(), false))
        );
    }

    #[test]
    fn every_week_month_year_keep_their_day_and_hour() {
        let monday = reminder(
            "2026-03-02T09:00:00+01:00",
            Some(Every::Week),
            Some("2026-06-08"),
        );
        // Summer time: still 9 o'clock, now at +02:00.
        assert_eq!(
            when(&monday, "2026-06-15T08:00:00+02:00"),
            Some(("2026-06-15T09:00:00+02:00".into(), false))
        );
        let rent = reminder("2026-01-31", Some(Every::Month), None);
        assert_eq!(
            when(&rent, "2026-02-28T10:00:00+01:00"),
            Some(("2026-02-28".into(), true)),
            "the last day of a shorter month"
        );
        let birthday = reminder("2020-11-24", Some(Every::Year), Some("2025-11-24"));
        assert_eq!(
            when(&birthday, "2026-11-24T07:00:00+01:00"),
            Some(("2026-11-24".into(), true))
        );
        assert_eq!(when(&birthday, "2026-06-01T07:00:00+02:00"), None);
    }
}
