//! `ipse calendar`: subscribe to a calendar's link, list and remove subscriptions.

use std::path::{Path, PathBuf};

use ipse_core::connector::{
    INSTANCES_FILE, Instance, Instances, Outcome, SecretStore, SyncState, duration, secret_key,
};
use ipse_core::sync::{SyncOptions, sync_instance};
use ipse_core::time::{SignedDuration, Timestamp};
use ipse_core::vault::{Source, Vault};
use ipse_core::{Error, Result};
use ipse_protocol::{ErrorCode, Secret};

use super::sync::{connector_command, outcome_lines};
use crate::connectors::calendar::{self, LINK_SECRET, fetch};
use crate::keychain::Keychain;

/// How often a calendar syncs unless the person says otherwise.
const DEFAULT_EVERY: SignedDuration = SignedDuration::from_mins(15);

/// Calendar servers are asked at most this often.
const MIN_EVERY: SignedDuration = SignedDuration::from_mins(5);

/// `ipse calendar subscribe`: checks the link, syncs the calendar once, and keeps the
/// subscription when that worked, or when the calendar only could not be reached.
pub(crate) fn subscribe(
    link: &str,
    name: Option<&str>,
    every: Option<&str>,
    vault: Option<PathBuf>,
) -> Result<()> {
    let link = fetch::parse(link).map_err(|e| Error::Refused(e.message(false)))?;
    let every = match every {
        None => DEFAULT_EVERY,
        Some(text) => duration(text).filter(|d| *d >= MIN_EVERY).ok_or_else(|| {
            Error::Refused(
                "--every takes a duration of at least 5 minutes, like 15m or 1h.".to_owned(),
            )
        })?,
    };
    let vault = open(vault)?;
    let root = vault.root();
    let secrets = Keychain;
    let mut instances = Instances::load(root)?;

    for existing in calendars(&instances) {
        let same = secrets
            .get(&secret_key(root, &existing.id, LINK_SECRET))?
            .is_some_and(|s| s.expose() == link.as_str());
        if same {
            return Err(Error::Refused(format!(
                "You are already subscribed to this calendar, as {:?} ({}).",
                existing.name, existing.id
            )));
        }
    }

    let instance = instances.new_instance(calendar::ID, name.unwrap_or("Calendar"), every)?;
    let key = secret_key(root, &instance.id, LINK_SECRET);
    secrets.set(&key, &Secret::new(link.as_str()))?;
    let options = SyncOptions {
        command: connector_command()?,
        language: "en".to_owned(),
        full: true,
        now: Timestamp::now(),
    };
    let synced = match sync_instance(&vault, &instance, &secrets, &options) {
        Ok(synced) => synced,
        Err(e) => {
            forget(root, &instance, &secrets)?;
            return Err(e);
        }
    };
    let kept = match &synced.outcome {
        Outcome::Done(_) => true,
        Outcome::Failed { code, .. } => {
            matches!(code, ErrorCode::Unavailable | ErrorCode::RateLimited)
        }
        Outcome::Needs { .. } | Outcome::Stopped(_) => false,
    };
    if !kept {
        forget(root, &instance, &secrets)?;
        let message = outcome_lines(&synced).join(" ");
        return Err(Error::Refused(message));
    }

    instances.add(instance.clone())?;
    let history = vault.history()?;
    let mut change = history.change(Source::Person);
    change.touched(&root.join(INSTANCES_FILE));
    change.commit("Subscribe to a calendar")?;

    for line in outcome_lines(&synced) {
        println!("{line}");
    }
    if synced.succeeded() {
        println!(
            "Subscribed to {:?} ({}). It syncs every {} when `ipse sync --due` runs; the guide shows how to run it on its own.",
            instance.name,
            instance.id,
            every_text(instance.every)
        );
    } else {
        println!(
            "The subscription to {:?} ({}) is kept: Ipse will try again. If the link is wrong, remove it with `ipse calendar unsubscribe {}`.",
            instance.name, instance.id, instance.id
        );
    }
    Ok(())
}

/// `ipse calendar list`: the subscriptions and when they last synced.
pub(crate) fn list(vault: Option<PathBuf>) -> Result<()> {
    let vault = open(vault)?;
    let instances = Instances::load(vault.root())?;
    let state = SyncState::load(vault.root())?;
    let zone = vault.settings().time_zone().clone();
    let mut any = false;
    for instance in calendars(&instances) {
        any = true;
        let last = match state.last(&instance.id) {
            None => "never synced".to_owned(),
            Some(run) => {
                let at = run.at.to_zoned(zone.clone()).strftime("%Y-%m-%d %H:%M");
                match run.failures {
                    0 => format!("synced {at}"),
                    1 => format!("failed {at}"),
                    n => format!("failed {at}, {n} times in a row"),
                }
            }
        };
        println!(
            "{}  {}  every {}  {last}",
            instance.id,
            instance.name,
            every_text(instance.every)
        );
    }
    if !any {
        println!("No calendar subscriptions yet. Add one with `ipse calendar subscribe <link>`.");
    }
    Ok(())
}

/// `ipse calendar unsubscribe`: stops syncing a calendar. Its events stay.
pub(crate) fn unsubscribe(id: &str, vault: Option<PathBuf>) -> Result<()> {
    let vault = open(vault)?;
    let root = vault.root();
    let mut instances = Instances::load(root)?;
    let instance = instances
        .get(id)
        .filter(|i| i.connector == calendar::ID)
        .cloned()
        .ok_or_else(|| {
            Error::Refused(format!(
                "There is no calendar subscription {id:?}. `ipse calendar list` shows them."
            ))
        })?;
    instances.remove(id)?;
    forget(root, &instance, &Keychain)?;
    let history = vault.history()?;
    let mut change = history.change(Source::Person);
    change.touched(&root.join(INSTANCES_FILE));
    change.commit("Unsubscribe from a calendar")?;
    println!(
        "Unsubscribed from {:?}. Its events stay in your memory.",
        instance.name
    );
    Ok(())
}

fn open(vault: Option<PathBuf>) -> Result<Vault> {
    Vault::open(&vault.unwrap_or_else(|| PathBuf::from(".")))
}

fn calendars(instances: &Instances) -> impl Iterator<Item = &Instance> {
    instances
        .list()
        .iter()
        .filter(|i| i.connector == calendar::ID)
}

/// Removes what Ipse kept for `instance` outside the list: its link and its last run.
fn forget(root: &Path, instance: &Instance, secrets: &dyn SecretStore) -> Result<()> {
    secrets.delete(&secret_key(root, &instance.id, LINK_SECRET))?;
    SyncState::load(root)?.forget(&instance.id)
}

/// `15 minutes`, `1 hour`, `1h30m`.
fn every_text(every: SignedDuration) -> String {
    let minutes = every.as_secs() / 60;
    match (minutes / 60, minutes % 60) {
        (0, 1) => "minute".to_owned(),
        (0, m) => format!("{m} minutes"),
        (1, 0) => "hour".to_owned(),
        (h, 0) => format!("{h} hours"),
        (h, m) => format!("{h}h{m:02}"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn intervals_in_words() {
        assert_eq!(every_text(SignedDuration::from_mins(15)), "15 minutes");
        assert_eq!(every_text(SignedDuration::from_hours(1)), "hour");
        assert_eq!(every_text(SignedDuration::from_hours(6)), "6 hours");
        assert_eq!(every_text(SignedDuration::from_mins(90)), "1h30");
    }
}
