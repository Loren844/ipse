//! Proposals: changes to memory files that wait for the person (`docs/spec/vault.md`,
//! section 12; decision 0005).
//!
//! Anything an AI infers, merges or changes is a proposal: a set of [`Change`]s kept in
//! `.ipse/proposals/<id>.json` until the person accepts it, refuses it, or undoes it
//! after accepting. Accepting applies every change in one change of the history,
//! `Ipse-Source: proposal:<id>`; a change that no longer applies to the files as they
//! are now makes the whole proposal stale, and nothing is written.

mod apply;
mod change;
mod key;

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

pub use apply::{DiffItem, FileDiff, Preview, What};
pub use change::{Change, NewRelation};
pub(crate) use key::get as key_value;

use crate::tasks::Tasks;
use crate::vault::{DateTime, Id, Source, Vault, write_atomic};
use crate::{Error, Result};

/// Version of the proposal format.
pub const FORMAT: u32 = 1;

/// Where proposals are kept, relative to the vault.
pub const FOLDER: &str = ".ipse/proposals";

/// One proposal, as saved.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Proposal {
    /// Version of the format.
    pub format: u32,
    /// Its identifier, time-ordered.
    pub id: String,
    /// Who made it.
    pub by: Author,
    /// When it was made, in the vault's time zone.
    pub created: String,
    /// Why, in plain words, as its author wrote it.
    pub reason: String,
    /// The task it answers, if any.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub task: Option<String>,
    /// The categories of the files it touches, for rights.
    pub categories: Vec<String>,
    /// What it changes, in order.
    pub changes: Vec<Change>,
    /// Where it stands.
    pub status: Status,
}

/// Who made a proposal.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(try_from = "String", into = "String")]
pub enum Author {
    /// A connected AI, by its client id: `client:claude`.
    Client(String),
    /// A connector instance, for a change the person had made too: `connector:contacts`.
    Connector(String),
    /// Ipse itself: `ipse`.
    Ipse,
}

impl TryFrom<String> for Author {
    type Error = String;

    fn try_from(text: String) -> std::result::Result<Self, String> {
        match text.split_once(':') {
            None if text == "ipse" => Ok(Self::Ipse),
            Some(("client", id)) if !id.is_empty() => Ok(Self::Client(id.to_owned())),
            Some(("connector", id)) if !id.is_empty() => Ok(Self::Connector(id.to_owned())),
            _ => Err("`by` must be `client:<id>`, `connector:<instance>` or `ipse`".to_owned()),
        }
    }
}

impl From<Author> for String {
    fn from(author: Author) -> Self {
        match author {
            Author::Client(id) => format!("client:{id}"),
            Author::Connector(id) => format!("connector:{id}"),
            Author::Ipse => "ipse".to_owned(),
        }
    }
}

/// Where a proposal stands.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "state", rename_all = "snake_case")]
pub enum Status {
    /// Waiting for the person.
    Waiting,
    /// Accepted and applied.
    Accepted {
        /// When.
        at: String,
        /// The change of the history it made; `None` when the files already said so.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        change: Option<String>,
    },
    /// Refused by the person.
    Refused {
        /// When.
        at: String,
        /// What the person said, for the AI to try again.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        comment: Option<String>,
    },
    /// Accepted, then undone by the person.
    Undone {
        /// When.
        at: String,
        /// The change of the history that undid it.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        change: Option<String>,
    },
    /// No longer applies: its files changed since it was made.
    Stale {
        /// When it was found stale.
        at: String,
        /// Why, naming the file but not its content.
        reason: String,
    },
}

impl Status {
    /// `waiting`, `accepted`, `refused`, `undone` or `stale`.
    #[must_use]
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Waiting => "waiting",
            Self::Accepted { .. } => "accepted",
            Self::Refused { .. } => "refused",
            Self::Undone { .. } => "undone",
            Self::Stale { .. } => "stale",
        }
    }
}

/// What accepting a proposal did.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Accepted {
    /// Applied; with the history change it made, if the files changed.
    Applied(Option<String>),
    /// Not applied: a change no longer fits the files. The proposal is now stale.
    Stale(String),
}

/// The proposals of one vault.
#[derive(Debug)]
pub struct Proposals<'v> {
    vault: &'v Vault,
    dir: PathBuf,
}

impl<'v> Proposals<'v> {
    /// The proposals of `vault`.
    #[must_use]
    pub fn new(vault: &'v Vault) -> Self {
        Self {
            vault,
            dir: vault.root().join(FOLDER),
        }
    }

    /// Checks `changes` against the vault and saves them as a new waiting proposal.
    /// A file to create gets its path now.
    ///
    /// # Errors
    ///
    /// [`Error::Refused`] with a plain-language message when a change is not valid (an
    /// unknown key, a file that does not exist, a value of the wrong form);
    /// [`Error::Io`] when the proposal cannot be saved.
    pub fn submit(
        &self,
        by: Author,
        reason: &str,
        task: Option<String>,
        changes: Vec<Change>,
    ) -> Result<Proposal> {
        let task = match task {
            Some(id) => Some(Tasks::new(self.vault).check_open(&id)?),
            None => None,
        };
        let proposal = self.prepare(by, reason, task, changes)?;
        self.save(&proposal)?;
        if let Some(task) = &proposal.task {
            Tasks::new(self.vault).proposed(task, &proposal.id)?;
        }
        Ok(proposal)
    }

    /// Records what the source `instance` changed after the person, as a proposal by
    /// that source: `changes` set keys of one file. A key it already proposed with this
    /// value and the person refused is left out ("keep mine"); a waiting proposal of
    /// this source for the same file takes the new changes instead of a second
    /// proposal. Returns whether a proposal waits for the person.
    ///
    /// `known` is this source's proposals as [`Proposals::list`] gave them, read once
    /// for a whole import; the proposal saved is added to it.
    pub(crate) fn propose_for_source(
        &self,
        instance: &str,
        reason: &str,
        changes: Vec<Change>,
        known: &mut Vec<Proposal>,
    ) -> Result<bool> {
        let by = Author::Connector(instance.to_owned());
        let Some(file) = changes.first().and_then(Change::file).map(str::to_owned) else {
            return Ok(false);
        };
        let mine: Vec<&Proposal> = known
            .iter()
            .filter(|p| p.by == by && p.changes.iter().all(|c| c.file() == Some(file.as_str())))
            .collect();
        let refused = |change: &Change| {
            mine.iter()
                .filter(|p| matches!(p.status, Status::Refused { .. }))
                .any(|p| p.changes.iter().any(|c| same_key_value(c, change)))
        };
        let new: Vec<Change> = changes.into_iter().filter(|c| !refused(c)).collect();
        let waiting = mine
            .into_iter()
            .find(|p| p.status == Status::Waiting)
            .cloned();
        if new.is_empty() {
            return Ok(waiting.is_some());
        }
        // Keys the source changed before and not now stay proposed, while they still
        // apply; otherwise only the new ones are.
        let mut tries = Vec::with_capacity(2);
        if let Some(old) = &waiting {
            let mut all = new.clone();
            all.extend(
                old.changes
                    .iter()
                    .filter(|c| !new.iter().any(|n| same_key(n, c)))
                    .cloned(),
            );
            if all.len() > new.len() {
                tries.push(all);
            }
        }
        tries.push(new);
        let mut prepared = None;
        for changes in tries {
            match self.prepare(by.clone(), reason, None, changes) {
                Ok(proposal) => {
                    prepared = Some(proposal);
                    break;
                }
                // The person's file already says what the source says, or changed
                // again since.
                Err(Error::Refused(_)) => {}
                Err(e) => return Err(e),
            }
        }
        let Some(mut proposal) = prepared else {
            return Ok(waiting.is_some());
        };
        if let Some(old) = waiting {
            proposal.id = old.id;
            proposal.created = old.created;
        }
        self.save(&proposal)?;
        known.retain(|p| p.id != proposal.id);
        known.push(proposal);
        Ok(true)
    }

    /// A checked proposal, not saved.
    fn prepare(
        &self,
        by: Author,
        reason: &str,
        task: Option<String>,
        mut changes: Vec<Change>,
    ) -> Result<Proposal> {
        if reason.trim().is_empty() {
            return Err(Error::Refused(
                "Say why, in a sentence the person will read.".to_owned(),
            ));
        }
        if changes.is_empty() {
            return Err(Error::Refused(
                "A proposal needs at least one change.".to_owned(),
            ));
        }
        let mut categories = Vec::new();
        let mut planned = Vec::new();
        for change in &mut changes {
            let category = change::check(self.vault, change, &planned)?;
            if let Change::Create {
                file: Some(file), ..
            } = change
            {
                planned.push(file.clone());
            }
            if !categories.contains(&category) {
                categories.push(category);
            }
        }
        categories.sort();
        let proposal = Proposal {
            format: FORMAT,
            id: Id::generate().to_string(),
            by,
            created: self.now(),
            reason: reason.trim().to_owned(),
            task,
            categories,
            changes,
            status: Status::Waiting,
        };
        let plan = apply::plan(self.vault, &proposal)?;
        if let Some(reason) = plan.preview.stale {
            return Err(Error::Refused(format!(
                "{reason} Read the file again and make the proposal from what it says now."
            )));
        }
        if plan.preview.files.is_empty() {
            return Err(Error::Refused(
                "This proposal changes nothing: the files already say so.".to_owned(),
            ));
        }
        Ok(proposal)
    }

    /// Every proposal, newest first. Files that cannot be read are skipped.
    ///
    /// # Errors
    ///
    /// [`Error::Io`] when the folder cannot be read.
    pub fn list(&self) -> Result<Vec<Proposal>> {
        let entries = match std::fs::read_dir(&self.dir) {
            Ok(entries) => entries,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
            Err(source) => {
                return Err(Error::Io {
                    action: "read",
                    path: self.dir.clone(),
                    source,
                });
            }
        };
        let mut all: Vec<Proposal> = entries
            .flatten()
            .map(|e| e.path())
            .filter(|p| p.extension().is_some_and(|e| e == "json"))
            .filter_map(|p| read(&p).ok())
            .collect();
        all.sort_by(|a, b| b.id.cmp(&a.id));
        Ok(all)
    }

    /// The proposal `id`, or the only one whose id starts with it (at least 4
    /// characters).
    ///
    /// # Errors
    ///
    /// [`Error::Refused`] when there is none, or several; [`Error::Invalid`] or
    /// [`Error::UnsupportedVersion`] when its file cannot be read.
    pub fn get(&self, id: &str) -> Result<Proposal> {
        let id = id.trim().to_ascii_lowercase();
        let unknown = || Error::Refused(format!("There is no proposal {id}."));
        if id.is_empty() || !id.chars().all(|c| c.is_ascii_alphanumeric()) {
            return Err(unknown());
        }
        let exact = self.path(&id);
        if exact.exists() {
            return read(&exact);
        }
        if id.len() < 4 {
            return Err(unknown());
        }
        let mut found: Vec<Proposal> = self
            .list()?
            .into_iter()
            .filter(|p| p.id.starts_with(&id))
            .collect();
        match found.len() {
            1 => Ok(found.remove(0)),
            0 => Err(unknown()),
            _ => Err(Error::Refused(format!(
                "Several proposals start with {id}: give more of the id."
            ))),
        }
    }

    /// What `proposal` says it changes, as it was written, file by file: for one
    /// already decided, whose changes the files now hold or never did. Reads nothing
    /// but the names of its files.
    #[must_use]
    pub fn described(&self, proposal: &Proposal) -> Preview {
        apply::described(self.vault, proposal)
    }

    /// What `proposal` would change in the files as they are now, file by file, and
    /// why it no longer applies if it does not.
    ///
    /// # Errors
    ///
    /// [`Error::Io`] when a file cannot be read.
    pub fn preview(&self, proposal: &Proposal) -> Result<Preview> {
        Ok(apply::plan(self.vault, proposal)?.preview)
    }

    /// Applies the waiting proposal `id` in one change of the history, after recording
    /// the person's own edits; or marks it stale, writing nothing, when a change no
    /// longer applies.
    ///
    /// # Errors
    ///
    /// [`Error::Refused`] when the proposal is not waiting; [`Error::Io`],
    /// [`Error::Invalid`] or [`Error::History`] when the files or the history cannot be
    /// written.
    pub fn accept(&self, id: &str) -> Result<Accepted> {
        let mut proposal = self.get(id)?;
        if proposal.status != Status::Waiting {
            return Err(not_waiting(&proposal));
        }
        let history = self.vault.history()?;
        let plan = apply::plan(self.vault, &proposal)?;
        if let Some(reason) = plan.preview.stale {
            proposal.status = Status::Stale {
                at: self.now(),
                reason: reason.clone(),
            };
            self.save(&proposal)?;
            if let Some(task) = &proposal.task {
                Tasks::new(self.vault).reopen(
                    task,
                    Some(format!("The last proposal no longer applied: {reason}")),
                )?;
            }
            return Ok(Accepted::Stale(reason));
        }
        let mut change = history.change(Source::Proposal(proposal.id.clone()));
        apply::write(self.vault, plan.files, &mut change)?;
        let entry = change.commit("Accept a proposal")?;
        let change = entry.map(|e| e.id);
        proposal.status = Status::Accepted {
            at: self.now(),
            change: change.clone(),
        };
        self.save(&proposal)?;
        if let Some(task) = &proposal.task {
            Tasks::new(self.vault).done(task, &proposal.id)?;
        }
        Ok(Accepted::Applied(change))
    }

    /// Refuses the proposal `id`, waiting or stale, with what the person said.
    ///
    /// # Errors
    ///
    /// [`Error::Refused`] when it was already decided; [`Error::Io`] when it cannot be
    /// saved.
    pub fn refuse(&self, id: &str, comment: Option<&str>) -> Result<Proposal> {
        let mut proposal = self.get(id)?;
        if !matches!(proposal.status, Status::Waiting | Status::Stale { .. }) {
            return Err(not_waiting(&proposal));
        }
        let comment = comment
            .map(str::trim)
            .filter(|c| !c.is_empty())
            .map(str::to_owned);
        proposal.status = Status::Refused {
            at: self.now(),
            comment: comment.clone(),
        };
        self.save(&proposal)?;
        if let Some(task) = &proposal.task {
            Tasks::new(self.vault).reopen(task, comment)?;
        }
        Ok(proposal)
    }

    /// Undoes the accepted proposal `id` as a new change of the history by the person.
    ///
    /// # Errors
    ///
    /// [`Error::Refused`] when it is not accepted, or when one of its files changed
    /// since (undoing would lose that later change); [`Error::Io`] or
    /// [`Error::History`] as for [`crate::vault::History::undo`].
    pub fn undo(&self, id: &str) -> Result<Proposal> {
        let mut proposal = self.get(id)?;
        let Status::Accepted { change, .. } = &proposal.status else {
            return Err(Error::Refused(format!(
                "Only an accepted proposal can be undone; this one is {}.",
                proposal.status.as_str()
            )));
        };
        let undone = match change {
            Some(change) => {
                let history = self.vault.history()?;
                history
                    .undo(change, Source::Person, "Undo a proposal")?
                    .map(|e| e.id)
            }
            None => None,
        };
        proposal.status = Status::Undone {
            at: self.now(),
            change: undone,
        };
        self.save(&proposal)?;
        if let Some(task) = &proposal.task {
            Tasks::new(self.vault).reopen(
                task,
                Some("The person undid the proposal they had accepted.".to_owned()),
            )?;
        }
        Ok(proposal)
    }

    fn path(&self, id: &str) -> PathBuf {
        self.dir.join(format!("{id}.json"))
    }

    fn save(&self, proposal: &Proposal) -> Result<()> {
        let mut text = serde_json::to_string_pretty(proposal).map_err(|_| Error::Invalid {
            path: self.path(&proposal.id),
            line: None,
            problem: "the proposal could not be written as JSON".to_owned(),
        })?;
        text.push('\n');
        write_atomic(&self.path(&proposal.id), text.as_bytes())
    }

    fn now(&self) -> String {
        DateTime::now(self.vault.settings().time_zone()).to_string()
    }
}

/// True when `a` and `b` set the same key of the same file.
fn same_key(a: &Change, b: &Change) -> bool {
    match (a, b) {
        (
            Change::SetKey { file, key, .. },
            Change::SetKey {
                file: other_file,
                key: other_key,
                ..
            },
        ) => file == other_file && key == other_key,
        _ => false,
    }
}

/// True when `a` and `b` set the same key of the same file to the same value.
fn same_key_value(a: &Change, b: &Change) -> bool {
    match (a, b) {
        (Change::SetKey { to, .. }, Change::SetKey { to: other_to, .. }) => {
            same_key(a, b) && to == other_to
        }
        _ => false,
    }
}

fn not_waiting(proposal: &Proposal) -> Error {
    Error::Refused(format!(
        "This proposal is already {}.",
        proposal.status.as_str()
    ))
}

/// Just enough of a proposal to read its format version.
#[derive(Deserialize)]
struct Head {
    format: u32,
}

/// Reads one proposal file.
fn read(path: &Path) -> Result<Proposal> {
    let text = std::fs::read_to_string(path).map_err(|source| Error::Io {
        action: "read",
        path: path.to_owned(),
        source,
    })?;
    let invalid = |problem: String| Error::Invalid {
        path: path.to_owned(),
        line: None,
        problem,
    };
    let head: Head = serde_json::from_str(&text)
        .map_err(|_| invalid("this is not a proposal: `format` is missing".to_owned()))?;
    if head.format > FORMAT {
        return Err(Error::UnsupportedVersion {
            what: "this proposal",
            found: head.format,
            supported: FORMAT,
        });
    }
    serde_json::from_str(&text).map_err(|e| {
        invalid(format!(
            "the proposal does not follow its format (line {}, column {})",
            e.line(),
            e.column()
        ))
    })
}
