//! `ipse import`: bring a file into the memory.

use std::path::PathBuf;

use ipse_core::connector::{Launch, Outcome, run_import};
use ipse_core::journal::Journal;
use ipse_core::materialize::{PeopleSummary, contacts_to_people};
use ipse_core::vault::{Source, Vault};
use ipse_core::{Error, Result};

/// Imports `file` into the memory in `vault` (the current folder by default).
pub(crate) fn run(file: &PathBuf, vault: Option<PathBuf>) -> Result<()> {
    let vault = Vault::open(&vault.unwrap_or_else(|| PathBuf::from(".")))?;
    let is_vcf = file
        .extension()
        .and_then(|e| e.to_str())
        .is_some_and(|e| e.eq_ignore_ascii_case("vcf"));
    if !is_vcf {
        return Err(Error::Refused(
            "Ipse can import contacts files (.vcf) for now.".to_owned(),
        ));
    }
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
    let mut launch = Launch::new(
        vec![
            exe.to_string_lossy().into_owned(),
            "connector".to_owned(),
            "contacts".to_owned(),
        ],
        "contacts",
        // The command line speaks English, whatever the vault's language.
        "en",
        settings.time_zone_name(),
    );
    launch.files = vec![file];
    launch.kinds = Some(vec!["contact.card".to_owned()]);

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

    let history = vault.history()?;
    let mut change = history.change(Source::Connector("contacts".to_owned()));
    let summary = contacts_to_people(&vault, &journal, "contacts", &mut change)?;
    change.commit("Import contacts")?;
    println!("{}", summary_line(summary));
    Ok(())
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
        };
        assert_eq!(summary_line(s(4, 0, 0)), "4 people added, 0 updated.");
        assert_eq!(summary_line(s(1, 2, 3)), "1 person added, 2 updated.");
        assert_eq!(
            summary_line(s(0, 0, 4)),
            "Everyone was already up to date (4 people)."
        );
    }
}
