//! The query layer: the one place data leaves the vault for an AI (`docs/spec/rights.md`,
//! sections 4 and 6).
//!
//! Rules every read follows:
//! - the access code is checked before anything is read for the request;
//! - a file outside the client's categories is never returned, counted or hinted at;
//! - a fact, relation or line of notes that links to such a file is left out entirely,
//!   so that nothing shows that something is hidden;
//! - the category is checked again on the file as it is when read, not only in the index.

use std::cell::RefCell;
use std::collections::{BTreeSet, HashMap};
use std::path::{Path, PathBuf};

use serde::Serialize;

use jiff::civil::Date;
use jiff::tz::TimeZone;
use jiff::{Timestamp, ToSpan, Zoned};

use super::Categories;
use super::clients::{Client, Clients};
use super::log::AccessLog;
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
    pub(super) vault: Vault,
    pub(super) index: SearchIndex,
    clients: Clients,
    pub(super) categories: Categories,
    log: AccessLog,
    /// The owner, for the interface.
    owner: Client,
    /// When every file was last checked for edits.
    checked: std::time::Instant,
    /// When the settings were last changed, as read: categories declared since apply
    /// from the next request.
    settings_changed: Option<std::time::SystemTime>,
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
        let categories = Categories::of(vault.settings());
        Ok(Self {
            settings_changed: settings_changed(vault.root()),
            owner: Client::owner(&categories),
            categories,
            log: AccessLog::new(vault.root()),
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
        self.reload_settings()?;
        self.update_index()?;
        self.clients = Clients::load(self.vault.root())?;
        Ok(())
    }

    /// Reads again the files that changed, and saves the index when it changed.
    fn update_index(&mut self) -> Result<()> {
        let root = self.vault.root();
        if self.index.update(root)? {
            // The saved index only saves time at the next start: failing to save it
            // never stops a read.
            let _ = self.index.save(root);
        }
        Ok(())
    }

    /// Reads the connected AIs again, checks `token`, then reads again the files that
    /// changed since they were read: a file added, removed or renamed is seen
    /// at once, a file edited in place within [`FULL_CHECK_EVERY`]. For a server that runs as long as the AI
    /// app: rights apply from the next request, and nothing is read for a refused code.
    ///
    /// # Errors
    ///
    /// [`Error::Refused`] when the code is missing or not valid; errors of
    /// [`Gateway::open`] when the vault cannot be read.
    pub fn fresh_access(&mut self, token: Option<&str>) -> Result<Access<'_>> {
        self.clients = Clients::load(self.vault.root())?;
        self.reload_settings()?;
        self.access(token)?;
        let root = self.vault.root();
        // Listing every file and reading again those that changed is one pass.
        if self.index.folders_changed(root) || self.checked.elapsed() >= FULL_CHECK_EVERY {
            self.update_index()?;
            self.checked = std::time::Instant::now();
        }
        self.access(token)
    }

    /// The reads of the vault's owner, in the interface: every category, through the
    /// same query layer as AIs, after reading again the files that changed. Not written
    /// to the access log, which is about AIs.
    ///
    /// # Errors
    ///
    /// Errors of [`Gateway::open`] when the vault cannot be read.
    pub fn fresh_owner(&mut self) -> Result<Access<'_>> {
        self.reload_settings()?;
        let root = self.vault.root();
        if self.index.folders_changed(root) || self.checked.elapsed() >= FULL_CHECK_EVERY {
            self.update_index()?;
            self.checked = std::time::Instant::now();
        }
        Ok(Access {
            gateway: self,
            client: &self.owner,
            read: RefCell::new(BTreeSet::new()),
        })
    }

    /// Reads the settings again when their file changed: a category declared since
    /// counts from now on.
    fn reload_settings(&mut self) -> Result<()> {
        let changed = settings_changed(self.vault.root());
        if changed != self.settings_changed {
            self.vault = Vault::open(self.vault.root())?;
            self.categories = Categories::of(self.vault.settings());
            self.owner = Client::owner(&self.categories);
            self.settings_changed = changed;
        }
        Ok(())
    }

    /// The search index, as last read: for the interface's own planning.
    #[must_use]
    pub fn index(&self) -> &SearchIndex {
        &self.index
    }

    /// The vault's settings.
    #[must_use]
    pub fn settings(&self) -> &crate::vault::Settings {
        self.vault.settings()
    }

    /// Changes the memory's language and time zone, as the person (see
    /// [`Vault::change_settings`]).
    ///
    /// # Errors
    ///
    /// Errors of [`Vault::change_settings`].
    pub fn change_settings(
        &mut self,
        language: crate::vault::Language,
        time_zone: &str,
    ) -> Result<()> {
        self.vault.change_settings(language, time_zone)?;
        Ok(())
    }

    /// The vault.
    #[must_use]
    pub fn vault(&self) -> &Vault {
        &self.vault
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
            read: RefCell::new(BTreeSet::new()),
        })
    }
}

/// Reads on behalf of one connected AI, for one request. It remembers the categories
/// of what it returned, for the access log ([`Access::log`]).
#[derive(Debug)]
pub struct Access<'g> {
    pub(super) gateway: &'g Gateway,
    pub(super) client: &'g Client,
    read: RefCell<BTreeSet<String>>,
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
    /// For a reminder: when it is due, the first time when it comes back.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub at: Option<String>,
    /// For a reminder: `day`, `week`, `month` or `year`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub every: Option<String>,
    /// For a reminder: the day it was last done.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub done: Option<String>,
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
    /// For an event: what kind it is (`meal`, `trip`…), when someone said.
    #[serde(rename = "kind", skip_serializing_if = "Option::is_none")]
    pub event_kind: Option<String>,
    /// For an event: who sent the invitation, when it is not the person; its title,
    /// place and notes were written by them. `someone else` when this AI may not see
    /// who (decision 0014).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub invited_by: Option<String>,
    /// Where its content came from, each once: `person` (typed by the person),
    /// `proposal` (proposed by an AI, accepted by the person), or the name of the
    /// source it was imported from (`contacts`, `calendar`…), whose text was written
    /// outside Ipse (decision 0014). A file without sources is the person's.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub origin: Vec<String>,
}

/// Where the content of a file came from, from its `sources`: each kind once, in
/// order. A file without sources was written by the person (at `ipse init`, or in an
/// editor).
fn origin(sources: &[String]) -> Vec<String> {
    if sources.is_empty() {
        return vec!["person".to_owned()];
    }
    let mut out: Vec<String> = Vec::with_capacity(sources.len());
    for source in sources {
        let kind = source
            .split_once(':')
            .map_or(source.as_str(), |(kind, _)| kind);
        if !kind.is_empty() && !out.iter().any(|k| k == kind) {
            out.push(kind.to_owned());
        }
    }
    out
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

/// A reminder that is due, overdue or due in the next 24 hours (issue #206).
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ReminderOut {
    /// Its file's id: `propose` a `set_key` of `done` on it once the person did it.
    pub id: String,
    /// What to remind the person of.
    pub name: String,
    /// When this occurrence is: a day, or a moment with its offset.
    pub at: String,
    /// True when that moment has passed and it is not done.
    pub overdue: bool,
    /// How often it comes back: `day`, `week`, `month` or `year`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub every: Option<String>,
    /// The file's path, to `propose` changes on it.
    pub file: String,
}

/// A relation as an AI receives it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct RelationOut {
    /// The relation word: `mère`, `works at`.
    pub word: String,
    /// `[[target]]`.
    pub target: String,
    /// The word's kind in the fixed vocabulary (`parent`, `works_at`…); left out for a
    /// free relation.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub kind: Option<&'static str>,
}

impl<'g> Access<'g> {
    /// The connected AI these reads are for.
    #[must_use]
    pub fn client(&self) -> &'g Client {
        self.client
    }

    /// The categories of what these reads returned so far.
    #[must_use]
    pub fn categories_read(&self) -> BTreeSet<String> {
        self.read.borrow().clone()
    }

    /// Logs this request in the access log: the tool, the categories of what it
    /// returned and how many results. Never its arguments or content.
    ///
    /// # Errors
    ///
    /// [`Error::Io`] when the log cannot be written.
    pub fn log(&self, tool: &str, results: usize) -> Result<()> {
        let now = Timestamp::now().to_zoned(self.time_zone().clone());
        self.gateway
            .log
            .record(&now, &self.client.id, tool, &self.read.borrow(), results)
    }

    /// Entities matching `query`, best first, among the categories this AI may read.
    /// `limit` is clamped to 1–50.
    #[must_use]
    pub fn search(&self, query: &str, limit: usize) -> Results {
        let results = self.gateway.index.search_with(
            query,
            limit,
            |c| self.reads(c.as_str()),
            |text| self.hides(text).then(|| self.strip(text)),
        );
        for hit in &results.hits {
            self.returned(hit.category.as_str());
        }
        results
    }

    /// True when this AI may read `category`.
    pub(super) fn reads(&self, category: &str) -> bool {
        self.client.reads(category, &self.gateway.categories)
    }

    /// Remembers that something of `category` was returned.
    pub(super) fn returned(&self, category: &str) {
        let category = self.gateway.categories.effective(category);
        let mut read = self.read.borrow_mut();
        if !read.contains(category) {
            read.insert(category.to_owned());
        }
    }

    /// The entity with this id, if this AI may read it.
    #[must_use]
    pub fn get(&self, id: &str) -> Option<Entry> {
        let id = Id::parse(id)?;
        let path = self
            .gateway
            .index
            .path_of(&id, |c| self.reads(c.as_str()))?;
        self.entry(path)
    }

    /// The entities named `name` (name or alias, ignoring case and accents) this AI may
    /// read.
    #[must_use]
    pub fn get_by_name(&self, name: &str) -> Vec<Entry> {
        self.gateway
            .index
            .named(name, |c| self.reads(c.as_str()))
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
        let overlaps = |e: &EventRef<'_>| overlaps(e, tz, from, to);
        // With someone, or at a place.
        let with_them = |e: &EventRef<'_>| {
            with.as_deref().is_none_or(|stem| {
                e.participants.iter().any(|p| p.eq_ignore_ascii_case(stem))
                    || e.place.is_some_and(|p| p.eq_ignore_ascii_case(stem))
            })
        };
        let mut events = Vec::with_capacity(limit);
        let mut more = false;
        let found = self
            .gateway
            .index
            .events(|c| self.reads(c.as_str()), |e| overlaps(e) && with_them(e));
        for event in found {
            if events.len() == limit {
                more = true;
                break;
            }
            events.extend(self.entry(event.path));
        }
        Events { events, more }
    }

    /// The `n` people this AI may read seen most in its events that overlap
    /// `from`–`to` (days count whole), as the person's timeline counts them: work
    /// meetings and crowds left out, ties to the most varied (issue #264, see
    /// [`crate::search::COMPANY_RULE`]). Each with how many events.
    #[must_use]
    pub fn seen_most(
        &self,
        from: &EventTime,
        to: &EventTime,
        n: usize,
    ) -> Vec<(super::LinkOut, u32)> {
        let tz = self.time_zone();
        let (Some(from), Some(to)) = (instant(from, tz, false), instant(to, tz, true)) else {
            return Vec::new();
        };
        let index = &self.gateway.index;
        let reads = |c: &crate::vault::Category| self.reads(c.as_str());
        let left_out = index.left_out_of_company(reads);
        index
            .seen_most(
                reads,
                |stem| index.person_path(stem, reads).is_some(),
                &left_out,
                |e| overlaps(e, tz, from, to),
                n,
            )
            .into_iter()
            .map(|(stem, events)| {
                let link = LinkOrText::Link(crate::vault::WikiLink {
                    target: stem,
                    display: None,
                });
                (self.link(&link), events)
            })
            .collect()
    }

    /// The series of repeating events this client may read, by event id (see
    /// [`SearchIndex::series`]). Only occurrences it may read are counted.
    #[must_use]
    pub fn series(&self) -> HashMap<String, crate::search::Series> {
        self.gateway
            .index
            .series(|c| self.reads(c.as_str()))
            .into_iter()
            .map(|(id, series)| (id.to_string(), series))
            .collect()
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
            for (row, category) in self.rows(&topic, &years) {
                if Status::of(&row.last_seen, &now, window) == Status::Current {
                    states.push((self.state_out(&topic, &row, None), category));
                }
            }
        }
        self.cap(states, MAX_STATES)
    }

    /// The reminders this AI may read that are due now, overdue, or due in the next 24
    /// hours, the earliest first (issue #206). At most [`MAX_STATES`].
    #[must_use]
    pub fn reminders(&self) -> Vec<ReminderOut> {
        let tz = self.time_zone();
        let now = Timestamp::now();
        let dir = self.gateway.vault.root().join(FileType::Reminder.folder());
        let mut paths: Vec<PathBuf> = std::fs::read_dir(dir)
            .into_iter()
            .flatten()
            .flatten()
            .map(|e| e.path())
            .filter(|p| p.extension().is_some_and(|x| x == "md"))
            .collect();
        paths.sort();
        let mut found: Vec<(Timestamp, ReminderOut, String)> = Vec::new();
        for path in paths {
            let Ok(file) = MemoryFile::read(&path) else {
                continue;
            };
            let common = file.common();
            if common.file_type != FileType::Reminder || !self.reads(common.category.as_str()) {
                continue;
            }
            let Some(fields) = file.entity() else {
                continue;
            };
            let Some(due) = crate::reminders::due(fields, now, tz, crate::reminders::AHEAD) else {
                continue;
            };
            let Some(when) = instant(&due.at, tz, false) else {
                continue;
            };
            found.push((
                when,
                ReminderOut {
                    id: common.id.to_string(),
                    name: fields.name.clone().unwrap_or_default(),
                    at: self.time_text(&due.at),
                    overdue: due.overdue,
                    every: fields.every.map(|e| e.as_str().to_owned()),
                    file: format!(
                        "{}/{}",
                        FileType::Reminder.folder(),
                        path.file_name().unwrap_or_default().to_string_lossy()
                    ),
                },
                common.category.to_string(),
            ));
        }
        found.sort_by_key(|(when, _, _)| *when);
        found.truncate(MAX_STATES);
        for (_, _, category) in &found {
            self.returned(category);
        }
        found.into_iter().map(|(_, r, _)| r).collect()
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
        for (row, category) in self.rows(&topic, &years) {
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
                states.push((self.state_out(&topic, &row, Some(label)), category));
            }
        }
        self.cap(states, limit.clamp(1, MAX_STATES))
    }

    /// At most `limit` states, the categories of those kept remembered.
    fn cap(&self, mut states: Vec<(StateOut, String)>, limit: usize) -> States {
        let more = states.len() > limit;
        states.truncate(limit);
        for (_, category) in &states {
            self.returned(category);
        }
        States {
            states: states.into_iter().map(|(s, _)| s).collect(),
            more,
        }
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
            .path_of(&id, |c| self.reads(c.as_str()))?;
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

    /// The rows of `topic` for `years` that this AI may read, with the category of
    /// their file: files of its categories, rows whose subject it may follow.
    /// Unreadable files are skipped.
    fn rows(&self, topic: &Topic, years: &[i16]) -> Vec<(crate::vault::StateRow, String)> {
        let mut rows = Vec::new();
        for &year in years {
            let path = crate::vault::naming::state_log_path(self.gateway.vault.root(), topic, year);
            let Ok(file) = MemoryFile::read(&path) else {
                continue;
            };
            let category = file.common().category.as_str();
            if !self.reads(category) || file.state_log().is_none() {
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
                    .map(|r| (r, category.to_owned())),
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
    pub(super) fn time_text(&self, time: &EventTime) -> String {
        match time {
            EventTime::Day(day) => day.to_string(),
            EventTime::At(at) => DateTime::at(at.timestamp(), self.time_zone()).to_string(),
        }
    }

    /// A link this AI may follow, or plain text; `None` for a hidden link.
    pub(super) fn visible(&self, value: &LinkOrText) -> Option<String> {
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
        if !self.reads(common.category.as_str()) {
            return None;
        }
        self.returned(common.category.as_str());
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
                event_kind: event.kind.map(|k| k.as_str().to_owned()),
                invited_by: event
                    .organizer
                    .as_ref()
                    .map(|o| self.visible(o).unwrap_or_else(|| "someone else".to_owned())),
                origin: origin(&common.sources),
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
            at: entity.at.as_ref().map(|t| self.time_text(t)),
            every: entity.every.map(|e| e.as_str().to_owned()),
            done: entity.done.map(|d| d.to_string()),
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
                    kind: r.kind().map(crate::vault::RelationKind::as_str),
                })
                .collect(),
            notes: sections
                .notes
                .as_deref()
                .map(|n| self.strip(n))
                .filter(|n| !n.trim().is_empty()),
            origin: origin(&common.sources),
            ..Entry::default()
        })
    }

    /// `text` without the lines that link to anything this AI may not read. A fact is
    /// one line: it is left out entirely.
    pub(super) fn strip(&self, text: &str) -> String {
        let mut out = String::with_capacity(text.len());
        for line in text.split_inclusive('\n') {
            if !self.hides(line) {
                out.push_str(line);
            }
        }
        out.trim_end_matches('\n').to_owned()
    }

    /// True when `text` holds a `[[link]]` this AI may not follow.
    pub(super) fn hides(&self, text: &str) -> bool {
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
        !categories.is_empty() && categories.iter().all(|c| self.reads(c.as_str()))
    }
}

fn type_name(file_type: FileType) -> String {
    file_type.as_str().to_owned()
}

/// The instant an event time stands for: a moment as is; a day at its start in `tz`,
/// or at the start of the next day when it ends a period (`end_of_day`).
/// True when the event `e` overlaps `from`–`to`: an event without an end lasts its day
/// when it is a day, an instant otherwise.
fn overlaps(e: &EventRef<'_>, tz: &TimeZone, from: Timestamp, to: Timestamp) -> bool {
    let Some(start) = instant(e.start, tz, false) else {
        return false;
    };
    let end = match e.end {
        Some(end) => instant(end, tz, false).unwrap_or(start),
        None if e.start.is_day() => instant(e.start, tz, true).unwrap_or(start),
        None => start,
    };
    start < to && (end > from || (end == start && start >= from))
}

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

/// When the settings file of the vault in `root` was last changed.
fn settings_changed(root: &Path) -> Option<std::time::SystemTime> {
    std::fs::metadata(root.join(crate::vault::SETTINGS_FILE))
        .and_then(|m| m.modified())
        .ok()
}
