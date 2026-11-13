//! Typed values of a memory file's front matter (`docs/spec/vault.md`, sections 2.2 and 3.1).
//!
//! Each type checks its format when it is built, so a value held by Ipse is always
//! valid and can be written back without further checks.

use std::fmt;

use jiff::fmt::temporal::Pieces;
use jiff::tz::{Offset, TimeZone};
use jiff::{Timestamp, civil};

/// Stable identifier of a memory file: a lowercase [ULID](https://github.com/ulid/spec).
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct Id(String);

const CROCKFORD_LOWER: &[u8; 32] = b"0123456789abcdefghjkmnpqrstvwxyz";

impl Id {
    /// A new identifier, sortable by creation time.
    #[must_use]
    pub fn generate() -> Self {
        Self(ulid::Ulid::generate().to_string().to_ascii_lowercase())
    }

    /// Checks that `text` is a lowercase ULID. Returns `None` otherwise.
    #[must_use]
    pub fn parse(text: &str) -> Option<Self> {
        let bytes = text.as_bytes();
        let valid = bytes.len() == 26
            && bytes[0] <= b'7'
            && bytes.iter().all(|b| CROCKFORD_LOWER.contains(b));
        valid.then(|| Self(text.to_owned()))
    }

    /// The identifier as text.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for Id {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

/// What a memory file holds: its `type` key.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum FileType {
    /// A person, under `people/`.
    Person,
    /// A place, under `places/`.
    Place,
    /// A company, a school, a club…, under `organizations/`.
    Organization,
    /// A project, under `projects/`.
    Project,
    /// An object or a device, under `things/`.
    Thing,
    /// An interest or a theme, under `topics/`.
    Topic,
    /// A film, a series, a book, an album, under `works/`.
    Work,
    /// A dated event, under `events/`.
    Event,
    /// A "right now" state log, under `states/`.
    StateLog,
    /// A summary of a period, under `summaries/`.
    Summary,
}

impl FileType {
    /// Every type, in the order of the spec.
    pub const ALL: [Self; 10] = [
        Self::Person,
        Self::Place,
        Self::Organization,
        Self::Project,
        Self::Thing,
        Self::Topic,
        Self::Work,
        Self::Event,
        Self::StateLog,
        Self::Summary,
    ];

    /// The value of the `type` key.
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Person => "person",
            Self::Place => "place",
            Self::Organization => "organization",
            Self::Project => "project",
            Self::Thing => "thing",
            Self::Topic => "topic",
            Self::Work => "work",
            Self::Event => "event",
            Self::StateLog => "state-log",
            Self::Summary => "summary",
        }
    }

    /// Reads a `type` value. Returns `None` for an unknown type.
    #[must_use]
    pub fn parse(text: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|t| t.as_str() == text)
    }

    /// True for entities: people, places, organizations, projects, things, topics, works.
    #[must_use]
    pub fn is_entity(self) -> bool {
        !matches!(self, Self::Event | Self::StateLog | Self::Summary)
    }

    /// The vault folder files of this type live in.
    #[must_use]
    pub fn folder(self) -> &'static str {
        match self {
            Self::Person => "people",
            Self::Place => "places",
            Self::Organization => "organizations",
            Self::Project => "projects",
            Self::Thing => "things",
            Self::Topic => "topics",
            Self::Work => "works",
            Self::Event => "events",
            Self::StateLog => "states",
            Self::Summary => "summaries",
        }
    }

    /// The rights category a new file of this type gets (`docs/spec/rights.md`, section 1).
    /// `None` for state logs, whose category depends on their topic.
    #[must_use]
    pub fn default_category(self) -> Option<Category> {
        let name = match self {
            Self::Person | Self::Organization => "people",
            Self::Place => "places",
            Self::Project => "work",
            Self::Thing => "things",
            Self::Topic | Self::Work => "media",
            Self::Event => "agenda",
            Self::Summary => "summaries",
            Self::StateLog => return None,
        };
        Category::new(name)
    }
}

impl fmt::Display for FileType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// A rights category: lowercase letters, digits and inner dashes, like `people` or `home`.
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct Category(String);

impl Category {
    /// Checks the format. Returns `None` for an empty name, uppercase letters, spaces,
    /// or a dash at either end.
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

    /// The category as text.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for Category {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

/// An instant with the UTC offset it was written with: `2026-11-12T19:30:00+01:00`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DateTime {
    timestamp: Timestamp,
    offset: Offset,
}

impl DateTime {
    /// Reads an RFC 3339 date and time with its offset. Returns `None` when the time or
    /// the offset is missing, or the text is not a valid date and time.
    #[must_use]
    pub fn parse(text: &str) -> Option<Self> {
        let pieces = Pieces::parse(text).ok()?;
        let time = pieces.time()?;
        let offset = pieces.to_numeric_offset()?;
        let timestamp = offset.to_timestamp(pieces.date().to_datetime(time)).ok()?;
        Some(Self { timestamp, offset })
    }

    /// The current time, to the second, with the offset of `time_zone`.
    #[must_use]
    pub fn now(time_zone: &TimeZone) -> Self {
        Self::at(Timestamp::now(), time_zone)
    }

    /// `timestamp`, truncated to the second, with the offset of `time_zone` at that instant.
    #[must_use]
    pub fn at(timestamp: Timestamp, time_zone: &TimeZone) -> Self {
        let timestamp = Timestamp::from_second(timestamp.as_second()).unwrap_or(timestamp);
        Self {
            timestamp,
            offset: time_zone.to_offset(timestamp),
        }
    }

    /// The instant.
    #[must_use]
    pub fn timestamp(&self) -> Timestamp {
        self.timestamp
    }

    /// The offset it was written with.
    #[must_use]
    pub fn offset(&self) -> Offset {
        self.offset
    }
}

impl fmt::Display for DateTime {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.timestamp.display_with_offset(self.offset).fmt(f)
    }
}

/// A birthday, with or without its year: `1965-03-12` or `--03-12`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Birthday {
    year: Option<i16>,
    month: i8,
    day: i8,
}

impl Birthday {
    /// Builds a birthday. Returns `None` for an impossible date. Without a year,
    /// February 29 is accepted.
    #[must_use]
    pub fn new(year: Option<i16>, month: i8, day: i8) -> Option<Self> {
        civil::Date::new(year.unwrap_or(2000), month, day).ok()?;
        Some(Self { year, month, day })
    }

    /// Reads `YYYY-MM-DD` or `--MM-DD`. Returns `None` for any other form.
    #[must_use]
    pub fn parse(text: &str) -> Option<Self> {
        let (year, rest) = if let Some(rest) = text.strip_prefix("--") {
            (None, rest)
        } else {
            let (year, rest) = text.split_at_checked(4)?;
            (Some(digits(year)?), rest.strip_prefix('-')?)
        };
        let (month, day) = rest.split_once('-')?;
        if month.len() != 2 || day.len() != 2 {
            return None;
        }
        let month = i8::try_from(digits(month)?).ok()?;
        let day = i8::try_from(digits(day)?).ok()?;
        Self::new(year, month, day)
    }

    /// The year, when known.
    #[must_use]
    pub fn year(&self) -> Option<i16> {
        self.year
    }

    /// The month, 1 to 12.
    #[must_use]
    pub fn month(&self) -> i8 {
        self.month
    }

    /// The day of the month, 1 to 31.
    #[must_use]
    pub fn day(&self) -> i8 {
        self.day
    }
}

impl fmt::Display for Birthday {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self.year {
            Some(year) => write!(f, "{year:04}-{:02}-{:02}", self.month, self.day),
            None => write!(f, "--{:02}-{:02}", self.month, self.day),
        }
    }
}

fn digits(text: &str) -> Option<i16> {
    if text.is_empty() || !text.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    text.parse().ok()
}

/// A link to a file of the vault, or plain text when there is no file to link to: a
/// calendar guest without a person file, an address without a place file.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum LinkOrText {
    /// `[[marie-dupont]]`.
    Link(WikiLink),
    /// `Marc Leroy`, `12 rue des Lilas, Lyon`.
    Text(String),
}

impl LinkOrText {
    /// A link when `text` is one (`[[marie-dupont]]`), plain text otherwise.
    #[must_use]
    pub fn parse(text: &str) -> Self {
        WikiLink::parse(text).map_or_else(|| Self::Text(text.to_owned()), Self::Link)
    }

    /// The link, if it is one.
    #[must_use]
    pub fn link(&self) -> Option<&WikiLink> {
        match self {
            Self::Link(link) => Some(link),
            Self::Text(_) => None,
        }
    }
}

impl fmt::Display for LinkOrText {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Link(link) => link.fmt(f),
            Self::Text(text) => f.write_str(text),
        }
    }
}

/// A link to another file of the vault: `[[marie-dupont]]` or `[[marie-dupont|Maman]]`.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct WikiLink {
    /// The file stem the link points to.
    pub target: String,
    /// The text shown instead of the target, if any.
    pub display: Option<String>,
}

impl WikiLink {
    /// Reads `[[target]]` or `[[target|display]]`. Returns `None` for any other form.
    #[must_use]
    pub fn parse(text: &str) -> Option<Self> {
        let inner = text.strip_prefix("[[")?.strip_suffix("]]")?;
        if inner.contains(['[', ']', '\n']) {
            return None;
        }
        let (target, display) = match inner.split_once('|') {
            Some((target, display)) => (target, Some(display.trim().to_owned())),
            None => (inner, None),
        };
        let target = target.trim();
        (!target.is_empty()).then(|| Self {
            target: target.to_owned(),
            display,
        })
    }
}

impl fmt::Display for WikiLink {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match &self.display {
            Some(display) => write!(f, "[[{}|{display}]]", self.target),
            None => write!(f, "[[{}]]", self.target),
        }
    }
}

/// A position in decimal degrees, WGS 84.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Geo {
    lat: f64,
    lon: f64,
}

impl Geo {
    /// Builds a position. Returns `None` outside ±90° latitude and ±180° longitude.
    #[must_use]
    pub fn new(lat: f64, lon: f64) -> Option<Self> {
        ((-90.0..=90.0).contains(&lat) && (-180.0..=180.0).contains(&lon))
            .then_some(Self { lat, lon })
    }

    /// Latitude in degrees.
    #[must_use]
    pub fn lat(&self) -> f64 {
        self.lat
    }

    /// Longitude in degrees.
    #[must_use]
    pub fn lon(&self) -> f64 {
        self.lon
    }
}

/// The keys every memory file has (`docs/spec/vault.md`, section 2.2).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Common {
    /// Stable identifier, never changes.
    pub id: Id,
    /// What the file holds.
    pub file_type: FileType,
    /// Rights category.
    pub category: Category,
    /// When the file was created.
    pub created: DateTime,
    /// When the file last changed.
    pub updated: DateTime,
    /// Where the content came from: `contacts:<id>`, `person`, `proposal:<id>`.
    pub sources: Vec<String>,
}

impl Common {
    /// The keys of a new file of `file_type`, created now in `time_zone`, with the type's
    /// default category (`people` for a person). Returns `None` for a state log, whose
    /// category must be given.
    #[must_use]
    pub fn new(file_type: FileType, time_zone: &TimeZone) -> Option<Self> {
        let now = DateTime::now(time_zone);
        Some(Self {
            id: Id::generate(),
            file_type,
            category: file_type.default_category()?,
            created: now,
            updated: now,
            sources: Vec::new(),
        })
    }
}

/// The keys of an entity (`docs/spec/vault.md`, sections 3.1 and 6).
#[derive(Debug, Clone, Default, PartialEq)]
pub struct EntityFields {
    /// Display name.
    pub name: Option<String>,
    /// Other names the person uses.
    pub aliases: Vec<String>,
    /// True on the vault owner's own file.
    pub owner: bool,
    /// Birthday of a person.
    pub birthday: Option<Birthday>,
    /// Email addresses.
    pub emails: Vec<String>,
    /// Phone numbers, E.164 when known.
    pub phones: Vec<String>,
    /// Postal address, free text.
    pub address: Option<String>,
    /// Position of a place.
    pub geo: Option<Geo>,
    /// The place this place or thing is in.
    pub located_in: Option<WikiLink>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ids() {
        let id = Id::generate();
        assert_eq!(Id::parse(id.as_str()), Some(id));
        assert!(Id::parse("01jcx8m2p4q6r8t0v2w4y6z8a0").is_some());
        assert!(
            Id::parse("01JCX8M2P4Q6R8T0V2W4Y6Z8A0").is_none(),
            "uppercase"
        );
        assert!(
            Id::parse("01jcx8m2p4q6r8t0v2w4y6z8a").is_none(),
            "25 characters"
        );
        assert!(
            Id::parse("01jcx8m2p4q6r8t0v2w4y6z8ai").is_none(),
            "i is not Crockford"
        );
        assert!(
            Id::parse("81jcx8m2p4q6r8t0v2w4y6z8a0").is_none(),
            "overflows 128 bits"
        );
    }

    #[test]
    fn file_types() {
        for t in FileType::ALL {
            assert_eq!(FileType::parse(t.as_str()), Some(t));
        }
        assert_eq!(FileType::parse("state-log"), Some(FileType::StateLog));
        assert_eq!(FileType::parse("Person"), None);
        assert!(FileType::Work.is_entity() && !FileType::Event.is_entity());
    }

    #[test]
    fn categories() {
        assert!(Category::new("people").is_some());
        assert!(Category::new("my-studies-2").is_some());
        for bad in ["", "People", "a b", "-x", "x-", "é"] {
            assert!(Category::new(bad).is_none(), "{bad:?}");
        }
    }

    #[test]
    fn date_times_keep_their_offset() {
        let dt = DateTime::parse("2026-11-12T19:30:00+01:00").unwrap();
        assert_eq!(dt.to_string(), "2026-11-12T19:30:00+01:00");
        assert_eq!(dt.timestamp().to_string(), "2026-11-12T18:30:00Z");
        assert_eq!(
            DateTime::parse("2026-11-12T18:30:00Z").unwrap().to_string(),
            "2026-11-12T18:30:00+00:00"
        );
        assert!(DateTime::parse("2026-11-12").is_none(), "no time");
        assert!(
            DateTime::parse("2026-11-12T19:30:00").is_none(),
            "no offset"
        );
        assert!(DateTime::parse("2026-13-12T19:30:00+01:00").is_none());
    }

    #[test]
    fn now_is_whole_seconds_in_the_zone() {
        let tz = TimeZone::get("Europe/Paris").unwrap();
        let ts: Timestamp = "2026-07-01T10:00:00.75Z".parse().unwrap();
        assert_eq!(
            DateTime::at(ts, &tz).to_string(),
            "2026-07-01T12:00:00+02:00"
        );
    }

    #[test]
    fn birthdays() {
        assert_eq!(
            Birthday::parse("1965-03-12").unwrap().to_string(),
            "1965-03-12"
        );
        assert_eq!(Birthday::parse("--03-12").unwrap().to_string(), "--03-12");
        assert!(
            Birthday::parse("--02-29").is_some(),
            "leap day without a year"
        );
        assert!(Birthday::parse("2023-02-29").is_none());
        for bad in [
            "1965-3-12",
            "65-03-12",
            "--3-12",
            "1965/03/12",
            "",
            "-03-12",
        ] {
            assert!(Birthday::parse(bad).is_none(), "{bad:?}");
        }
    }

    #[test]
    fn wikilinks() {
        let link = WikiLink::parse("[[marie-dupont|Maman]]").unwrap();
        assert_eq!(link.target, "marie-dupont");
        assert_eq!(link.display.as_deref(), Some("Maman"));
        assert_eq!(link.to_string(), "[[marie-dupont|Maman]]");
        assert_eq!(WikiLink::parse("[[lyon]]").unwrap().to_string(), "[[lyon]]");
        for bad in ["lyon", "[[]]", "[lyon]", "[[a]] [[b]]"] {
            assert!(WikiLink::parse(bad).is_none(), "{bad:?}");
        }
    }

    #[test]
    fn geo_ranges() {
        assert!(Geo::new(45.7578, 4.832).is_some());
        assert!(Geo::new(91.0, 0.0).is_none());
        assert!(Geo::new(0.0, f64::NAN).is_none());
    }

    #[test]
    fn new_common_keys() {
        let common = Common::new(FileType::Person, &TimeZone::UTC).unwrap();
        assert_eq!(common.category.as_str(), "people");
        assert_eq!(common.created, common.updated);
        assert!(Common::new(FileType::StateLog, &TimeZone::UTC).is_none());
    }
}
