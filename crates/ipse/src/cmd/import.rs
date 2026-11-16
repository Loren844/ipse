//! `ipse import`: bring a file into the memory.

use std::fmt::Write as _;
use std::path::PathBuf;

use ipse_core::connector::{Launch, Outcome, RunRecord, record_run, run_session};
use ipse_core::journal::Journal;
use ipse_core::materialize::{
    EventsSummary, PeopleSummary, calendar_to_events, contacts_to_people,
};
use ipse_core::tasks::Tasks;
use ipse_core::vault::{DateTime, Source, Vault};
use ipse_core::{Error, Result};

/// What a file holds, from its extension.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Kind {
    Contacts,
    Calendar,
}

impl Kind {
    fn of(file: &std::path::Path) -> Option<Self> {
        let extension = file.extension()?.to_str()?.to_ascii_lowercase();
        match extension.as_str() {
            "vcf" | "vcard" => Some(Self::Contacts),
            "ics" | "ical" | "ifb" => Some(Self::Calendar),
            _ => None,
        }
    }

    /// The built-in connector, which is also the journal source.
    fn connector(self) -> &'static str {
        match self {
            Self::Contacts => "contacts",
            Self::Calendar => "calendar",
        }
    }

    fn record_kind(self) -> &'static str {
        match self {
            Self::Contacts => "contact.card",
            Self::Calendar => "calendar.event",
        }
    }
}

/// Imports `file` into the memory in `vault` (the current folder by default).
pub(crate) fn run(file: &std::path::Path, vault: Option<PathBuf>) -> Result<()> {
    let vault = Vault::open(&vault.unwrap_or_else(|| PathBuf::from(".")))?;
    for line in import_into(&vault, file)? {
        println!("{line}");
    }
    Ok(())
}

/// Imports `file` into `vault` as `ipse import` does; returns what to tell the person.
///
/// # Errors
///
/// [`Error::Refused`] for a file Ipse cannot import or a connector that failed;
/// errors writing the journal, the files, the history or the tasks.
pub(crate) fn import_into(vault: &Vault, file: &std::path::Path) -> Result<Vec<String>> {
    let kind = Kind::of(file).ok_or_else(|| {
        Error::Refused(
            "Ipse can import contacts files (.vcf) and calendar files (.ics) for now.".to_owned(),
        )
    })?;
    let file = std::fs::canonicalize(file)
        .ok()
        .filter(|p| p.is_file())
        .ok_or_else(|| Error::Refused(format!("{} is not a file.", file.display())))?;
    let settings = vault.settings();
    let instance = kind.connector();
    let launch = launch(kind, file, settings.time_zone_name())?;

    let mut journal = Journal::new(vault.root());
    let report = run_session(&mut journal, &launch)?;
    let mut run = RunRecord::new(
        DateTime::now(settings.time_zone()).to_string(),
        instance,
        "import",
        &report,
        journal.recovered(),
    );
    if !matches!(report.outcome, Outcome::Done(_)) {
        record_run(vault.root(), &run)?;
    }
    match report.outcome {
        Outcome::Done(_) => {}
        Outcome::Failed { message, .. } | Outcome::Needs { message, .. } => {
            return Err(Error::Refused(message));
        }
        Outcome::Stopped(reason) => {
            return Err(Error::Refused(format!(
                "The import stopped: {reason}. Nothing already imported is lost; try again."
            )));
        }
    }
    let mut lines = report.progress.clone();

    // The person's own edits are recorded first, so that the import never hides them.
    let history = vault.history()?;
    let mut change = history.change(Source::Connector(instance.to_owned()));
    let mut relations = 0;
    let mut owner = false;
    let (line, conflicts, deleted) = match kind {
        Kind::Contacts => {
            let summary = contacts_to_people(vault, &journal, instance, &mut change)?;
            run.people(&summary);
            relations = summary.relations;
            owner = summary.owner;
            (
                summary_line(summary),
                (summary.conflicts, summary.proposals),
                summary.deleted,
            )
        }
        Kind::Calendar => {
            let summary = calendar_to_events(
                vault,
                &journal,
                instance,
                &mut change,
                jiff::Timestamp::now(),
            )?;
            run.events(&summary);
            (
                events_line(summary),
                (summary.conflicts, summary.proposals),
                summary.deleted,
            )
        }
    };
    let summary = match kind {
        Kind::Contacts => "Import contacts",
        Kind::Calendar => "Import calendar",
    };
    let touched = change.paths();
    change.commit(summary)?;
    let tasks = Tasks::new(vault).extract_from(&touched)?;
    run.tasks = tasks as u64;
    record_run(vault.root(), &run)?;
    let duplicates = ipse_core::duplicates::propose(vault)?;
    lines.push(line);
    let optional = [
        (conflicts.0 > 0).then(|| conflicts_line(conflicts)),
        (deleted > 0).then(|| deleted_line(deleted)),
        owner.then(|| OWNER_LINE.to_owned()),
        (relations > 0).then(|| relations_line(relations)),
        (tasks > 0).then(|| tasks_line(tasks)),
        (duplicates > 0).then(|| super::duplicates::line(duplicates)),
    ];
    lines.extend(optional.into_iter().flatten());
    Ok(lines)
}

/// How to start the built-in connector for `kind` on `file`.
fn launch(kind: Kind, file: PathBuf, time_zone: &str) -> Result<Launch> {
    let exe = std::env::current_exe().map_err(|source| Error::Io {
        action: "find",
        path: PathBuf::from("ipse"),
        source,
    })?;
    let instance = kind.connector();
    let mut launch = Launch::new(
        vec![
            exe.to_string_lossy().into_owned(),
            "connector".to_owned(),
            instance.to_owned(),
        ],
        instance,
        // The command line speaks English, whatever the vault's language.
        "en",
        time_zone,
    );
    launch.files = vec![file];
    launch.kinds = Some(vec![kind.record_kind().to_owned()]);
    Ok(launch)
}

pub(crate) fn tasks_line(n: usize) -> String {
    let tasks = if n == 1 {
        "1 task"
    } else {
        &format!("{n} tasks")
    };
    format!("{tasks} for your AI: notes to turn into facts (`ipse tasks list`).")
}

pub(crate) const OWNER_LINE: &str = "Your own contact card completed your file.";

pub(crate) fn relations_line(n: u64) -> String {
    if n == 1 {
        "1 relation added to your own file, from your contact card.".to_owned()
    } else {
        format!("{n} relations added to your own file, from your contact card.")
    }
}

pub(crate) fn events_line(s: EventsSummary) -> String {
    let events = |n: u64| if n == 1 { "event" } else { "events" };
    let mut line = if s.added == 0 && s.updated == 0 && s.cancelled == 0 {
        format!(
            "Every event was already up to date ({} {}).",
            s.unchanged,
            events(s.unchanged)
        )
    } else {
        format!(
            "{} {} added, {} updated.",
            s.added,
            events(s.added),
            s.updated
        )
    };
    if s.cancelled > 0 {
        let _ = write!(
            line,
            " {} removed from the calendar, marked cancelled.",
            s.cancelled
        );
    }
    line
}

pub(crate) fn deleted_line(n: u64) -> String {
    if n == 1 {
        "1 file you deleted was not created again.".to_owned()
    } else {
        format!("{n} files you deleted were not created again.")
    }
}

/// The files where the source changed what the person had changed, and the proposals
/// that wait for them.
pub(crate) fn conflicts_line((n, proposals): (u64, u64)) -> String {
    let mut line = if n == 1 {
        "1 file: the source changed a detail you had changed yourself; your version was kept."
            .to_owned()
    } else {
        format!(
            "{n} files: the source changed details you had changed yourself; your versions were kept."
        )
    };
    if proposals > 0 {
        line.push_str(" What the source says waits for you: `ipse proposals list`.");
    }
    line
}

pub(crate) fn summary_line(s: PeopleSummary) -> String {
    let people = |n: u64| if n == 1 { "person" } else { "people" };
    if s.added == 0 && s.updated == 0 {
        format!(
            "Everyone was already up to date ({} {}).",
            s.unchanged,
            people(s.unchanged)
        )
    } else {
        format!(
            "{} {} added, {} updated.",
            s.added,
            people(s.added),
            s.updated
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn summary_in_plain_words() {
        let s = |added, updated, unchanged| PeopleSummary {
            added,
            updated,
            unchanged,
            skipped: 0,
            conflicts: 0,
            deleted: 0,
            relations: 0,
            owner: false,
            proposals: 0,
        };
        assert_eq!(summary_line(s(4, 0, 0)), "4 people added, 0 updated.");
        assert_eq!(summary_line(s(1, 2, 3)), "1 person added, 2 updated.");
        assert_eq!(
            summary_line(s(0, 0, 4)),
            "Everyone was already up to date (4 people)."
        );
    }
}
