//! State logs: what is true only while it is recent (`docs/spec/vault.md`, section 5).
//!
//! A state log is one file per topic and year, `states/<topic>/<YYYY>.md`, whose body is
//! a table of subjects with when they were first and last seen. Whether a state is
//! current is computed when it is read, never written.

use std::fmt::{self, Write as _};

use jiff::civil::Date;
use jiff::{SignedDuration, Zoned};

use super::fields::{DateTime, WikiLink};

/// A state topic: lowercase letters, digits and inner dashes, like `music` or `home`.
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct Topic(String);

impl Topic {
    /// Checks the format: `[a-z0-9-]+`, without a dash at either end.
    #[must_use]
    pub fn new(text: &str) -> Option<Self> {
        let valid = !text.is_empty()
            && !text.starts_with('-')
            && !text.ends_with('-')
            && text
                .bytes()
                .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-');
        valid.then(|| Self(text.to_owned()))
    }

    /// The topic as text.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }

    /// How long a state of this topic stays current by default; `None` for a topic the
    /// spec does not name.
    #[must_use]
    pub fn default_window(&self) -> Option<SignedDuration> {
        let hours = match self.0.as_str() {
            "music" => 21 * 24,
            "watching" | "reading" => 30 * 24,
            "location" => 24,
            "home" => return Some(SignedDuration::from_mins(15)),
            _ => return None,
        };
        Some(SignedDuration::from_hours(hours))
    }
}

impl fmt::Display for Topic {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

/// The keys of a state log.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StateLogFields {
    /// What the states are about.
    pub topic: Topic,
    /// The year the file covers.
    pub year: i16,
}

/// What a state is about: an entity when one exists, plain text otherwise.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Subject {
    /// `[[nina-simone]]`.
    Link(WikiLink),
    /// A subject without a file.
    Text(String),
}

impl fmt::Display for Subject {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Link(link) => link.fmt(f),
            Self::Text(text) => f.write_str(text),
        }
    }
}

/// When a state was seen: a day in the vault's time zone, or a moment for topics whose
/// window is under a day.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Seen {
    /// A day, in the vault's time zone.
    Day(Date),
    /// A moment, with its offset.
    At(DateTime),
}

impl Seen {
    /// Reads `YYYY-MM-DD` or an RFC 3339 date and time with its offset.
    #[must_use]
    pub fn parse(text: &str) -> Option<Self> {
        match super::event::EventTime::parse(text)? {
            super::event::EventTime::Day(day) => Some(Self::Day(day)),
            super::event::EventTime::At(at) => Some(Self::At(at)),
        }
    }

    /// Orders days and moments together: a day counts as its first second in UTC.
    fn sort_key(&self) -> jiff::Timestamp {
        match self {
            Self::Day(day) => day
                .to_zoned(jiff::tz::TimeZone::UTC)
                .map_or(jiff::Timestamp::MIN, |z| z.timestamp()),
            Self::At(at) => at.timestamp(),
        }
    }

    /// How long ago this was, at `now`. A day is as old as the whole days between it
    /// and today, in `now`'s time zone: seen today is 0, yesterday is 24 hours.
    #[must_use]
    pub fn age(&self, now: &Zoned) -> SignedDuration {
        match self {
            Self::Day(day) => {
                let days = now.date().since(*day).map_or(0, |span| span.get_days());
                SignedDuration::from_hours(i64::from(days) * 24)
            }
            Self::At(at) => now.timestamp().duration_since(at.timestamp()),
        }
    }
}

impl fmt::Display for Seen {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Day(day) => day.fmt(f),
            Self::At(at) => at.fmt(f),
        }
    }
}

/// One row of a state log.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StateRow {
    /// What the state is about.
    pub subject: Subject,
    /// When it was first seen.
    pub first_seen: Seen,
    /// When it was last seen.
    pub last_seen: Seen,
    /// How many raw observations support it.
    pub count: u64,
    /// Where the observations came from: `device`, `netflix`.
    pub source: String,
}

/// Whether a state still holds (`docs/spec/vault.md`, section 5.2).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Status {
    /// Seen within the topic's window.
    Current,
    /// Seen within three windows.
    Recent,
    /// Older.
    Past,
}

impl Status {
    /// The status of a state last seen at `last_seen`, at `now`, for a topic whose
    /// window is `window`. A state seen in the future counts as current.
    #[must_use]
    pub fn of(last_seen: &Seen, now: &Zoned, window: SignedDuration) -> Self {
        let age = last_seen.age(now);
        if age <= window {
            Self::Current
        } else if age <= window.saturating_mul(3) {
            Self::Recent
        } else {
            Self::Past
        }
    }
}

/// Why a state table could not be read.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TableError {
    /// The line of the body, from 1.
    pub line: u32,
    /// What is wrong, in words for the person; never the line's content.
    pub problem: String,
}

const COLUMNS: usize = 5;

/// The rows of a state log's table. Lines outside the table are ignored; a table row
/// with the wrong number of columns or a value of the wrong form is reported, not
/// guessed.
///
/// # Errors
///
/// [`TableError`] naming the first wrong line.
pub fn parse_table(body: &str) -> Result<Vec<StateRow>, TableError> {
    let mut rows = Vec::new();
    let mut header_seen = false;
    for (n, line) in body.lines().enumerate() {
        let line_number = u32::try_from(n + 1).unwrap_or(u32::MAX);
        let error = |problem: String| TableError {
            line: line_number,
            problem,
        };
        let line = line.trim();
        if !line.starts_with('|') {
            continue;
        }
        let cells = cells(line);
        if cells.len() != COLUMNS {
            return Err(error(format!(
                "this row has {} columns; a state row has 5: subject, first seen, last seen, count, source",
                cells.len()
            )));
        }
        if !header_seen {
            header_seen = true;
            continue;
        }
        if cells
            .iter()
            .all(|c| !c.is_empty() && c.chars().all(|ch| matches!(ch, '-' | ':')))
        {
            continue;
        }
        let subject = if cells[0].starts_with("[[") {
            Subject::Link(WikiLink::parse(cells[0]).ok_or_else(|| {
                error("the subject must be a link like [[nina-simone]], or plain text".to_owned())
            })?)
        } else if cells[0].is_empty() {
            return Err(error("the subject is empty".to_owned()));
        } else {
            Subject::Text(cells[0].replace("\\|", "|"))
        };
        let seen = |text: &str, column: &str| {
            Seen::parse(text).ok_or_else(|| {
                error(format!(
                    "`{column}` must be a date like 2026-11-12, or a date and time with its offset"
                ))
            })
        };
        rows.push(StateRow {
            subject,
            first_seen: seen(cells[1], "First seen")?,
            last_seen: seen(cells[2], "Last seen")?,
            count: cells[3]
                .parse()
                .map_err(|_| error("`Count` must be a whole number".to_owned()))?,
            source: cells[4].replace("\\|", "|"),
        });
    }
    Ok(rows)
}

/// The cells of a table row, trimmed. A `|` inside a link (`[[a|b]]`) or escaped
/// (`\|`) does not split.
fn cells(line: &str) -> Vec<&str> {
    let inner = line.strip_prefix('|').unwrap_or(line);
    let inner = inner.strip_suffix('|').unwrap_or(inner);
    let mut cells = Vec::new();
    let mut start = 0;
    let mut depth = 0_u32;
    let bytes = inner.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        match bytes[i] {
            b'\\' => i += 1,
            b'[' if bytes.get(i + 1) == Some(&b'[') => {
                depth += 1;
                i += 1;
            }
            b']' if bytes.get(i + 1) == Some(&b']') => {
                depth = depth.saturating_sub(1);
                i += 1;
            }
            b'|' if depth == 0 => {
                cells.push(inner[start..i].trim());
                start = i + 1;
            }
            _ => {}
        }
        i += 1;
    }
    cells.push(inner[start..].trim());
    cells
}

/// The table of a state log, rows ordered by last seen, most recent first.
#[must_use]
pub fn render_table(rows: &[StateRow]) -> String {
    let mut sorted: Vec<&StateRow> = rows.iter().collect();
    sorted.sort_by(|a, b| {
        b.last_seen
            .sort_key()
            .cmp(&a.last_seen.sort_key())
            .then_with(|| a.subject.to_string().cmp(&b.subject.to_string()))
    });
    let mut out = String::with_capacity(64 * (rows.len() + 2));
    out.push_str("\n| Subject | First seen | Last seen | Count | Source |\n| --- | --- | --- | --- | --- |\n");
    for row in sorted {
        let subject = match &row.subject {
            // A link's `|` is read as part of the link.
            Subject::Link(link) => link.to_string(),
            Subject::Text(text) => text.replace('|', "\\|"),
        };
        let _ = writeln!(
            out,
            "| {subject} | {} | {} | {} | {} |",
            row.first_seen,
            row.last_seen,
            row.count,
            row.source.replace('|', "\\|")
        );
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn paris(text: &str) -> Zoned {
        format!("{text}[Europe/Paris]").parse().unwrap()
    }

    #[test]
    fn status_at_the_exact_boundaries() {
        let window = SignedDuration::from_hours(21 * 24);
        let day = |d: &str| Seen::Day(d.parse().unwrap());
        let now = paris("2026-11-22T18:00:00+01:00");
        assert_eq!(
            Status::of(&day("2026-11-22"), &now, window),
            Status::Current
        );
        assert_eq!(
            Status::of(&day("2026-11-01"), &now, window),
            Status::Current,
            "age == window"
        );
        assert_eq!(Status::of(&day("2026-10-31"), &now, window), Status::Recent);
        assert_eq!(
            Status::of(&day("2026-09-20"), &now, window),
            Status::Recent,
            "age == 3 × window"
        );
        assert_eq!(Status::of(&day("2026-09-19"), &now, window), Status::Past);
        assert_eq!(
            Status::of(&day("2026-12-01"), &now, window),
            Status::Current,
            "seen in the future"
        );

        let home = SignedDuration::from_mins(15);
        let at = |t: &str| Seen::At(DateTime::parse(t).unwrap());
        let now = paris("2026-11-12T18:45:00+01:00");
        assert_eq!(
            Status::of(&at("2026-11-12T18:30:00+01:00"), &now, home),
            Status::Current
        );
        assert_eq!(
            Status::of(&at("2026-11-12T17:30:00Z"), &now, home),
            Status::Current,
            "same instant in UTC"
        );
        assert_eq!(
            Status::of(&at("2026-11-12T18:29:59+01:00"), &now, home),
            Status::Recent
        );
        assert_eq!(
            Status::of(&at("2026-11-12T18:00:00+01:00"), &now, home),
            Status::Recent
        );
        assert_eq!(
            Status::of(&at("2026-11-12T17:59:59+01:00"), &now, home),
            Status::Past
        );
    }

    #[test]
    fn default_windows() {
        let window = |t: &str| Topic::new(t).unwrap().default_window();
        assert_eq!(window("music"), Some(SignedDuration::from_hours(504)));
        assert_eq!(window("home"), Some(SignedDuration::from_mins(15)));
        assert_eq!(window("location"), Some(SignedDuration::from_hours(24)));
        assert_eq!(window("podcasts"), None);
        assert!(Topic::new("Music").is_none() && Topic::new("-x").is_none());
    }

    const TABLE: &str = "
| Subject | First seen | Last seen | Count | Source |
| --- | --- | --- | --- | --- |
|  [[justice]]  | 2026-11-08 | 2026-11-11 |  21  | device |
| [[nina-simone|Nina]] | 2026-10-20 | 2026-11-12 | 64 | device |
| Une série sans fiche | 2026-10-02 | 2026-10-30 | 9 | netflix |
";

    #[test]
    fn tables_read_with_extra_spaces_and_write_by_last_seen() {
        let rows = parse_table(TABLE).unwrap();
        assert_eq!(rows.len(), 3);
        assert_eq!(rows[0].subject.to_string(), "[[justice]]");
        assert_eq!(rows[0].count, 21);
        assert_eq!(rows[1].subject.to_string(), "[[nina-simone|Nina]]");
        assert_eq!(
            rows[2].subject,
            Subject::Text("Une série sans fiche".into())
        );

        let written = render_table(&rows);
        let order: Vec<&str> = written
            .lines()
            .skip(3)
            .map(|l| l.split(" | ").next().unwrap())
            .collect();
        assert_eq!(
            order,
            [
                "| [[nina-simone|Nina]]",
                "| [[justice]]",
                "| Une série sans fiche"
            ]
        );
        assert_eq!(parse_table(&written).unwrap().len(), 3, "reads back");
    }

    #[test]
    fn wrong_rows_are_reported_not_guessed() {
        let missing = "| Subject | First seen | Last seen | Count | Source |\n| --- | --- | --- | --- | --- |\n| [[x]] | 2026-11-08 | 2026-11-11 | 21 |\n";
        let error = parse_table(missing).unwrap_err();
        assert_eq!(error.line, 3);
        assert!(error.problem.contains("4 columns"), "{}", error.problem);

        let extra = "| a | b | c | d | e | f |\n";
        assert_eq!(parse_table(extra).unwrap_err().line, 1);

        for (row, word) in [
            ("| [[x]] | 08/11/2026 | 2026-11-11 | 1 | d |", "First seen"),
            (
                "| [[x]] | 2026-11-08 | 2026-11-11T10:00:00 | 1 | d |",
                "Last seen",
            ),
            ("| [[x]] | 2026-11-08 | 2026-11-11 | many | d |", "Count"),
            ("| [[x | 2026-11-08 | 2026-11-11 | 1 | d |", "columns"),
            ("|  | 2026-11-08 | 2026-11-11 | 1 | d |", "empty"),
        ] {
            let text = format!("| S | F | L | C | S |\n|---|---|---|---|---|\n{row}\n");
            let error = parse_table(&text).unwrap_err();
            assert_eq!(error.line, 3, "{row}");
            assert!(error.problem.contains(word), "{row}: {}", error.problem);
            assert!(!error.problem.contains("08/11"), "never quotes the line");
        }
    }

    #[test]
    fn date_times_for_short_windows() {
        let text = "| S | F | L | C | S |\n|---|---|---|---|---|\n| salon | 2026-11-12T08:00:00+01:00 | 2026-11-12T18:42:00+01:00 | 12 | home-assistant |\n";
        let rows = parse_table(text).unwrap();
        assert_eq!(rows[0].last_seen.to_string(), "2026-11-12T18:42:00+01:00");
        assert!(render_table(&rows).contains("| salon | 2026-11-12T08:00:00+01:00 |"));
    }
}
