//! The query layer: the one place data leaves the vault for an AI (`docs/spec/rights.md`,
//! sections 4 and 6).
//!
//! Rules every read follows:
//! - the access code is checked before anything is read for the request;
//! - a file outside the client's categories is never returned, counted or hinted at;
//! - a fact, relation or line of notes that links to such a file is left out entirely,
//!   so that nothing shows that something is hidden;
//! - the category is checked again on the file as it is when read, not only in the index.

use std::path::{Path, PathBuf};

use serde::Serialize;

use jiff::civil::Date;
use jiff::tz::TimeZone;
use jiff::{Timestamp, ToSpan, Zoned};

use super::clients::{Client, Clients};
use crate::search::{EventRef, Results, SearchIndex};
use crate::vault::state::{Seen, Status, Subject, Topic, parse_table};
use crate::vault::{DateTime, EventTime, FileType, Id, LinkOrText, MemoryFile, Vault};
use crate::{Error, Result};

/// Most events one call returns.
pub const MAX_EVENTS: usize = 200;

/// Most states one call returns.
pub const MAX_STATES: usize = 200;

/// The window of a topic with no default and no setting.
const FALLBACK_WINDOW_DAYS: i64 = 7;

/// How often [`Gateway::fresh_access`] looks at every file for edits made in place;
/// added, removed and renamed files are seen at once.
pub const FULL_CHECK_EVERY: std::time::Duration = std::time::Duration::from_secs(2);

/// What an AI can ask for, opened on one vault.
#[derive(Debug)]
pub struct Gateway {
    vault: Vault,
    index: SearchIndex,
    clients: Clients,
    /// When every file was last checked for edits.
    checked: std::time::Instant,
}

impl Gateway {
    /// Opens the vault in `root`, reads its entities and its connected AIs.
    ///
    /// # Errors
    ///
    /// Errors of [`Vault::open`], [`SearchIndex::load`] and [`Clients::load`].
    pub fn open(root: &Path) -> Result<Self> {
        let vault = Vault::open(root)?;
        let index = SearchIndex::load(vault.root())?;
        let clients = Clients::load(vault.root())?;
        Ok(Self {
            vault,
            index,
            clients,
            checked: std::time::Instant::now(),
        })
    }

    /// Reads the entities and the connected AIs again, after the vault changed.
    ///
    /// # Errors
    ///
    /// As [`Gateway::open`].
    pub fn refresh(&mut self) -> Result<()> {
        self.index = SearchIndex::load(self.vault.root())?;
        self.clients = Clients::load(self.vault.root())?;
        Ok(())
    }

    /// Reads the connected AIs again, checks `token`, then reads the entities again if
    /// the vault changed since they were read: a file added, removed or renamed is seen
    /// at once, a file edited in place within [`FULL_CHECK_EVERY`]. For a server that runs as long as the AI
    /// app: rights apply from the next request, and nothing is read for a refused code.
    ///
    /// # Errors
    ///
    /// [`Error::Refused`] when the code is missing or not valid; errors of
    /// [`Gateway::open`] when the vault cannot be read.
    pub fn fresh_access(&mut self, token: Option<&str>) -> Result<Access<'_>> {
        self.clients = Clients::load(self.vault.root())?;
        self.access(token)?;
        let root = self.vault.root();
        let mut reload = self.index.folders_changed(root);
        if !reload && self.checked.elapsed() >= FULL_CHECK_EVERY {
            self.checked = std::time::Instant::now();
            reload = self.index.changed(root)?;
        }
        if reload {
            self.index = SearchIndex::load(root)?;
            self.checked = std::time::Instant::now();
        }
        self.access(token)
    }

    /// The reads allowed by `token`. A missing or unknown code is refused before
    /// anything is read for the request.
    ///
    /// # Errors
    ///
    /// [`Error::Refused`] when the code is missing or not valid.
    pub fn access(&self, token: Option<&str>) -> Result<Access<'_>> {
        let client = token
            .filter(|t| !t.is_empty())
            .and_then(|t| self.clients.authenticate(t))
            .ok_or_else(|| {
                Error::Refused(
                    "This access code is not valid. Connect the AI again with `ipse client add`."
                        .to_owned(),
                )
            })?;
        Ok(Access {
            gateway: self,
            client,
        })
    }
}

/// Reads on behalf of one connected AI.
#[derive(Debug, Clone, Copy)]
pub struct Access<'g> {
    gateway: &'g Gateway,
    client: &'g Client,
}

/// One entity or event, as an AI receives it.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
pub struct Entry {
    /// Stable identifier.
    pub id: String,
    /// What the entity is: `person`, `place`…
    #[serde(rename = "type")]
    pub file_type: String,
    /// Display name.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    /// Other names.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub aliases: Vec<String>,
    /// `YYYY-MM-DD`, or `--MM-DD` without the year.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub birthday: Option<String>,
    /// Email addresses.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub emails: Vec<String>,
    /// Phone numbers.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub phones: Vec<String>,
    /// Postal address.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub address: Option<String>,
    /// The place it is in, as a link; left out when that place is hidden.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub located_in: Option<String>,
    /// Facts; those linking to a hidden file are left out.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub facts: Vec<String>,
    /// Relations; those pointing to a hidden file are left out.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub relations: Vec<RelationOut>,
    /// Notes; lines linking to a hidden file are left out.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub notes: Option<String>,
    /// For an event: when it starts, `YYYY-MM-DD` for a whole day, otherwise RFC 3339
    /// with the vault's offset.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub start: Option<String>,
    /// For an event: when it ends, exclusive.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub end: Option<String>,
    /// For an event: where, as a link or the address; a hidden place is left out.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub place: Option<String>,
    /// For an event: who, as links or names, the owner implied; hidden people are
    /// left out.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub participants: Vec<String>,
    /// For an event: `planned`, `done` or `cancelled`, when known.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub status: Option<String>,
}

/// Events of a period.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
pub struct Events {
    /// Ordered by start.
    pub events: Vec<Entry>,
    /// True when there were more than the limit.
    pub more: bool,
}

/// One state, as an AI receives it (`docs/spec/vault.md`, section 5).
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct StateOut {
    /// `music`, `watching`…
    pub topic: String,
    /// A link, or plain text.
    pub subject: String,
    /// When it was first seen.
    pub first_seen: String,
    /// When it was last seen.
    pub last_seen: String,
    /// How many observations support it.
    pub count: u64,
    /// Where they came from.
    pub source: String,
    /// `current`, `recent` or `past`; given by `states_history` only.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub status: Option<&'static str>,
}

/// States of a period.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
pub struct States {
    /// Most recently seen first.
    pub states: Vec<StateOut>,
    /// True when there were more than the limit.
    pub more: bool,
}

/// A relation as an AI receives it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct RelationOut {
    /// The relation word: `mère`, `works at`.
    pub word: String,
    /// `[[target]]`.
    pub target: String,
}

impl<'g> Access<'g> {
    /// The connected AI these reads are for.
    #[must_use]
    pub fn client(&self) -> &'g Client {
        self.client
    }

    /// Entities matching `query`, best first, among the categories this AI may read.
    /// `limit` is clamped to 1–50.
    #[must_use]
    pub fn search(&self, query: &str, limit: usize) -> Results {
        self.gateway.index.search_with(
            query,
            limit,
            |c| self.client.reads(c.as_str()),
            |text| self.hides(text).then(|| self.strip(text)),
        )
    }

    /// The entity with this id, if this AI may read it.
    #[must_use]
    pub fn get(&self, id: &str) -> Option<Entry> {
        let id = Id::parse(id)?;
        let path = self
            .gateway
            .index
            .path_of(&id, |c| self.client.reads(c.as_str()))?;
        self.entry(path)
    }

    /// The entities named `name` (name or alias, ignoring case and accents) this AI may
    /// read.
    #[must_use]
    pub fn get_by_name(&self, name: &str) -> Vec<Entry> {
        self.gateway
            .index
            .named(name, |c| self.client.reads(c.as_str()))
            .into_iter()
            .filter_map(|path| self.entry(path))
            .collect()
    }

    /// The owner's own file, if this AI may read the `identity` category.
    #[must_use]
    pub fn about_me(&self) -> Option<Entry> {
        let owner = self.gateway.vault.owner_file();
        let relative = owner
            .strip_prefix(self.gateway.vault.root())
            .unwrap_or(owner);
        self.entry(relative)
    }

    /// The events of this AI's categories that overlap `from`–`to` (days count whole,
    /// in the vault's time zone), with the entity `with` among their participants when
    /// given, ordered by start. `limit` is clamped to 1–200.
    #[must_use]
    pub fn events(
        &self,
        from: &EventTime,
        to: &EventTime,
        with: Option<&str>,
        limit: usize,
    ) -> Events {
        let tz = self.time_zone();
        let (Some(from), Some(to)) = (instant(from, tz, false), instant(to, tz, true)) else {
            return Events::default();
        };
        let with = match with {
            None => None,
            Some(id) => match self.stem_of(id) {
                Some(stem) => Some(stem),
                // Unknown and hidden look the same: nothing.
                None => return Events::default(),
            },
        };
        let limit = limit.clamp(1, MAX_EVENTS);
        let overlaps = |e: &EventRef<'_>| {
            let Some(start) = instant(e.start, tz, false) else {
                return false;
            };
            let end = match e.end {
                Some(end) => instant(end, tz, false).unwrap_or(start),
                None if e.start.is_day() => instant(e.start, tz, true).unwrap_or(start),
                None => start,
            };
            start < to && (end > from || (end == start && start >= from))
        };
        let with_them = |e: &EventRef<'_>| {
            with.as_deref()
                .is_none_or(|stem| e.participants.iter().any(|p| p.eq_ignore_ascii_case(stem)))
        };
        let mut events = Vec::with_capacity(limit);
        let mut more = false;
        let found = self.gateway.index.events(
            |c| self.client.reads(c.as_str()),
            |e| overlaps(e) && with_them(e),
        );
        for event in found {
            if events.len() == limit {
                more = true;
                break;
            }
            events.extend(self.entry(event.path));
        }
        Events { events, more }
    }

    /// The states that hold now, of `topics` (every topic when empty), each with when
    /// it was last seen. Recent and past states are never returned here.
    #[must_use]
    pub fn now(&self, topics: &[String]) -> States {
        let now = Timestamp::now().to_zoned(self.time_zone().clone());
        let years = [now.year() - 1, now.year()];
        let mut states = Vec::new();
        for topic in self.topics(topics) {
            let window = self.window(&topic);
            for (row, _) in self.rows(&topic, &years) {
                if Status::of(&row.last_seen, &now, window) == Status::Current {
                    states.push(self.state_out(&topic, &row, None));
                }
            }
        }
        cap(states, MAX_STATES)
    }

    /// Every state of `topic` seen during `from`–`to`, current, recent or past, with its
    /// status. The only way to read past states. `limit` is clamped to 1–200.
    #[must_use]
    pub fn states_history(
        &self,
        topic: &str,
        from: &EventTime,
        to: &EventTime,
        limit: usize,
    ) -> States {
        let Some(topic) = Topic::new(topic) else {
            return States::default();
        };
        let tz = self.time_zone();
        let (Some(from_at), Some(to_at)) = (instant(from, tz, false), instant(to, tz, true)) else {
            return States::default();
        };
        let now = Timestamp::now().to_zoned(tz.clone());
        let window = self.window(&topic);
        let first = from.date().year();
        let last = to.date().year();
        let years: Vec<i16> = (first..=last).collect();
        let mut states = Vec::new();
        for (row, _) in self.rows(&topic, &years) {
            let (Some(first_seen), Some(last_seen)) = (
                seen_instant(&row.first_seen, tz, false),
                seen_instant(&row.last_seen, tz, true),
            ) else {
                continue;
            };
            if first_seen < to_at && last_seen > from_at {
                let label = match Status::of(&row.last_seen, &now, window) {
                    Status::Current => "current",
                    Status::Recent => "recent",
                    Status::Past => "past",
                };
                states.push(self.state_out(&topic, &row, Some(label)));
            }
        }
        cap(states, limit.clamp(1, MAX_STATES))
    }

    fn time_zone(&self) -> &TimeZone {
        self.gateway.vault.settings().time_zone()
    }

    fn window(&self, topic: &Topic) -> jiff::SignedDuration {
        self.gateway
            .vault
            .settings()
            .window(topic)
            .unwrap_or_else(|| jiff::SignedDuration::from_hours(FALLBACK_WINDOW_DAYS * 24))
    }

    /// The file stem of the entity with this id, if this AI may read it.
    fn stem_of(&self, id: &str) -> Option<String> {
        let id = Id::parse(id)?;
        let path = self
            .gateway
            .index
            .path_of(&id, |c| self.client.reads(c.as_str()))?;
        Some(path.file_stem()?.to_string_lossy().into_owned())
    }

    /// `topics`, or every topic folder of the vault when empty.
    fn topics(&self, topics: &[String]) -> Vec<Topic> {
        if !topics.is_empty() {
            return topics.iter().filter_map(|t| Topic::new(t)).collect();
        }
        let dir = self.gateway.vault.root().join(FileType::StateLog.folder());
        let mut found: Vec<Topic> = std::fs::read_dir(dir)
            .into_iter()
            .flatten()
            .flatten()
            .filter_map(|e| Topic::new(&e.file_name().to_string_lossy()))
            .collect();
        found.sort();
        found
    }

    /// The rows of `topic` for `years` that this AI may read: files of its categories,
    /// rows whose subject it may follow. Unreadable files are skipped.
    fn rows(&self, topic: &Topic, years: &[i16]) -> Vec<(crate::vault::StateRow, i16)> {
        let mut rows = Vec::new();
        for &year in years {
            let path = crate::vault::naming::state_log_path(self.gateway.vault.root(), topic, year);
            let Ok(file) = MemoryFile::read(&path) else {
                continue;
            };
            if !self.client.reads(file.common().category.as_str()) || file.state_log().is_none() {
                continue;
            }
            let Ok(table) = parse_table(file.body()) else {
                continue;
            };
            rows.extend(
                table
                    .into_iter()
                    .filter(|r| match &r.subject {
                        Subject::Link(link) => self.follows(&link.target),
                        Subject::Text(_) => true,
                    })
                    .map(|r| (r, year)),
            );
        }
        rows.sort_by_key(|(r, _)| {
            std::cmp::Reverse(seen_instant(&r.last_seen, self.time_zone(), false))
        });
        rows
    }

    fn state_out(
        &self,
        topic: &Topic,
        row: &crate::vault::StateRow,
        status: Option<&'static str>,
    ) -> StateOut {
        StateOut {
            topic: topic.to_string(),
            subject: row.subject.to_string(),
            first_seen: self.seen_text(&row.first_seen),
            last_seen: self.seen_text(&row.last_seen),
            count: row.count,
            source: row.source.clone(),
            status,
        }
    }

    fn seen_text(&self, seen: &Seen) -> String {
        match seen {
            Seen::Day(day) => day.to_string(),
            Seen::At(at) => DateTime::at(at.timestamp(), self.time_zone()).to_string(),
        }
    }

    /// An event time as the AI receives it: a day as is, a moment with the vault's
    /// offset.
    fn time_text(&self, time: &EventTime) -> String {
        match time {
            EventTime::Day(day) => day.to_string(),
            EventTime::At(at) => DateTime::at(at.timestamp(), self.time_zone()).to_string(),
        }
    }

    /// A link this AI may follow, or plain text; `None` for a hidden link.
    fn visible(&self, value: &LinkOrText) -> Option<String> {
        match value {
            LinkOrText::Link(link) => self.follows(&link.target).then(|| link.to_string()),
            LinkOrText::Text(text) => (!self.hides(text)).then(|| text.clone()),
        }
    }

    /// Reads one entity or event file and checks its category on the file itself.
    fn entry(&self, relative: &Path) -> Option<Entry> {
        let path: PathBuf = self.gateway.vault.root().join(relative);
        let file = MemoryFile::read(&path).ok()?;
        let common = file.common();
        if !self.client.reads(common.category.as_str()) {
            return None;
        }
        if let Some(event) = file.event() {
            return Some(Entry {
                id: common.id.to_string(),
                file_type: type_name(common.file_type),
                name: Some(event.title.clone()),
                notes: file
                    .sections()
                    .notes
                    .as_deref()
                    .map(|n| self.strip(n))
                    .filter(|n| !n.trim().is_empty()),
                start: Some(self.time_text(&event.start)),
                end: event.end.as_ref().map(|e| self.time_text(e)),
                place: event.place.as_ref().and_then(|p| self.visible(p)),
                participants: event
                    .participants
                    .iter()
                    .filter_map(|p| self.visible(p))
                    .collect(),
                status: event.status.map(|s| s.as_str().to_owned()),
                ..Entry::default()
            });
        }
        let entity = file.entity()?;
        let sections = file.sections();
        Some(Entry {
            id: common.id.to_string(),
            file_type: type_name(common.file_type),
            name: entity.name.clone(),
            aliases: entity.aliases.clone(),
            birthday: entity.birthday.map(|b| b.to_string()),
            emails: entity.emails.clone(),
            phones: entity.phones.clone(),
            address: entity.address.clone(),
            located_in: entity
                .located_in
                .as_ref()
                .filter(|l| self.follows(&l.target))
                .map(ToString::to_string),
            facts: sections
                .facts
                .iter()
                .filter(|f| !self.hides(&f.text))
                .map(|f| f.text.clone())
                .collect(),
            relations: sections
                .relations
                .iter()
                .filter(|r| self.follows(&r.link.target))
                .map(|r| RelationOut {
                    word: r.word.clone(),
                    target: r.link.to_string(),
                })
                .collect(),
            notes: sections
                .notes
                .as_deref()
                .map(|n| self.strip(n))
                .filter(|n| !n.trim().is_empty()),
            ..Entry::default()
        })
    }

    /// `text` without the lines that link to anything this AI may not read. A fact is
    /// one line: it is left out entirely.
    fn strip(&self, text: &str) -> String {
        let mut out = String::with_capacity(text.len());
        for line in text.split_inclusive('\n') {
            if !self.hides(line) {
                out.push_str(line);
            }
        }
        out.trim_end_matches('\n').to_owned()
    }

    /// True when `text` holds a `[[link]]` this AI may not follow.
    fn hides(&self, text: &str) -> bool {
        let mut rest = text;
        while let Some(open) = rest.find("[[") {
            let Some(close) = rest[open..].find("]]").map(|c| open + c) else {
                return false;
            };
            let target = rest[open + 2..close].split('|').next().unwrap_or_default();
            if !self.follows(target.trim()) {
                return true;
            }
            rest = &rest[close + 2..];
        }
        false
    }

    /// True when a link to `target` points to known entities this AI may all read. A
    /// link to nothing known counts as hidden.
    fn follows(&self, target: &str) -> bool {
        let categories = self.gateway.index.categories_of(target);
        !categories.is_empty() && categories.iter().all(|c| self.client.reads(c.as_str()))
    }
}

fn type_name(file_type: FileType) -> String {
    file_type.as_str().to_owned()
}

/// The instant an event time stands for: a moment as is; a day at its start in `tz`,
/// or at the start of the next day when it ends a period (`end_of_day`).
fn instant(time: &EventTime, tz: &TimeZone, end_of_day: bool) -> Option<Timestamp> {
    match time {
        EventTime::At(at) => Some(at.timestamp()),
        EventTime::Day(day) => day_start(*day, tz, end_of_day),
    }
}

fn seen_instant(seen: &Seen, tz: &TimeZone, end_of_day: bool) -> Option<Timestamp> {
    match seen {
        Seen::At(at) => Some(at.timestamp()),
        Seen::Day(day) => day_start(*day, tz, end_of_day),
    }
}

fn day_start(day: Date, tz: &TimeZone, next: bool) -> Option<Timestamp> {
    let day = if next {
        day.checked_add(1.day()).ok()?
    } else {
        day
    };
    let zoned: Zoned = day.to_zoned(tz.clone()).ok()?;
    Some(zoned.timestamp())
}

fn cap(mut states: Vec<StateOut>, limit: usize) -> States {
    let more = states.len() > limit;
    states.truncate(limit);
    States { states, more }
}
