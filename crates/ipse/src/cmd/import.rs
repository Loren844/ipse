//! `ipse import`: bring a file into the memory.

use std::fmt::Write as _;
use std::path::PathBuf;

use ipse_core::connector::{Launch, Outcome, run_import};
use ipse_core::journal::Journal;
use ipse_core::materialize::{
    EventsSummary, PeopleSummary, calendar_to_events, contacts_to_people,
};
use ipse_core::vault::{Source, Vault};
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
pub(crate) fn run(file: &PathBuf, vault: Option<PathBuf>) -> Result<()> {
    let vault = Vault::open(&vault.unwrap_or_else(|| PathBuf::from(".")))?;
    let kind = Kind::of(file).ok_or_else(|| {
        Error::Refused(
            "Ipse can import contacts files (.vcf) and calendar files (.ics) for now.".to_owned(),
        )
    })?;
    let file = std::fs::canonicalize(file)
        .ok()
        .filter(|p| p.is_file())
        .ok_or_else(|| Error::Refused(format!("{} is not a file.", file.display())))?;
    let exe = std::env::current_exe().map_err(|source| Error::Io {
        action: "find",
        path: PathBuf::from("ipse"),
        source,
    })?;

    let settings = vault.settings();
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
        settings.time_zone_name(),
    );
    launch.files = vec![file];
    launch.kinds = Some(vec![kind.record_kind().to_owned()]);

    let mut journal = Journal::new(vault.root());
    let report = run_import(&mut journal, &launch)?;
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
    for line in &report.progress {
        println!("{line}");
    }

    // The person's own edits are recorded first, so that the import never hides them.
    let history = vault.history()?;
    let mut change = history.change(Source::Connector(instance.to_owned()));
    let (line, conflicts, deleted) = match kind {
        Kind::Contacts => {
            let summary = contacts_to_people(&vault, &journal, instance, &mut change)?;
            (summary_line(summary), summary.conflicts, summary.deleted)
        }
        Kind::Calendar => {
            let summary = calendar_to_events(
                &vault,
                &journal,
                instance,
                &mut change,
                jiff::Timestamp::now(),
            )?;
            (events_line(summary), summary.conflicts, summary.deleted)
        }
    };
    let summary = match kind {
        Kind::Contacts => "Import contacts",
        Kind::Calendar => "Import calendar",
    };
    change.commit(summary)?;
    println!("{line}");
    if conflicts > 0 {
        println!("{}", conflicts_line(conflicts));
    }
    if deleted > 0 {
        println!("{}", deleted_line(deleted));
    }
    Ok(())
}

fn events_line(s: EventsSummary) -> String {
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

fn deleted_line(n: u64) -> String {
    if n == 1 {
        "1 file you deleted was not created again.".to_owned()
    } else {
        format!("{n} files you deleted were not created again.")
    }
}

fn conflicts_line(n: u64) -> String {
    if n == 1 {
        "1 file: the source changed a detail you had changed yourself; your version was kept."
            .to_owned()
    } else {
        format!(
            "{n} files: the source changed details you had changed yourself; your versions were kept."
        )
    }
}

fn summary_line(s: PeopleSummary) -> String {
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
        };
        assert_eq!(summary_line(s(4, 0, 0)), "4 people added, 0 updated.");
        assert_eq!(summary_line(s(1, 2, 3)), "1 person added, 2 updated.");
        assert_eq!(
            summary_line(s(0, 0, 4)),
            "Everyone was already up to date (4 people)."
        );
    }
}
