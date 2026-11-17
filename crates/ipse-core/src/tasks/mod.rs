//! Processing tasks: the work that needs a language model, left for the person's own AI
//! (`docs/spec/vault.md`, section 13; decision 0010).
//!
//! A task is a small piece of work any AI can take: read these notes and propose facts,
//! say whether these two people are the same, ask the person who someone is, summarize
//! a day. The AI answers with a proposal ([`crate::proposals`]); the task is `proposed`
//! while it waits, `done` once the person accepts it, and open again, with the person's
//! comment, when they refuse it.

pub mod classify;
mod extract;
pub mod question;
mod summary;

pub use summary::WINDOW_DAYS;

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::vault::{DateTime, Id, Vault, write_atomic};
use crate::{Error, Result};

/// Version of the task format.
pub const FORMAT: u32 = 1;

/// Where tasks are kept, relative to the vault.
pub const FOLDER: &str = ".ipse/tasks";

/// Items in one task at most.
pub const MAX_ITEMS: usize = 100;

/// One task, as saved.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Task {
    /// Version of the format.
    pub format: u32,
    /// Its identifier, time-ordered.
    pub id: String,
    /// What kind of work it is.
    pub kind: TaskKind,
    /// When it was made, in the vault's time zone.
    pub created: String,
    /// One line saying what it is about, without personal data.
    pub description: String,
    /// The categories of its items, for rights.
    pub categories: Vec<String>,
    /// What it covers.
    pub items: Vec<ItemRef>,
    /// Where it stands.
    pub status: TaskStatus,
    /// What the person said when they refused the last proposal for it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub comment: Option<String>,
    /// For a `summary` task, the period: `day:2026-11-14`, `week:2026-11-09`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub period: Option<String>,
}

/// The kinds of task (`docs/spec/rights.md`, section 4.2).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TaskKind {
    /// New text from a connector to turn into facts and relations.
    Extract,
    /// Possible duplicates to merge or tell apart.
    Merge,
    /// A gap only the person can fill.
    Question,
    /// A period to summarize.
    Summary,
    /// Events to give a kind.
    Classify,
}

impl TaskKind {
    /// Every kind of task.
    pub const ALL: [Self; 5] = [
        Self::Extract,
        Self::Merge,
        Self::Question,
        Self::Summary,
        Self::Classify,
    ];

    /// `extract`, `merge`, `question`, `summary` or `classify`.
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Extract => "extract",
            Self::Merge => "merge",
            Self::Question => "question",
            Self::Summary => "summary",
            Self::Classify => "classify",
        }
    }
}

/// One item a task covers: a file, and the hash of what in it the task is about.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ItemRef {
    /// The file, relative to the vault, with `/`.
    pub file: String,
    /// Its category when the task was made.
    pub category: String,
    /// BLAKE3 of the text the task is about, to tell when it changed.
    pub hash: String,
}

/// Where a task stands.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "state", rename_all = "snake_case")]
pub enum TaskStatus {
    /// Nobody answered it yet, or the last answer was refused.
    Open,
    /// A proposal answering it waits for the person.
    Proposed {
        /// That proposal.
        proposal: String,
    },
    /// The person accepted a proposal answering it, or the AI found nothing worth
    /// keeping.
    Done {
        /// The accepted proposal; `None` when there was nothing to propose.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        proposal: Option<String>,
    },
}

impl TaskStatus {
    /// `open`, `proposed` or `done`.
    #[must_use]
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Open => "open",
            Self::Proposed { .. } => "proposed",
            Self::Done { .. } => "done",
        }
    }
}

/// One item of a task, as given to an AI: the file and the text to work on, read now.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Item {
    /// The file, relative to the vault, with `/`.
    pub file: String,
    /// Its name or title.
    pub name: String,
    /// Its type: `person`, `event`…
    pub file_type: String,
    /// Its category.
    pub category: String,
    /// The text to work on; `None` when the file no longer has it, or when the item is
    /// given as `details`.
    pub text: Option<String>,
    /// The item's fields, each in its own key (an event's title, place, notes…), so
    /// that text from the memory is never mixed with Ipse's words (decision 0014).
    pub details: Option<serde_json::Value>,
}

/// A task in full: what is expected, its items as they are now, an example answer.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TaskDetail {
    /// The task.
    pub task: Task,
    /// What a good answer does, in plain words.
    pub expected: String,
    /// Its items, read now.
    pub items: Vec<Item>,
    /// An example of a good proposal, as JSON.
    pub example: String,
}

/// The tasks of one vault.
#[derive(Debug)]
pub struct Tasks<'v> {
    vault: &'v Vault,
    dir: PathBuf,
}

impl<'v> Tasks<'v> {
    /// The tasks of `vault`.
    #[must_use]
    pub fn new(vault: &'v Vault) -> Self {
        Self {
            vault,
            dir: vault.root().join(FOLDER),
        }
    }

    /// Every task, oldest first. Files that cannot be read are skipped.
    ///
    /// # Errors
    ///
    /// [`Error::Io`] when the folder cannot be read.
    pub fn list(&self) -> Result<Vec<Task>> {
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
        let mut all: Vec<Task> = entries
            .flatten()
            .map(|e| e.path())
            .filter(|p| {
                p.extension().is_some_and(|e| e == "json")
                    && p.file_stem()
                        .is_some_and(|s| Id::parse(&s.to_string_lossy()).is_some())
            })
            .filter_map(|p| read(&p).ok())
            .collect();
        all.sort_by(|a, b| a.id.cmp(&b.id));
        Ok(all)
    }

    /// The task `id`, or the only one whose id starts with it (at least 4 characters).
    ///
    /// # Errors
    ///
    /// [`Error::Refused`] when there is none, or several; [`Error::Invalid`] or
    /// [`Error::UnsupportedVersion`] when its file cannot be read.
    pub fn get(&self, id: &str) -> Result<Task> {
        let id = id.trim().to_ascii_lowercase();
        let unknown = || Error::Refused(format!("There is no task {id}."));
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
        let mut found: Vec<Task> = self
            .list()?
            .into_iter()
            .filter(|t| t.id.starts_with(&id))
            .collect();
        match found.len() {
            1 => Ok(found.remove(0)),
            0 => Err(unknown()),
            _ => Err(Error::Refused(format!(
                "Several tasks start with {id}: give more of the id."
            ))),
        }
    }

    /// The task `id` in full, its items read from the files as they are now.
    ///
    /// # Errors
    ///
    /// As [`Tasks::get`].
    pub fn detail(&self, id: &str) -> Result<TaskDetail> {
        let task = self.get(id)?;
        Ok(match task.kind {
            TaskKind::Extract => extract::detail(self.vault, task),
            TaskKind::Summary => summary::detail(self.vault, task),
            TaskKind::Question => question::detail(self.vault, task),
            TaskKind::Classify => classify::detail(self.vault, task),
            TaskKind::Merge => TaskDetail {
                expected: String::new(),
                items: Vec::new(),
                example: String::new(),
                task,
            },
        })
    }

    /// Makes `extract` tasks for the notes of `files` (absolute paths, from an
    /// import) that are new or changed since a task last covered them, 20 per task.
    /// Returns how many tasks it made.
    ///
    /// # Errors
    ///
    /// [`Error::Io`] when a task cannot be saved.
    pub fn extract_from(&self, files: &[PathBuf]) -> Result<usize> {
        extract::make(self, files)
    }

    /// Makes the `summary` tasks due on `today`: each past day of the last
    /// [`WINDOW_DAYS`] days with events, then each week, month and year once the
    /// summaries inside it exist, each only when what it holds changed since it was
    /// last asked. Returns how many tasks it made.
    ///
    /// # Errors
    ///
    /// [`Error::Io`] when a task cannot be saved.
    pub fn plan_summaries(
        &self,
        index: &crate::search::SearchIndex,
        today: jiff::civil::Date,
    ) -> Result<usize> {
        summary::plan(self, index, today)
    }

    /// Makes the `question` tasks due on `today`: people known only by a first name or
    /// a nickname, each asked about once, at most [`question::OPEN`] waiting and
    /// [`question::PER_DAY`] new a day. Returns how many tasks it made.
    ///
    /// # Errors
    ///
    /// [`Error::Io`] when a task cannot be saved.
    pub fn plan_questions(
        &self,
        index: &crate::search::SearchIndex,
        today: jiff::civil::Date,
    ) -> Result<usize> {
        question::plan(self, index, today)
    }

    /// Makes the `classify` tasks due on `today`: events without a kind, each asked
    /// about once, [`classify::PER_TASK`] a task, one category a task, at most
    /// [`classify::OPEN`] waiting and [`classify::PER_DAY`] new a day. Returns how many
    /// tasks it made.
    ///
    /// # Errors
    ///
    /// [`Error::Io`] when a task cannot be saved.
    pub fn plan_classify(
        &self,
        index: &crate::search::SearchIndex,
        today: jiff::civil::Date,
    ) -> Result<usize> {
        classify::plan(self, index, today)
    }

    /// Notes that `proposal` answers the open task `id`.
    ///
    /// # Errors
    ///
    /// [`Error::Refused`] when there is no such task or it is not open.
    pub(crate) fn proposed(&self, id: &str, proposal: &str) -> Result<()> {
        let mut task = self.get(id)?;
        if task.status != TaskStatus::Open {
            return Err(Error::Refused(format!(
                "Task {} is already {}: pick an open one.",
                task.id,
                task.status.as_str()
            )));
        }
        task.status = TaskStatus::Proposed {
            proposal: proposal.to_owned(),
        };
        self.save(&task)
    }

    /// The task `id` must be open for a new proposal: an error saying why not.
    pub(crate) fn check_open(&self, id: &str) -> Result<String> {
        let task = self.get(id)?;
        if task.status == TaskStatus::Open {
            Ok(task.id)
        } else {
            Err(Error::Refused(format!(
                "Task {} is already {}: pick an open one.",
                task.id,
                task.status.as_str()
            )))
        }
    }

    /// Closes the task `id`: the person accepted `proposal`.
    pub(crate) fn done(&self, id: &str, proposal: &str) -> Result<()> {
        self.update(id, |task| {
            task.status = TaskStatus::Done {
                proposal: Some(proposal.to_owned()),
            };
            task.comment = None;
        })
    }

    /// Closes the open task `id` without a proposal: the AI found nothing worth keeping.
    ///
    /// # Errors
    ///
    /// [`Error::Refused`] when there is no such task or it is not open.
    pub fn dismiss(&self, id: &str) -> Result<()> {
        let id = self.check_open(id)?;
        self.update(&id, |task| {
            task.status = TaskStatus::Done { proposal: None };
            task.comment = None;
        })
    }

    /// Opens the task `id` again, with what the person said or why the proposal could
    /// not be applied.
    pub(crate) fn reopen(&self, id: &str, comment: Option<String>) -> Result<()> {
        self.update(id, |task| {
            task.status = TaskStatus::Open;
            task.comment = comment;
        })
    }

    /// Changes the task `id`, if it still exists.
    fn update(&self, id: &str, change: impl FnOnce(&mut Task)) -> Result<()> {
        match self.get(id) {
            Ok(mut task) => {
                change(&mut task);
                self.save(&task)
            }
            // A task removed by hand: nothing to update.
            Err(Error::Refused(_)) => Ok(()),
            Err(e) => Err(e),
        }
    }

    fn path(&self, id: &str) -> PathBuf {
        self.dir.join(format!("{id}.json"))
    }

    fn save(&self, task: &Task) -> Result<()> {
        save_json(&self.path(&task.id), task)
    }

    fn now(&self) -> String {
        DateTime::now(self.vault.settings().time_zone()).to_string()
    }
}

/// Writes `value` as JSON, atomically.
fn save_json(path: &Path, value: &impl Serialize) -> Result<()> {
    let mut text = serde_json::to_string_pretty(value).map_err(|_| Error::Invalid {
        path: path.to_owned(),
        line: None,
        problem: "could not be written as JSON".to_owned(),
    })?;
    text.push('\n');
    write_atomic(path, text.as_bytes())
}

/// Just enough of a task to read its format version.
#[derive(Deserialize)]
struct Head {
    format: u32,
}

/// Reads one task file.
fn read(path: &Path) -> Result<Task> {
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
        .map_err(|_| invalid("this is not a task: `format` is missing".to_owned()))?;
    if head.format > FORMAT {
        return Err(Error::UnsupportedVersion {
            what: "this task",
            found: head.format,
            supported: FORMAT,
        });
    }
    serde_json::from_str(&text).map_err(|e| {
        invalid(format!(
            "the task does not follow its format (line {}, column {})",
            e.line(),
            e.column()
        ))
    })
}
