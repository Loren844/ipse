//! Recurrence rules (`RRULE`, RFC 5545 section 3.3.10), expanded into the days an event
//! repeats on.
//!
//! Supported: `FREQ` daily to yearly, `INTERVAL`, `COUNT`, `UNTIL`, `BYDAY` (with
//! positions such as `2TU` or `-1FR`), `BYMONTHDAY`, `BYMONTH`, `BYSETPOS` and `WKST`.
//! The time of day always comes from the event's start, so expansion works on days.
//! Rules that repeat within a day (`HOURLY`, `BYHOUR`…) or by week number or day of the
//! year are rare in calendars and are reported as not supported.

use jiff::ToSpan;
use jiff::civil::{Date, Weekday};

/// How often a rule repeats.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Freq {
    Daily,
    Weekly,
    Monthly,
    Yearly,
}

/// The end of a rule, as written in `UNTIL`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Until {
    /// `UNTIL=20261231`.
    Date(Date),
    /// `UNTIL=20261231T225959Z` or a floating date-time, as written.
    DateTime(String),
}

/// A parsed `RRULE`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Rule {
    freq: Freq,
    interval: i64,
    pub(crate) count: Option<u32>,
    pub(crate) until: Option<Until>,
    by_day: Vec<(Option<i16>, Weekday)>,
    by_month_day: Vec<i8>,
    by_month: Vec<i8>,
    by_set_pos: Vec<i32>,
    week_start: Weekday,
}

/// A rule Ipse cannot expand.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Unsupported;

impl Rule {
    /// Reads an `RRULE` value like `FREQ=WEEKLY;BYDAY=MO,WE;UNTIL=20261231T000000Z`.
    ///
    /// # Errors
    ///
    /// [`Unsupported`] for a rule that is malformed or uses parts Ipse does not expand.
    pub(crate) fn parse(text: &str) -> Result<Self, Unsupported> {
        let mut rule = Self {
            freq: Freq::Daily,
            interval: 1,
            count: None,
            until: None,
            by_day: Vec::new(),
            by_month_day: Vec::new(),
            by_month: Vec::new(),
            by_set_pos: Vec::new(),
            week_start: Weekday::Monday,
        };
        let mut freq = None;
        for part in text.trim().split(';').filter(|p| !p.trim().is_empty()) {
            let (key, value) = part.split_once('=').ok_or(Unsupported)?;
            let value = value.trim();
            let list = || value.split(',').map(str::trim);
            match key.trim().to_ascii_uppercase().as_str() {
                "FREQ" => {
                    freq = Some(match value.to_ascii_uppercase().as_str() {
                        "DAILY" => Freq::Daily,
                        "WEEKLY" => Freq::Weekly,
                        "MONTHLY" => Freq::Monthly,
                        "YEARLY" => Freq::Yearly,
                        _ => return Err(Unsupported),
                    });
                }
                "INTERVAL" => {
                    rule.interval = value.parse().map_err(|_| Unsupported)?;
                    if !(1..=10_000).contains(&rule.interval) {
                        return Err(Unsupported);
                    }
                }
                "COUNT" => rule.count = Some(value.parse().map_err(|_| Unsupported)?),
                "UNTIL" => {
                    rule.until = Some(match super::time::parse_date(value) {
                        Some(date) => Until::Date(date),
                        None => Until::DateTime(value.to_owned()),
                    });
                }
                "BYDAY" => {
                    rule.by_day = list().map(by_day).collect::<Result<_, _>>()?;
                }
                "BYMONTHDAY" => {
                    rule.by_month_day = list()
                        .map(|d| d.parse::<i8>().ok().filter(|d| (1..=31).contains(&d.abs())))
                        .collect::<Option<_>>()
                        .ok_or(Unsupported)?;
                }
                "BYMONTH" => {
                    rule.by_month = list()
                        .map(|m| m.parse::<i8>().ok().filter(|m| (1..=12).contains(m)))
                        .collect::<Option<_>>()
                        .ok_or(Unsupported)?;
                }
                "BYSETPOS" => {
                    rule.by_set_pos = list()
                        .map(|p| p.parse::<i32>().ok().filter(|p| *p != 0))
                        .collect::<Option<_>>()
                        .ok_or(Unsupported)?;
                }
                "WKST" => rule.week_start = weekday(value).ok_or(Unsupported)?,
                // Within a day, or by week number or day of the year: not expanded.
                "BYHOUR" | "BYMINUTE" | "BYSECOND" | "BYWEEKNO" | "BYYEARDAY" => {
                    return Err(Unsupported);
                }
                _ => {}
            }
        }
        rule.freq = freq.ok_or(Unsupported)?;
        Ok(rule)
    }

    /// The days the event repeats on, from `start` (always the first) in order, up to
    /// and including `last`. `COUNT` is honoured; `UNTIL` is left to the caller, who
    /// knows the time of day and the time zone.
    pub(crate) fn days(&self, start: Date, last: Date) -> Days<'_> {
        Days {
            rule: self,
            start,
            last,
            period: 0,
            pending: Vec::new(),
            emitted: 0,
            started: false,
        }
    }

    /// The candidate days of the period beginning on `anchor`, sorted, with `BYSETPOS`
    /// applied. Days the rule does not name come from `start`.
    fn candidates(&self, anchor: Date, start: Date) -> Vec<Date> {
        let mut days = match self.freq {
            Freq::Daily => vec![anchor]
                .into_iter()
                .filter(|d| self.month_ok(*d) && self.month_day_ok(*d) && self.weekday_ok(*d))
                .collect(),
            Freq::Weekly => {
                let wanted: Vec<Weekday> = if self.by_day.is_empty() {
                    vec![start.weekday()]
                } else {
                    self.by_day.iter().map(|(_, w)| *w).collect()
                };
                (0..7)
                    .filter_map(|i| anchor.checked_add(i.days()).ok())
                    .filter(|d| wanted.contains(&d.weekday()) && self.month_ok(*d))
                    .collect()
            }
            Freq::Monthly => {
                if self.month_ok(anchor) {
                    self.in_month(anchor, start.day())
                } else {
                    Vec::new()
                }
            }
            Freq::Yearly => self.in_year(anchor.year(), start),
        };
        days.sort_unstable();
        days.dedup();
        if self.by_set_pos.is_empty() {
            return days;
        }
        let len = i32::try_from(days.len()).unwrap_or(i32::MAX);
        let mut picked: Vec<Date> = self
            .by_set_pos
            .iter()
            .filter_map(|&p| {
                let index = if p > 0 { p - 1 } else { len + p };
                usize::try_from(index)
                    .ok()
                    .and_then(|i| days.get(i).copied())
            })
            .collect();
        picked.sort_unstable();
        picked.dedup();
        picked
    }

    /// Days of the month of `first` (the 1st) matching the rule; `default_day` when the
    /// rule names no day.
    fn in_month(&self, first: Date, default_day: i8) -> Vec<Date> {
        let first = first.first_of_month();
        let len = first.days_in_month();
        if !self.by_month_day.is_empty() {
            return self
                .by_month_day
                .iter()
                .filter_map(|&d| {
                    let day = if d > 0 { d } else { len + d + 1 };
                    (1..=len)
                        .contains(&day)
                        .then(|| first.with().day(day).build().ok())?
                })
                .filter(|d| self.weekday_ok(*d))
                .collect();
        }
        if !self.by_day.is_empty() {
            let mut days = Vec::new();
            for &(n, weekday) in &self.by_day {
                match n {
                    Some(n) => {
                        let n = i8::try_from(n).unwrap_or(i8::MAX);
                        days.extend(first.nth_weekday_of_month(n, weekday).ok());
                    }
                    None => days.extend(
                        (0..len)
                            .filter_map(|i| first.checked_add(i64::from(i).days()).ok())
                            .filter(|d| d.weekday() == weekday),
                    ),
                }
            }
            return days;
        }
        first
            .with()
            .day(default_day)
            .build()
            .ok()
            .filter(|d| d.month() == first.month())
            .into_iter()
            .collect()
    }

    /// Days of `year` matching the rule.
    fn in_year(&self, year: i16, start: Date) -> Vec<Date> {
        let month = |m: i8| Date::new(year, m, 1).ok();
        if !self.by_month.is_empty() {
            return self
                .by_month
                .iter()
                .filter_map(|&m| month(m))
                .flat_map(|first| self.in_month(first, start.day()))
                .collect();
        }
        if !self.by_month_day.is_empty() {
            return (1..=12)
                .filter_map(month)
                .flat_map(|first| self.in_month(first, 1))
                .collect();
        }
        if !self.by_day.is_empty() {
            let mut days = Vec::new();
            let jan1 = Date::new(year, 1, 1).ok();
            for &(n, weekday) in &self.by_day {
                match (n, jan1) {
                    (Some(n), Some(jan1)) => {
                        let found = if n > 0 {
                            jan1.nth_weekday(i32::from(n), weekday)
                        } else {
                            jan1.with()
                                .month(12)
                                .day(31)
                                .build()
                                .and_then(|dec31| dec31.checked_add(1.day()))
                                .and_then(|next| next.nth_weekday(i32::from(n), weekday))
                        };
                        days.extend(found.ok().filter(|d| d.year() == year));
                    }
                    (None, Some(jan1)) => days.extend(
                        (0..366)
                            .filter_map(|i| jan1.checked_add(i64::from(i).days()).ok())
                            .filter(|d| d.year() == year && d.weekday() == weekday),
                    ),
                    _ => {}
                }
            }
            return days;
        }
        Date::new(year, start.month(), start.day())
            .ok()
            .into_iter()
            .collect()
    }

    fn month_ok(&self, d: Date) -> bool {
        self.by_month.is_empty() || self.by_month.contains(&d.month())
    }

    fn month_day_ok(&self, d: Date) -> bool {
        let len = d.days_in_month();
        self.by_month_day.is_empty()
            || self
                .by_month_day
                .iter()
                .any(|&m| m == d.day() || len + m + 1 == d.day())
    }

    fn weekday_ok(&self, d: Date) -> bool {
        self.by_day.is_empty() || self.by_day.iter().any(|(_, w)| *w == d.weekday())
    }
}

/// The days of a rule, in order. See [`Rule::days`].
pub(crate) struct Days<'r> {
    rule: &'r Rule,
    start: Date,
    last: Date,
    period: i64,
    /// Days of the current period still to return, in reverse order.
    pending: Vec<Date>,
    emitted: u32,
    started: bool,
}

impl Days<'_> {
    /// The first day of period number `n`, `None` past the last day.
    fn anchor(&self, n: i64) -> Option<Date> {
        let rule = self.rule;
        let step = n.checked_mul(rule.interval)?;
        let date = match rule.freq {
            Freq::Daily => self.start.checked_add(step.days()).ok()?,
            Freq::Weekly => {
                let back = (self.start.weekday().to_monday_zero_offset()
                    - rule.week_start.to_monday_zero_offset())
                .rem_euclid(7);
                let week = self.start.checked_sub(i64::from(back).days()).ok()?;
                week.checked_add(step.weeks()).ok()?
            }
            Freq::Monthly => {
                let first = self.start.first_of_month();
                first.checked_add(step.months()).ok()?
            }
            Freq::Yearly => {
                let year = i64::from(self.start.year()).checked_add(step)?;
                Date::new(i16::try_from(year).ok()?, 1, 1).ok()?
            }
        };
        (date <= self.last).then_some(date)
    }

    fn done(&self) -> bool {
        self.rule.count.is_some_and(|c| self.emitted >= c)
    }
}

impl Iterator for Days<'_> {
    type Item = Date;

    fn next(&mut self) -> Option<Date> {
        if self.done() {
            return None;
        }
        if !self.started {
            self.started = true;
            if self.start > self.last {
                return None;
            }
            self.emitted += 1;
            return Some(self.start);
        }
        loop {
            if let Some(day) = self.pending.pop() {
                if day > self.last {
                    self.pending.clear();
                    return None;
                }
                self.emitted += 1;
                return Some(day);
            }
            let anchor = self.anchor(self.period)?;
            self.period += 1;
            let mut days = self.rule.candidates(anchor, self.start);
            days.retain(|d| *d > self.start);
            days.reverse();
            self.pending = days;
        }
    }
}

/// `MO`, `2TU`, `-1FR`.
fn by_day(text: &str) -> Result<(Option<i16>, Weekday), Unsupported> {
    let split = text.len().checked_sub(2).ok_or(Unsupported)?;
    let (n, day) = text.split_at(split);
    let day = weekday(day).ok_or(Unsupported)?;
    let n = match n.trim_start_matches('+') {
        "" => None,
        n => Some(
            n.parse::<i16>()
                .ok()
                .filter(|n| *n != 0 && (-53..=53).contains(n))
                .ok_or(Unsupported)?,
        ),
    };
    Ok((n, day))
}

fn weekday(text: &str) -> Option<Weekday> {
    Some(match text.trim().to_ascii_uppercase().as_str() {
        "MO" => Weekday::Monday,
        "TU" => Weekday::Tuesday,
        "WE" => Weekday::Wednesday,
        "TH" => Weekday::Thursday,
        "FR" => Weekday::Friday,
        "SA" => Weekday::Saturday,
        "SU" => Weekday::Sunday,
        _ => return None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn d(text: &str) -> Date {
        text.parse().unwrap()
    }

    fn days(rule: &str, start: &str, last: &str) -> Vec<String> {
        Rule::parse(rule)
            .unwrap()
            .days(d(start), d(last))
            .map(|d| d.to_string())
            .collect()
    }

    #[test]
    fn daily_and_weekly() {
        assert_eq!(
            days("FREQ=DAILY;COUNT=3", "2026-11-30", "2027-12-31"),
            ["2026-11-30", "2026-12-01", "2026-12-02"]
        );
        assert_eq!(
            days("FREQ=DAILY;INTERVAL=10", "2026-01-01", "2026-01-31"),
            ["2026-01-01", "2026-01-11", "2026-01-21", "2026-01-31"]
        );
        assert_eq!(
            days("FREQ=WEEKLY;BYDAY=MO,WE", "2026-11-04", "2026-11-17"),
            ["2026-11-04", "2026-11-09", "2026-11-11", "2026-11-16"]
        );
        assert_eq!(
            days("FREQ=WEEKLY;INTERVAL=2;COUNT=3", "2026-11-04", "2027-12-31"),
            ["2026-11-04", "2026-11-18", "2026-12-02"]
        );
    }

    #[test]
    fn week_start_changes_which_weeks_count() {
        // RFC 5545, section 3.3.10: the same rule with WKST=MO and WKST=SU.
        assert_eq!(
            days(
                "FREQ=WEEKLY;INTERVAL=2;COUNT=4;BYDAY=TU,SU;WKST=MO",
                "1997-08-05",
                "1998-01-01"
            ),
            ["1997-08-05", "1997-08-10", "1997-08-19", "1997-08-24"]
        );
        assert_eq!(
            days(
                "FREQ=WEEKLY;INTERVAL=2;COUNT=4;BYDAY=TU,SU;WKST=SU",
                "1997-08-05",
                "1998-01-01"
            ),
            ["1997-08-05", "1997-08-17", "1997-08-19", "1997-08-31"]
        );
    }

    #[test]
    fn monthly() {
        assert_eq!(
            days("FREQ=MONTHLY;COUNT=4", "2026-01-31", "2027-12-31"),
            ["2026-01-31", "2026-03-31", "2026-05-31", "2026-07-31"],
            "months without a 31st are skipped"
        );
        assert_eq!(
            days(
                "FREQ=MONTHLY;BYDAY=-1FR;COUNT=3",
                "2026-01-30",
                "2027-12-31"
            ),
            ["2026-01-30", "2026-02-27", "2026-03-27"]
        );
        assert_eq!(
            days("FREQ=MONTHLY;BYDAY=2TU", "2026-11-10", "2027-02-28"),
            ["2026-11-10", "2026-12-08", "2027-01-12", "2027-02-09"]
        );
        assert_eq!(
            days("FREQ=MONTHLY;BYMONTHDAY=-1", "2026-01-31", "2026-04-30"),
            ["2026-01-31", "2026-02-28", "2026-03-31", "2026-04-30"]
        );
        // The last working day of the month.
        assert_eq!(
            days(
                "FREQ=MONTHLY;BYDAY=MO,TU,WE,TH,FR;BYSETPOS=-1",
                "2026-01-30",
                "2026-05-31"
            ),
            [
                "2026-01-30",
                "2026-02-27",
                "2026-03-31",
                "2026-04-30",
                "2026-05-29"
            ]
        );
    }

    #[test]
    fn yearly() {
        assert_eq!(
            days("FREQ=YEARLY", "2024-02-29", "2032-12-31"),
            ["2024-02-29", "2028-02-29", "2032-02-29"],
            "a 29 February birthday only in leap years"
        );
        assert_eq!(
            days(
                "FREQ=YEARLY;BYMONTH=11;BYDAY=4TH",
                "2026-11-26",
                "2028-12-31"
            ),
            ["2026-11-26", "2027-11-25", "2028-11-23"],
            "Thanksgiving"
        );
        assert_eq!(
            days("FREQ=YEARLY;BYDAY=1MO", "2026-01-05", "2028-12-31"),
            ["2026-01-05", "2027-01-04", "2028-01-03"]
        );
        assert_eq!(
            days("FREQ=YEARLY;BYDAY=-1MO", "2026-12-28", "2027-12-31"),
            ["2026-12-28", "2027-12-27"]
        );
    }

    #[test]
    fn the_start_is_always_first_and_counted() {
        assert_eq!(
            days("FREQ=WEEKLY;BYDAY=FR;COUNT=2", "2026-11-04", "2027-12-31"),
            ["2026-11-04", "2026-11-06"]
        );
        assert!(days("FREQ=DAILY", "2026-11-04", "2026-11-03").is_empty());
    }

    #[test]
    fn rules_that_never_match_end() {
        assert_eq!(
            days(
                "FREQ=YEARLY;BYMONTH=2;BYMONTHDAY=30",
                "2026-01-01",
                "2030-12-31"
            ),
            ["2026-01-01"]
        );
    }

    #[test]
    fn unsupported_and_malformed_rules() {
        for rule in [
            "FREQ=HOURLY",
            "FREQ=DAILY;BYHOUR=9,17",
            "FREQ=YEARLY;BYWEEKNO=20",
            "FREQ=YEARLY;BYYEARDAY=100",
            "BYDAY=MO",
            "FREQ=WEEKLY;BYDAY=XX",
            "FREQ=DAILY;INTERVAL=0",
            "FREQ=MONTHLY;BYMONTHDAY=32",
            "nonsense",
        ] {
            assert_eq!(Rule::parse(rule), Err(Unsupported), "{rule}");
        }
        let rule = Rule::parse("FREQ=DAILY;UNTIL=20261231T225959Z;X-NAME=1").unwrap();
        assert_eq!(rule.until, Some(Until::DateTime("20261231T225959Z".into())));
    }
}
