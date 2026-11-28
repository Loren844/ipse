//! The processing tools of the query layer (`docs/spec/rights.md`, section 4.2): tasks
//! for the AI, and the proposals it submits.
//!
//! A client works in the categories it may both read and propose in. A task that
//! touches any other category is invisible to it, and a change to a file of another
//! category is refused with the same words as a change to a file that does not exist:
//! nothing hints at what it may not see.

use serde::Serialize;
use serde_json::Value;

use super::Access;
use crate::proposals::{Accepted, Author, Change, Proposals, Status};
use crate::summaries;
use crate::tasks::{Task, TaskKind, TaskStatus, Tasks};
use crate::vault::{Category, FileType, MemoryFile};
use crate::{Error, Result};

/// Most tasks `tasks` lists at once.
pub const MAX_TASKS: usize = 50;

/// Most changes in one proposal.
pub const MAX_CHANGES: usize = 100;

/// The longest reason, in characters.
const MAX_REASON: usize = 1_000;

/// The longest notes of a new file, in characters.
const MAX_NOTES: usize = 10_000;

/// The largest proposal, as JSON.
const MAX_BYTES: usize = 256 * 1024;

/// One task, as `tasks` lists it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct TaskLine {
    /// Its id, for `task`.
    pub id: String,
    /// `extract`, `merge`, `question` or `summary`.
    pub kind: &'static str,
    /// What it is about, in one line.
    pub description: String,
    /// How many items it covers.
    pub items: usize,
    /// What the person said about the last answer, when they refused it.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub comment: Option<String>,
}

/// One task in full, as `task` returns it.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct TaskOut {
    /// Its id.
    pub id: String,
    /// Its kind.
    pub kind: &'static str,
    /// `open`, `proposed` or `done`.
    pub status: &'static str,
    /// What it is about.
    pub description: String,
    /// What the person said about the last answer, when they refused it.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub comment: Option<String>,
    /// What a good answer does.
    pub expected: String,
    /// The items, read now: only files this client may read.
    pub items: Vec<ItemOut>,
    /// An example of a good `propose` call.
    pub example: Value,
}

/// One item of a task.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ItemOut {
    /// The file, as `propose` takes it.
    pub file: String,
    /// Its name or title.
    pub name: String,
    /// Its type.
    #[serde(rename = "type")]
    pub file_type: String,
    /// The text to work on; `null` when the file no longer has it, or when the item is
    /// given as `details`.
    pub text: Option<String>,
    /// The item's fields, each in its own key; left out when the item is a text.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub details: Option<Value>,
}

/// What `propose` did.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Proposed {
    /// The new proposal's id; `None` when the AI said there was nothing to propose.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub proposal: Option<String>,
    /// `waiting` for a proposal, `accepted` for one the person accepts in advance from
    /// this AI (decision 0017), `done` for a task closed with nothing to propose.
    pub status: &'static str,
}

/// A proposal, as `proposal` returns it to the client that made it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ProposalOut {
    /// Its id.
    pub id: String,
    /// `waiting`, `accepted`, `refused`, `undone` or `stale`.
    pub status: &'static str,
    /// The reason the client gave.
    pub reason: String,
    /// The task it answers.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub task: Option<String>,
    /// What the person said, when they refused it.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub comment: Option<String>,
    /// Why it no longer applies, when stale.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub stale: Option<String>,
}

fn no_task(id: &str) -> Error {
    Error::Refused(format!(
        "There is no task {}.",
        id.trim().to_ascii_lowercase()
    ))
}

fn no_proposal(id: &str) -> Error {
    Error::Refused(format!(
        "There is no proposal {}.",
        id.trim().to_ascii_lowercase()
    ))
}

impl Access<'_> {
    /// True when this AI may propose changes in `category`: it must also read it.
    /// Unknown categories count as `private`.
    #[must_use]
    pub fn proposes(&self, category: &str) -> bool {
        let effective = self.gateway.categories.effective(category);
        self.reads(category) && self.client.propose.iter().any(|c| c == effective)
    }

    /// The owner's file, relative to the vault with `/`, when this AI may propose
    /// changes to it.
    fn owner_file_if_proposes(&self) -> Option<String> {
        let vault = &self.gateway.vault;
        let owner = MemoryFile::read(vault.owner_file()).ok()?;
        if !self.proposes(owner.common().category.as_str()) {
            return None;
        }
        let relative = vault.owner_file().strip_prefix(vault.root()).ok()?;
        Some(
            relative
                .components()
                .map(|c| c.as_os_str().to_string_lossy())
                .collect::<Vec<_>>()
                .join("/"),
        )
    }

    /// A task is for an AI that may propose in all its categories; a summary task, for
    /// one that may propose summaries and read the rest.
    fn sees(&self, task: &Task) -> bool {
        if task.categories.is_empty() {
            return false;
        }
        if task.kind == TaskKind::Summary {
            return self.proposes("summaries") && task.categories.iter().all(|c| self.reads(c));
        }
        task.categories.iter().all(|c| self.proposes(c))
    }

    /// The open tasks this AI may work on, oldest first, of `kind` if given; with
    /// whether there were more than `limit` (clamped to 1–50).
    ///
    /// # Errors
    ///
    /// [`Error::Refused`] for an unknown kind; [`Error::Io`] when the tasks cannot be
    /// read.
    pub fn tasks(&self, kind: Option<&str>, limit: usize) -> Result<(Vec<TaskLine>, bool)> {
        let kind = match kind.map(str::trim).filter(|k| !k.is_empty()) {
            None => None,
            Some(k) => Some(
                TaskKind::ALL
                    .into_iter()
                    .find(|t| t.as_str() == k)
                    .ok_or_else(|| {
                        Error::Refused(format!(
                            "`{k}` is not a kind of task: use extract, merge, question, summary or classify."
                        ))
                    })?,
            ),
        };
        let limit = limit.clamp(1, MAX_TASKS);
        let tasks = Tasks::new(&self.gateway.vault);
        // The days that ended since the last call are due now.
        let today = summaries::today(&self.gateway.vault);
        tasks.plan_summaries(&self.gateway.index, today)?;
        tasks.plan_questions(&self.gateway.index, today)?;
        tasks.plan_classify(&self.gateway.index, today)?;
        let mut open: Vec<Task> = tasks
            .list()?
            .into_iter()
            .filter(|t| t.status == TaskStatus::Open)
            .filter(|t| kind.is_none_or(|k| t.kind == k))
            .filter(|t| self.sees(t))
            .collect();
        let more = open.len() > limit;
        open.truncate(limit);
        for task in &open {
            for category in &task.categories {
                self.returned(category);
            }
        }
        Ok((
            open.into_iter()
                .map(|t| TaskLine {
                    kind: t.kind.as_str(),
                    items: t.items.len(),
                    id: t.id,
                    description: t.description,
                    comment: t.comment,
                })
                .collect(),
            more,
        ))
    }

    /// The task `id` in full, if this AI may work on it. Items now in a category it
    /// may not read are left out.
    ///
    /// # Errors
    ///
    /// [`Error::Refused`] when there is no such task, or this AI may not see it, in
    /// the same words.
    pub fn task(&self, id: &str) -> Result<TaskOut> {
        let tasks = Tasks::new(&self.gateway.vault);
        let task = match tasks.get(id) {
            Ok(task) if self.sees(&task) => task,
            Ok(_) | Err(Error::Refused(_)) => return Err(no_task(id)),
            Err(e) => return Err(e),
        };
        let detail = tasks.detail(&task.id)?;
        for category in &task.categories {
            self.returned(category);
        }
        let items: Vec<ItemOut> = detail
            .items
            .into_iter()
            .filter(|i| self.reads(&i.category))
            .map(|i| ItemOut {
                file: i.file,
                name: i.name,
                file_type: i.file_type,
                text: i.text,
                details: i.details,
            })
            .collect();
        let mut example: Value = serde_json::from_str(&detail.example).unwrap_or(Value::Null);
        // A question's answer may link the person from the owner's own file: shown only
        // to an AI that may propose there.
        let owner = (task.kind == TaskKind::Question)
            .then(|| self.owner_file_if_proposes())
            .flatten();
        // The example names a file of the task: never one this AI may no longer read.
        if let Some(changes) = example.get_mut("changes").and_then(Value::as_array_mut) {
            if task.kind == TaskKind::Question {
                changes.retain(|c| {
                    c.get("kind").and_then(Value::as_str) != Some("add_relation")
                        || c.get("file").and_then(Value::as_str) == owner.as_deref()
                });
            }
            let fallback = items
                .first()
                .map_or("people/marie-dupont.md", |i| i.file.as_str());
            for change in changes {
                let Some(file) = change.get("file").and_then(Value::as_str) else {
                    continue;
                };
                let visible =
                    items.iter().any(|i| i.file == file) || owner.as_deref() == Some(file);
                if !visible {
                    change["file"] = Value::String(fallback.to_owned());
                }
            }
        }
        Ok(TaskOut {
            id: task.id,
            kind: task.kind.as_str(),
            status: task.status.as_str(),
            description: task.description,
            comment: task.comment,
            expected: detail.expected,
            items,
            example,
        })
    }

    /// Submits `changes` as a proposal by this AI, answering `task` if given. With no
    /// change and a task, closes the task: there was nothing worth keeping.
    ///
    /// # Errors
    ///
    /// [`Error::Refused`] with a message the AI can act on: a change to a file it may
    /// not propose on (worded as for a file that does not exist), a task it may not
    /// see, a proposal too large, or any refusal of
    /// [`crate::proposals::Proposals::submit`].
    pub fn propose(
        &self,
        task: Option<&str>,
        changes: Vec<Change>,
        reason: &str,
    ) -> Result<Proposed> {
        self.propose_noted(task, changes, reason, false)
    }

    /// As [`Access::propose`]; `on_its_own` when the AI noted it during a conversation
    /// without the person asking (issue #246). The proposal is marked so in the
    /// review inbox, and goes through the same checks, rights and standing consent as
    /// any other: the mark widens nothing. An answer to a task is never on its own.
    ///
    /// # Errors
    ///
    /// As for [`Access::propose`].
    pub fn propose_noted(
        &self,
        task: Option<&str>,
        changes: Vec<Change>,
        reason: &str,
        on_its_own: bool,
    ) -> Result<Proposed> {
        let task = task.map(str::trim).filter(|t| !t.is_empty());
        if let Some(id) = task {
            match Tasks::new(&self.gateway.vault).get(id) {
                Ok(found) if self.sees(&found) => {}
                Ok(_) | Err(Error::Refused(_)) => return Err(no_task(id)),
                Err(e) => return Err(e),
            }
        }
        if changes.is_empty() {
            let Some(id) = task else {
                return Err(Error::Refused(
                    "A proposal needs at least one change. To say a task holds nothing worth keeping, give its `task` with no changes.".to_owned(),
                ));
            };
            let tasks = Tasks::new(&self.gateway.vault);
            let found = tasks.get(id)?;
            tasks.dismiss(&found.id)?;
            for category in &found.categories {
                self.returned(category);
            }
            return Ok(Proposed {
                proposal: None,
                status: "done",
            });
        }
        if changes.len() > MAX_CHANGES {
            return Err(Error::Refused(format!(
                "A proposal holds at most {MAX_CHANGES} changes: split it in several."
            )));
        }
        if reason.chars().count() > MAX_REASON {
            return Err(Error::Refused(format!(
                "The reason is too long: keep it under {MAX_REASON} characters."
            )));
        }
        if serde_json::to_vec(&changes).map_or(0, |v| v.len()) > MAX_BYTES {
            return Err(Error::Refused(
                "This proposal is too large: split it in several.".to_owned(),
            ));
        }
        for change in &changes {
            self.may_change(change)?;
        }
        let proposal = Proposals::new(&self.gateway.vault).submit_noted(
            Author::Client(self.client.id.clone()),
            reason,
            task.map(str::to_owned),
            changes,
            on_its_own && task.is_none(),
        )?;
        for category in &proposal.categories {
            self.returned(category);
        }
        // The person's standing consent for this AI, if any: accepted now, by the same
        // path as when they accept it (decision 0017).
        let mut status = "waiting";
        if !self.client.auto_accept.is_empty()
            && let Some(Accepted::Applied(_)) = Proposals::new(&self.gateway.vault)
                .accept_by_consent(&proposal.id, &self.client.auto_accept)?
        {
            status = "accepted";
        }
        Ok(Proposed {
            proposal: Some(proposal.id),
            status,
        })
    }

    /// Refuses a change this AI may not make, in the words used for a file that does
    /// not exist when it touches a file it may not propose on.
    fn may_change(&self, change: &Change) -> Result<()> {
        if let Change::WriteSummary { .. } = change {
            if !self.proposes("summaries") {
                return Err(Error::Refused(
                    "You cannot propose summaries in this memory.".to_owned(),
                ));
            }
            return Ok(());
        }
        if let Change::Create {
            file_type,
            category,
            notes,
            ..
        } = change
        {
            if notes
                .as_ref()
                .is_some_and(|n| n.chars().count() > MAX_NOTES)
            {
                return Err(Error::Refused(format!(
                    "The notes of a new file are too long: keep them under {MAX_NOTES} characters."
                )));
            }
            let category = match category {
                Some(c) => Category::new(c),
                None => FileType::parse(file_type).and_then(FileType::default_category),
            };
            // An unknown type or category is refused by the proposal's own checks.
            if let Some(category) = category
                && !self.proposes(category.as_str())
            {
                return Err(Error::Refused(
                    "You cannot propose new files in this category.".to_owned(),
                ));
            }
            return Ok(());
        }
        let Some(file) = change.file() else {
            return Ok(());
        };
        let path = self.gateway.vault.root().join(file);
        // Bad paths and missing files are refused by the proposal's own checks, with
        // the same words used here.
        if !file.contains("..")
            && path.is_file()
            && let Ok(found) = MemoryFile::read(&path)
            && !self.proposes(found.common().category.as_str())
        {
            return Err(Error::Refused(format!("There is no file {file}.")));
        }
        Ok(())
    }

    /// The proposal `id`, if this AI made it.
    ///
    /// # Errors
    ///
    /// [`Error::Refused`] when there is no such proposal, or another AI made it, in
    /// the same words.
    pub fn proposal(&self, id: &str) -> Result<ProposalOut> {
        let proposal = match Proposals::new(&self.gateway.vault).get(id) {
            Ok(p) if p.by == Author::Client(self.client.id.clone()) => p,
            Ok(_) | Err(Error::Refused(_)) => return Err(no_proposal(id)),
            Err(e) => return Err(e),
        };
        for category in &proposal.categories {
            self.returned(category);
        }
        let (comment, stale) = match &proposal.status {
            Status::Refused { comment, .. } => (comment.clone(), None),
            Status::Stale { reason, .. } => (None, Some(reason.clone())),
            _ => (None, None),
        };
        Ok(ProposalOut {
            status: proposal.status.as_str(),
            id: proposal.id,
            reason: proposal.reason,
            task: proposal.task,
            comment,
            stale,
        })
    }
}
