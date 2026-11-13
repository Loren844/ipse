//! Dates, times and time zones of iCalendar values.
//!
//! A `TZID` is resolved, in order: as an IANA name (`Europe/Paris`), as the end of a
//! longer name (`/mozilla.org/20050126_1/Europe/Paris`), as a common Windows name
//! (`Romance Standard Time`), then from the file's own `VTIMEZONE`, turned into a POSIX
//! rule. IANA names come first because they know past rule changes; a `VTIMEZONE` only
//! gives the rules of today.

use std::collections::HashMap;

use jiff::civil::{Date, DateTime, Time};
use jiff::tz::TimeZone;
use jiff::{Timestamp, Zoned};

use super::ics::{Definition, Observance, Stamp};

/// When an event starts or ends.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum When {
    /// A whole day.
    Day(Date),
    /// A moment, in the event's time zone.
    At(Zoned),
}

impl When {
    /// `2026-11-14`, or `2026-11-14T20:00:00+01:00`.
    pub(crate) fn text(&self) -> String {
        match self {
            Self::Day(date) => date.to_string(),
            Self::At(zoned) => zoned.strftime("%Y-%m-%dT%H:%M:%S%:z").to_string(),
        }
    }

    /// What identifies this start among the occurrences of a repeating event.
    pub(crate) fn key(&self) -> Key {
        match self {
            Self::Day(date) => Key::Day(*date),
            Self::At(zoned) => Key::At(zoned.timestamp()),
        }
    }
}

/// An occurrence's identity: its original start, as a day or an instant. The same
/// whatever time zone it was written in, so that two calendars agree on it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub(crate) enum Key {
    Day(Date),
    At(Timestamp),
}

impl Key {
    /// `2026-11-14`, or `2026-11-14T19:00:00Z`.
    pub(crate) fn text(&self) -> String {
        match self {
            Self::Day(date) => date.to_string(),
            Self::At(ts) => ts.strftime("%Y-%m-%dT%H:%M:%SZ").to_string(),
        }
    }

    /// The day, for a day; the UTC day otherwise.
    pub(crate) fn date(&self) -> Date {
        match self {
            Self::Day(date) => *date,
            Self::At(ts) => ts.to_zoned(TimeZone::UTC).date(),
        }
    }
}

/// The time zones of one file.
pub(crate) struct Zones {
    /// Resolved `TZID`s; `None` for those that could not be.
    known: HashMap<String, Option<TimeZone>>,
    /// The file's `VTIMEZONE`s, by `TZID`.
    defined: HashMap<String, TimeZone>,
    /// The vault's time zone, for floating times and unknown `TZID`s.
    fallback: TimeZone,
}

impl Zones {
    pub(crate) fn new(fallback: TimeZone) -> Self {
        Self {
            known: HashMap::new(),
            defined: HashMap::new(),
            fallback,
        }
    }

    /// The vault's time zone.
    pub(crate) fn fallback(&self) -> &TimeZone {
        &self.fallback
    }

    /// Records a `VTIMEZONE` of the file.
    pub(crate) fn define(&mut self, definition: &Definition) {
        if let Some(tz) = posix(&definition.observances) {
            self.defined.insert(definition.tzid.clone(), tz);
            // A name that failed before may resolve now.
            self.known.retain(|_, tz| tz.is_some());
        }
    }

    /// True when `tzid` is absent or can be resolved.
    pub(crate) fn knows(&mut self, tzid: Option<&str>) -> bool {
        tzid.is_none_or(|t| self.zone(t).is_some())
    }

    /// The time zone named `tzid`, if it can be resolved.
    pub(crate) fn zone(&mut self, tzid: &str) -> Option<TimeZone> {
        if let Some(tz) = self.known.get(tzid) {
            return tz.clone();
        }
        let tz = iana(tzid).or_else(|| self.defined.get(tzid).cloned());
        self.known.insert(tzid.to_owned(), tz.clone());
        tz
    }

    /// The time zone of a value: its `TZID`, else the vault's.
    pub(crate) fn zone_of(&mut self, stamp: &Stamp) -> TimeZone {
        stamp
            .tzid
            .as_deref()
            .and_then(|t| self.zone(t))
            .unwrap_or_else(|| self.fallback.clone())
    }

    /// A value as a day or a moment. A UTC time is shown in the vault's time zone; a
    /// floating time or an unknown `TZID` is read in the vault's time zone.
    pub(crate) fn when(&mut self, stamp: &Stamp) -> Option<When> {
        let value = stamp.value.trim();
        if let Some(date) = parse_date(value) {
            return Some(When::Day(date));
        }
        let (datetime, utc) = parse_datetime(value)?;
        if utc {
            let ts = datetime.to_zoned(TimeZone::UTC).ok()?.timestamp();
            return Some(When::At(ts.to_zoned(self.fallback.clone())));
        }
        datetime.to_zoned(self.zone_of(stamp)).ok().map(When::At)
    }
}

/// `20261114`.
pub(crate) fn parse_date(value: &str) -> Option<Date> {
    if value.len() != 8 || !value.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    Date::new(
        value[..4].parse().ok()?,
        value[4..6].parse().ok()?,
        value[6..].parse().ok()?,
    )
    .ok()
}

/// `20261114T200000`, with `true` when it ends with `Z` (UTC).
pub(crate) fn parse_datetime(value: &str) -> Option<(DateTime, bool)> {
    let (value, utc) = match value.strip_suffix(['Z', 'z']) {
        Some(v) => (v, true),
        None => (value, false),
    };
    let (date, time) = value.split_once(['T', 't'])?;
    let date = parse_date(date)?;
    if time.len() != 6 || !time.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    let time = Time::new(
        time[..2].parse().ok()?,
        time[2..4].parse().ok()?,
        // A leap second becomes the last second of the minute.
        time[4..].parse::<i8>().ok()?.min(59),
        0,
    )
    .ok()?;
    Some((date.to_datetime(time), utc))
}

/// An IANA time zone from a `TZID`, trying the end of longer names and common Windows
/// names.
fn iana(tzid: &str) -> Option<TimeZone> {
    let tzid = tzid.trim();
    if let Ok(tz) = TimeZone::get(tzid) {
        return Some(tz);
    }
    let suffixes = tzid
        .match_indices('/')
        .map(|(i, _)| &tzid[i + 1..])
        .filter(|s| s.contains('/'));
    for suffix in suffixes {
        if let Ok(tz) = TimeZone::get(suffix) {
            return Some(tz);
        }
    }
    WINDOWS
        .iter()
        .find(|(windows, _)| windows.eq_ignore_ascii_case(tzid))
        .and_then(|(_, name)| TimeZone::get(name).ok())
}

/// The Windows time zones Outlook writes most, and their IANA names (CLDR).
const WINDOWS: [(&str, &str); 24] = [
    ("Romance Standard Time", "Europe/Paris"),
    ("W. Europe Standard Time", "Europe/Berlin"),
    ("Central Europe Standard Time", "Europe/Budapest"),
    ("Central European Standard Time", "Europe/Warsaw"),
    ("GMT Standard Time", "Europe/London"),
    ("Greenwich Standard Time", "Atlantic/Reykjavik"),
    ("E. Europe Standard Time", "Europe/Chisinau"),
    ("FLE Standard Time", "Europe/Kiev"),
    ("GTB Standard Time", "Europe/Bucharest"),
    ("Russian Standard Time", "Europe/Moscow"),
    ("Morocco Standard Time", "Africa/Casablanca"),
    ("UTC", "UTC"),
    ("Coordinated Universal Time", "UTC"),
    ("Eastern Standard Time", "America/New_York"),
    ("Central Standard Time", "America/Chicago"),
    ("Mountain Standard Time", "America/Denver"),
    ("Pacific Standard Time", "America/Los_Angeles"),
    ("Atlantic Standard Time", "America/Halifax"),
    ("E. South America Standard Time", "America/Sao_Paulo"),
    ("India Standard Time", "Asia/Kolkata"),
    ("China Standard Time", "Asia/Shanghai"),
    ("Tokyo Standard Time", "Asia/Tokyo"),
    ("AUS Eastern Standard Time", "Australia/Sydney"),
    ("Arabian Standard Time", "Asia/Dubai"),
];

/// A `VTIMEZONE`'s current rules as a POSIX time zone: the latest standard and
/// daylight parts, each changing on the n-th (or last) weekday of a month.
fn posix(observances: &[Observance]) -> Option<TimeZone> {
    let latest = |daylight: bool| {
        observances
            .iter()
            .filter(|o| o.daylight == daylight)
            .max_by(|a, b| a.start.cmp(&b.start))
    };
    let standard = latest(false)?;
    let std_offset = posix_offset(&standard.offset_to)?;
    let rules = latest(true).and_then(|daylight| {
        Some((
            posix_offset(&daylight.offset_to)?,
            posix_change(daylight)?,
            posix_change(standard)?,
        ))
    });
    let text = match rules {
        Some((dst_offset, to_dst, to_std)) => {
            format!("STD{std_offset}DST{dst_offset},{to_dst},{to_std}")
        }
        None => format!("STD{std_offset}"),
    };
    TimeZone::posix(&text).ok()
}

/// `+0130` (east of UTC) as POSIX `-1:30` (POSIX counts west of UTC).
fn posix_offset(offset: &str) -> Option<String> {
    let offset = offset.trim();
    let (sign, digits) = match offset.as_bytes().first()? {
        b'+' => ("-", &offset[1..]),
        b'-' => ("", &offset[1..]),
        _ => return None,
    };
    if !(digits.len() == 4 || digits.len() == 6) || !digits.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    let hours: u8 = digits[..2].parse().ok()?;
    let minutes = &digits[2..4];
    let seconds = digits.get(4..6).unwrap_or("00");
    Some(format!("{sign}{hours}:{minutes}:{seconds}"))
}

/// `FREQ=YEARLY;BYMONTH=3;BYDAY=-1SU` starting at `T020000` as POSIX `M3.5.0/2:00:00`.
fn posix_change(o: &Observance) -> Option<String> {
    let rule = o.rule.as_deref()?;
    let mut month = None;
    let mut week = None;
    let mut weekday = None;
    let mut month_day = None;
    for part in rule.split(';') {
        let (key, value) = part.split_once('=')?;
        match key.trim().to_ascii_uppercase().as_str() {
            "FREQ" if !value.eq_ignore_ascii_case("YEARLY") => return None,
            "BYMONTH" => month = value.trim().parse::<u8>().ok(),
            "BYDAY" => {
                let value = value.trim();
                let split = value.len().checked_sub(2)?;
                let (n, day) = value.split_at(split);
                weekday = DAYS.iter().position(|d| d.eq_ignore_ascii_case(day));
                week = match n {
                    "" => None,
                    "-1" => Some(5),
                    n => n
                        .trim_start_matches('+')
                        .parse::<u8>()
                        .ok()
                        .filter(|n| (1..=4).contains(n)),
                };
            }
            // `BYMONTHDAY=8,9,10,11,12,13,14;BYDAY=SU`: the second Sunday.
            "BYMONTHDAY" => {
                month_day = value
                    .split(',')
                    .next()
                    .and_then(|d| d.trim().parse::<u8>().ok());
            }
            _ => {}
        }
    }
    let week = week.or_else(|| month_day.map(|d| (d.saturating_sub(1) / 7 + 1).min(5)))?;
    let (_, time) = o.start.trim().split_once(['T', 't'])?;
    let time = time.get(..6)?;
    let (h, m, s) = (&time[..2], &time[2..4], &time[4..6]);
    let hours: u8 = h.parse().ok()?;
    Some(format!("M{}.{week}.{}/{hours}:{m}:{s}", month?, weekday?))
}

/// iCalendar weekday names, Sunday first as in POSIX.
const DAYS: [&str; 7] = ["SU", "MO", "TU", "WE", "TH", "FR", "SA"];

#[cfg(test)]
mod tests {
    use super::*;

    fn stamp(value: &str, tzid: Option<&str>) -> Stamp {
        Stamp {
            value: value.into(),
            tzid: tzid.map(Into::into),
        }
    }

    fn zones() -> Zones {
        Zones::new(TimeZone::get("Europe/Paris").unwrap())
    }

    #[test]
    fn values_as_days_and_moments() {
        let mut z = zones();
        let text = |z: &mut Zones, v, tz| z.when(&stamp(v, tz)).unwrap().text();
        assert_eq!(text(&mut z, "20261114", None), "2026-11-14");
        assert_eq!(
            text(&mut z, "20261114T200000", Some("America/New_York")),
            "2026-11-14T20:00:00-05:00"
        );
        assert_eq!(
            text(&mut z, "20261114T190000Z", None),
            "2026-11-14T20:00:00+01:00",
            "UTC shown in the vault's time zone"
        );
        assert_eq!(
            text(&mut z, "20260714T200000", None),
            "2026-07-14T20:00:00+02:00",
            "floating time in the vault's time zone"
        );
        assert!(z.when(&stamp("2026-11-14", None)).is_none());
        assert!(z.when(&stamp("20261340", None)).is_none());
    }

    #[test]
    fn tzids_resolve_by_name_suffix_or_windows_name() {
        let mut z = zones();
        for tzid in [
            "Europe/Paris",
            "/mozilla.org/20050126_1/Europe/Paris",
            "/freeassociation.sourceforge.net/Europe/Paris",
            "Romance Standard Time",
        ] {
            let tz = z.zone(tzid).unwrap();
            assert_eq!(tz.iana_name(), Some("Europe/Paris"), "{tzid}");
        }
        assert!(z.zone("Customized Time Zone").is_none());
        assert!(!z.knows(Some("Customized Time Zone")));
        assert!(z.knows(None));
    }

    #[test]
    fn a_file_s_own_definition_is_used_when_the_name_is_unknown() {
        let mut z = zones();
        assert!(!z.knows(Some("Customized Time Zone")));
        let observance = |daylight, start: &str, from: &str, to: &str, month| Observance {
            daylight,
            start: start.into(),
            offset_from: from.into(),
            offset_to: to.into(),
            rule: Some(format!("FREQ=YEARLY;BYDAY=-1SU;BYMONTH={month}")),
        };
        z.define(&Definition {
            tzid: "Customized Time Zone".into(),
            observances: vec![
                observance(false, "16010101T030000", "+0200", "+0100", 10),
                observance(true, "16010101T020000", "+0100", "+0200", 3),
            ],
        });
        assert!(z.knows(Some("Customized Time Zone")));
        let mut at = |v| {
            z.when(&stamp(v, Some("Customized Time Zone")))
                .unwrap()
                .text()
        };
        assert_eq!(at("20260328T120000"), "2026-03-28T12:00:00+01:00");
        assert_eq!(at("20260329T120000"), "2026-03-29T12:00:00+02:00");
        assert_eq!(at("20261024T120000"), "2026-10-24T12:00:00+02:00");
        assert_eq!(at("20261025T120000"), "2026-10-25T12:00:00+01:00");
    }

    #[test]
    fn posix_rules() {
        assert_eq!(posix_offset("+0100").as_deref(), Some("-1:00:00"));
        assert_eq!(posix_offset("-0330").as_deref(), Some("3:30:00"));
        assert_eq!(posix_offset("+053000").as_deref(), Some("-5:30:00"));
        assert_eq!(posix_offset("0100"), None);
        let o = Observance {
            daylight: true,
            start: "19700308T020000".into(),
            rule: Some("FREQ=YEARLY;BYMONTH=3;BYMONTHDAY=8,9,10,11,12,13,14;BYDAY=SU".into()),
            ..Observance::default()
        };
        assert_eq!(posix_change(&o).as_deref(), Some("M3.2.0/2:00:00"));
        let fixed = posix(&[Observance {
            start: "19700101T000000".into(),
            offset_to: "+0530".into(),
            ..Observance::default()
        }])
        .unwrap();
        let noon = Date::new(2026, 6, 1).unwrap().to_datetime(Time::midnight());
        assert_eq!(
            noon.to_zoned(fixed).unwrap().offset().seconds(),
            5 * 3600 + 1800
        );
    }

    #[test]
    fn occurrence_keys_ignore_how_they_were_written() {
        let mut z = zones();
        let a = z
            .when(&stamp("20261114T200000", Some("Europe/Paris")))
            .unwrap();
        let b = z.when(&stamp("20261114T190000Z", None)).unwrap();
        assert_eq!(a.key(), b.key());
        assert_eq!(a.key().text(), "2026-11-14T19:00:00Z");
        assert_eq!(
            When::Day(Date::new(2026, 11, 14).unwrap()).key().text(),
            "2026-11-14"
        );
    }
}
