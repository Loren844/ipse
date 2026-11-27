//! `ipse sync`: syncs the sources the person subscribed to.

use std::path::PathBuf;

use ipse_core::connector::{Instances, Outcome, StopReason, SyncState};
use ipse_core::sync::{SyncOptions, Synced, sync_instance};
use ipse_core::time::Timestamp;
use ipse_core::vault::Vault;
use ipse_core::{Error, Result};

use super::import::{
    OWNER_LINE, conflicts_line, deleted_line, events_line, relations_line, summary_line, tasks_line,
};
use crate::keychain::secrets;

/// Syncs every subscription, or with `due` only those whose interval passed: the form
/// for a job the system runs every few minutes.
pub(crate) fn run(due: bool, full: bool, vault: Option<PathBuf>) -> Result<()> {
    let vault = Vault::open(&vault.unwrap_or_else(|| PathBuf::from(".")))?;
    let instances = Instances::load(vault.root())?;
    if instances.list().is_empty() {
        if !due {
            println!(
                "Nothing to sync yet. Subscribe to a calendar with `ipse calendar subscribe <link>`, or connect an account with `ipse dav add <address> --user <name>`."
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
    let secrets = secrets();
    for instance in instances.list() {
        if due && !state.due(instance, now) {
            continue;
        }
        let synced = sync_instance(&vault, instance, &*secrets, &options)?;
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
            lines.push(conflicts_line((s.conflicts, s.proposals)));
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
            lines.push(conflicts_line((s.conflicts, s.proposals)));
        }
        if s.owner {
            lines.push(OWNER_LINE.to_owned());
        }
        if s.relations > 0 {
            lines.push(relations_line(s.relations));
        }
    }
    if let Some(s) = synced.home
        && s.logs > 0
    {
        lines.push(home_line(s.stays));
    }
    if let Some(s) = synced.watching
        && s.logs > 0
    {
        lines.push(watching_line(s.plays));
    }
    if let Some(s) = synced.photos {
        if s.logs > 0 {
            lines.push(photos_line(s.people, s.linked));
        }
        if s.proposals > 0 {
            lines.push(aliases_line(s.proposals));
        }
    }
    if synced.tasks > 0 {
        lines.push(tasks_line(synced.tasks));
    }
    if synced.duplicates > 0 {
        lines.push(super::duplicates::line(synced.duplicates));
    }
    lines
}

fn nothing_changed(synced: &Synced) -> bool {
    synced.progress.is_empty()
        && synced.tasks == 0
        && synced.duplicates == 0
        && synced
            .events
            .is_none_or(|s| s.added + s.updated + s.cancelled + s.conflicts == 0)
        && synced
            .people
            .is_none_or(|s| s.added + s.updated + s.conflicts + s.relations == 0 && !s.owner)
        && synced.home.is_none_or(|s| s.logs == 0)
        && synced.watching.is_none_or(|s| s.logs == 0)
        && synced
            .photos
            .is_none_or(|s| s.logs == 0 && s.proposals == 0)
}

fn photos_line(people: u64, linked: u64) -> String {
    let people = match people {
        1 => "1 person".to_owned(),
        n => format!("{n} people"),
    };
    format!("Updated who is on your photos: {people}, {linked} found in your memory.")
}

fn aliases_line(proposals: u64) -> String {
    match proposals {
        1 => "1 person of the photos may be someone of your memory: see `ipse proposals list`."
            .to_owned(),
        n => format!(
            "{n} people of the photos may be someone of your memory: see `ipse proposals list`."
        ),
    }
}

fn watching_line(plays: u64) -> String {
    let plays = match plays {
        1 => "1 film or episode".to_owned(),
        n => format!("{n} films and episodes"),
    };
    format!("Updated what you watch: {plays} in all.")
}

fn home_line(stays: u64) -> String {
    let stays = match stays {
        1 => "1 stay".to_owned(),
        n => format!("{n} stays"),
    };
    format!("Updated when you were at home and in your zones: {stays} in all.")
}

fn stopped_line(reason: StopReason) -> String {
    format!("The sync stopped: {reason}. Nothing already synced is lost; Ipse will try again.")
}
