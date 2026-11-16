//! `ipse duplicates`: looks for people who may be the same, and events copied into
//! two calendars, and proposes merges (decision 0011). Nothing is merged here.

use std::path::PathBuf;

use ipse_core::Result;
use ipse_core::duplicates;
use ipse_core::vault::Vault;

/// Looks for possible duplicates in `vault` and proposes the new ones.
pub(crate) fn run(vault: Option<PathBuf>) -> Result<()> {
    let vault = Vault::open(&vault.unwrap_or_else(|| PathBuf::from(".")))?;
    let made = duplicates::propose(&vault)?;
    if made == 0 {
        println!("No new possible duplicate.");
    } else {
        println!("{}", line(made));
    }
    Ok(())
}

/// What an import or a sync says when it found possible duplicates.
pub(crate) fn line(made: usize) -> String {
    let what = if made == 1 {
        "1 possible duplicate waits".to_owned()
    } else {
        format!("{made} possible duplicates wait")
    };
    format!("{what} for your decision: merge, or keep apart (`ipse ui`, or `ipse proposals list`).")
}
