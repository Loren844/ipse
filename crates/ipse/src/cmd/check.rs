//! `ipse check`: says which files of the memory need fixing, changes nothing.

use std::path::PathBuf;

use ipse_core::vault::{Vault, check};
use ipse_core::{Error, Result};

/// Checks the memory in `vault` (the current folder by default). Fails when something
/// needs fixing, so that a script notices.
pub(crate) fn run(vault: Option<PathBuf>) -> Result<()> {
    let vault = Vault::open(&vault.unwrap_or_else(|| PathBuf::from(".")))?;
    let report = check(vault.root())?;
    let files = |n: usize| if n == 1 { "file" } else { "files" };
    if report.problems.is_empty() {
        println!(
            "Every file reads well ({} {}).",
            report.files,
            files(report.files)
        );
        return Ok(());
    }
    for problem in &report.problems {
        let line = problem
            .line
            .map(|l| format!(", line {l}"))
            .unwrap_or_default();
        println!("{}{line}: {}", problem.path.display(), problem.message);
    }
    let n = report.problems.len();
    Err(Error::Refused(format!(
        "{n} {} to fix, in {} {} looked at. Fix them in any editor, then run `ipse check` again.",
        if n == 1 { "thing" } else { "things" },
        report.files,
        files(report.files)
    )))
}
