//! `ipse sync`: syncs the sources the person subscribed to.

use std::path::PathBuf;

use ipse_core::connector::{Instances, Outcome, StopReason, SyncState};
use ipse_core::sync::{SyncOptions, Synced, sync_instance};
use ipse_core::time::Timestamp;
use ipse_core::vault::Vault;
use ipse_core::{Error, Result};

use super::import::{
    OWNER_LINE, conflicts_line, deleted_line, events_line, relations_line, summary_line,
};
use crate::keychain::Keychain;

/// Syncs every subscription, or with `due` only those whose interval passed: the form
/// for a job the system runs every few minutes.
pub(crate) fn run(due: bool, full: bool, vault: Option<PathBuf>) -> Result<()> {
    let vault = Vault::open(&vault.unwrap_or_else(|| PathBuf::from(".")))?;
    let instances = Instances::load(vault.root())?;
    if instances.list().is_empty() {
        if !due {
            println!(
                "Nothing to sync yet. Subscribe to a calendar with `ipse calendar subscribe <link>`."
            );
        }
        return Ok(());
    }
    let now = Timestamp::now();
    let state = SyncState::load(vault.root())?;
    let options = SyncOptions {
        command: connector_command()?,
        language: "en".to_owned(),
        full,
        now,
    };
    for instance in instances.list() {
        if due && !state.due(instance, now) {
            continue;
        }
        let synced = sync_instance(&vault, instance, &Keychain, &options)?;
        let lines = outcome_lines(&synced);
        // A job run on a schedule says something only when there is something to say.
        let quiet = due && synced.succeeded() && nothing_changed(&synced);
        if !quiet {
            println!("{} ({}): {}", instance.name, instance.id, lines.join(" "));
        }
    }
    Ok(())
}

/// How to start a built-in connector: this very program, `ipse connector <name>`.
pub(crate) fn connector_command() -> Result<Vec<String>> {
    let exe = std::env::current_exe().map_err(|source| Error::Io {
        action: "find",
        path: PathBuf::from("ipse"),
        source,
    })?;
    Ok(vec![
        exe.to_string_lossy().into_owned(),
        "connector".to_owned(),
    ])
}

/// What a sync did, in plain words.
pub(crate) fn outcome_lines(synced: &Synced) -> Vec<String> {
    let mut lines = synced.progress.clone();
    match &synced.outcome {
        Outcome::Done(_) => {}
        Outcome::Failed { message, .. } | Outcome::Needs { message, .. } => {
            lines.push(message.clone());
        }
        Outcome::Stopped(reason) => lines.push(stopped_line(*reason)),
    }
    if let Some(s) = synced.events {
        if synced.succeeded() || s.added + s.updated + s.cancelled > 0 {
            lines.push(events_line(s));
        }
        if s.conflicts > 0 {
            lines.push(conflicts_line(s.conflicts));
        }
        if s.deleted > 0 {
            lines.push(deleted_line(s.deleted));
        }
    }
    if let Some(s) = synced.people {
        if synced.succeeded() || s.added + s.updated > 0 {
            lines.push(summary_line(s));
        }
        if s.conflicts > 0 {
            lines.push(conflicts_line(s.conflicts));
        }
        if s.owner {
            lines.push(OWNER_LINE.to_owned());
        }
        if s.relations > 0 {
            lines.push(relations_line(s.relations));
        }
    }
    lines
}

fn nothing_changed(synced: &Synced) -> bool {
    synced.progress.is_empty()
        && synced
            .events
            .is_none_or(|s| s.added + s.updated + s.cancelled + s.conflicts == 0)
        && synced
            .people
            .is_none_or(|s| s.added + s.updated + s.conflicts + s.relations == 0 && !s.owner)
}

fn stopped_line(reason: StopReason) -> String {
    format!("The sync stopped: {reason}. Nothing already synced is lost; Ipse will try again.")
}
