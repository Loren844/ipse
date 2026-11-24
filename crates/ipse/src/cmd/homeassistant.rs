//! `ipse homeassistant`: connect the person's Home Assistant, list and remove it
//! (`docs/connectors/homeassistant.md`).
//!
//! The access token goes straight to the system keychain: it is never written to a
//! file, a log, the history or a message.

use std::path::{Path, PathBuf};

use ipse_core::connector::{
    INSTANCES_FILE, Instance, Instances, Outcome, SecretStore, SyncState, duration, secret_key,
};
use ipse_core::journal::Journal;
use ipse_core::sync::{SyncOptions, Synced, sync_instance};
use ipse_core::time::{SignedDuration, Timestamp};
use ipse_core::vault::{Source, Vault};
use ipse_core::{Error, Result};
use ipse_protocol::Secret;
use serde_json::Value;

use super::dav::ask_secret;
use super::sync::{connector_command, outcome_lines};
use crate::connectors::homeassistant::{self, Cursor, Found, TOKEN_SECRET};
use crate::keychain::secrets;

/// How often Home Assistant is read by default: being at home is current for 15
/// minutes (`docs/spec/vault.md`, section 5.2).
pub(crate) const DEFAULT_EVERY: SignedDuration = SignedDuration::from_mins(5);

/// Home Assistant is asked at most this often.
const MIN_EVERY: SignedDuration = SignedDuration::from_mins(1);

/// A Home Assistant now connected, with the people found on it.
pub(crate) struct Connected {
    pub(crate) instance: Instance,
    pub(crate) found: Vec<Found>,
    /// Its address is `http://`, on the home network: the token is sent unencrypted
    /// there.
    pub(crate) plain: bool,
}

/// Why Home Assistant is not connected.
pub(crate) enum NotConnected {
    /// The address or the token cannot be used; why, in English.
    BadAddress(String),
    /// The same server is already connected, under this name.
    Already(String),
    /// The server answered, but not as a Home Assistant Ipse can read.
    Failed(Box<Synced>),
    /// The memory, the history or the keychain failed.
    Other(Error),
}

impl From<Error> for NotConnected {
    fn from(error: Error) -> Self {
        Self::Other(error)
    }
}

/// Connects the Home Assistant at `address`: keeps the token in `secrets`, finds its
/// people, and keeps the source when that worked. Nobody's presence is read until
/// [`choose`].
pub(crate) fn connect(
    vault: &Vault,
    address: &str,
    token: &str,
    name: Option<&str>,
    every: SignedDuration,
    secrets: &dyn SecretStore,
) -> std::result::Result<Connected, NotConnected> {
    let (url, plain) =
        homeassistant::address(address).map_err(|f| NotConnected::BadAddress(f.message(false)))?;
    let token = token.trim();
    if token.is_empty() {
        return Err(NotConnected::BadAddress(
            "Give a long-lived access token of your Home Assistant.".to_owned(),
        ));
    }
    let root = vault.root();
    let mut instances = Instances::load(root)?;
    if let Some(existing) =
        sources(&instances).find(|i| i.config.get("url").and_then(Value::as_str) == Some(&url))
    {
        return Err(NotConnected::Already(existing.name.clone()));
    }
    let mut instance =
        instances.new_instance(homeassistant::ID, name.unwrap_or("Home Assistant"), every)?;
    instance.config.insert("url".to_owned(), Value::String(url));
    secrets.set(
        &secret_key(root, &instance.id, TOKEN_SECRET),
        &Secret::new(token),
    )?;
    let synced = match sync_once(vault, &instance, secrets, true) {
        Ok(synced) => synced,
        Err(e) => {
            forget(root, &instance, secrets)?;
            return Err(NotConnected::Other(e));
        }
    };
    if !matches!(synced.outcome, Outcome::Done(_)) {
        forget(root, &instance, secrets)?;
        return Err(NotConnected::Failed(Box::new(synced)));
    }
    let found = found(vault, &instance.id)?;
    instances.add(instance.clone())?;
    let history = vault.history()?;
    let mut change = history.change(Source::Person);
    change.touched(&root.join(INSTANCES_FILE));
    change.commit("Connect Home Assistant")?;
    Ok(Connected {
        instance,
        found,
        plain,
    })
}

/// The people found on the Home Assistant `id` at its last session.
///
/// # Errors
///
/// Errors reading the journal.
pub(crate) fn found(vault: &Vault, id: &str) -> Result<Vec<Found>> {
    let cursor = Journal::new(vault.root()).cursor(id)?;
    Ok(cursor
        .and_then(|c| serde_json::from_value::<Cursor>(c).ok())
        .map(|c| c.found)
        .unwrap_or_default())
}

/// The person followed on the Home Assistant `instance`; `None` before the person
/// chose.
#[must_use]
pub(crate) fn chosen(instance: &Instance) -> Option<String> {
    instance
        .config
        .get("person")
        .and_then(Value::as_str)
        .map(str::to_owned)
}

/// Follows the presence of `person` (an entity like `person.ines`) on the Home
/// Assistant `id`, then syncs it.
///
/// # Errors
///
/// [`Error::Refused`] when no Home Assistant is connected under `id`, or `person` was
/// not found on it; errors of the memory, the history or the keychain.
pub(crate) fn choose(
    vault: &Vault,
    id: &str,
    person: &str,
    secrets: &dyn SecretStore,
) -> Result<Synced> {
    let root = vault.root();
    let mut instances = Instances::load(root)?;
    let mut instance = source(&instances, id)?;
    if !found(vault, id)?.iter().any(|f| f.entity == person) {
        return Err(Error::Refused(format!(
            "{person:?} is not a person of this Home Assistant."
        )));
    }
    instance
        .config
        .insert("person".to_owned(), Value::String(person.to_owned()));
    instances.set_config(id, instance.config.clone())?;
    let history = vault.history()?;
    let mut change = history.change(Source::Person);
    change.touched(&root.join(INSTANCES_FILE));
    change.commit("Choose whom to follow in Home Assistant")?;
    sync_once(vault, &instance, secrets, false)
}

/// Stops following the Home Assistant `id`: its token leaves the keychain, what it
/// brought stays.
///
/// # Errors
///
/// [`Error::Refused`] when no Home Assistant is connected under `id`; errors of the
/// keychain, the list of sources or the history.
pub(crate) fn disconnect(vault: &Vault, id: &str, secrets: &dyn SecretStore) -> Result<Instance> {
    let root = vault.root();
    let mut instances = Instances::load(root)?;
    let instance = source(&instances, id)?;
    instances.remove(id)?;
    forget(root, &instance, secrets)?;
    let history = vault.history()?;
    let mut change = history.change(Source::Person);
    change.touched(&root.join(INSTANCES_FILE));
    change.commit("Disconnect Home Assistant")?;
    Ok(instance)
}

/// `ipse homeassistant add`: connects Home Assistant, asking its token, and follows
/// `person`, or the only person it has.
pub(crate) fn add(
    address: &str,
    person: Option<&str>,
    name: Option<&str>,
    every: Option<&str>,
    vault: Option<PathBuf>,
) -> Result<()> {
    let every = match every {
        None => DEFAULT_EVERY,
        Some(text) => duration(text).filter(|d| *d >= MIN_EVERY).ok_or_else(|| {
            Error::Refused(
                "--every takes a duration of at least 1 minute, like 5m or 15m.".to_owned(),
            )
        })?,
    };
    let vault = open(vault)?;
    let token = ask_secret("Long-lived access token", "the token")?;
    let store = secrets();
    let Connected {
        instance,
        found,
        plain,
    } = match connect(&vault, address, &token, name, every, &*store) {
        Ok(connected) => connected,
        Err(NotConnected::BadAddress(message)) => return Err(Error::Refused(message)),
        Err(NotConnected::Already(name)) => {
            return Err(Error::Refused(format!(
                "This Home Assistant is already connected, as {name:?}."
            )));
        }
        Err(NotConnected::Failed(synced)) => {
            return Err(Error::Refused(outcome_lines(&synced).join(" ")));
        }
        Err(NotConnected::Other(error)) => return Err(error),
    };
    if plain {
        println!(
            "This address uses http://, so the token goes unencrypted over your home network. Use https:// if your Home Assistant offers it."
        );
    }
    let person = match (person, found.as_slice()) {
        (Some(person), _) => person.to_owned(),
        (None, [only]) => only.entity.clone(),
        (None, _) => {
            disconnect(&vault, &instance.id, &*store)?;
            let people: Vec<String> = found
                .iter()
                .map(|f| format!("{} ({})", f.entity, f.name))
                .collect();
            return Err(Error::Refused(if people.is_empty() {
                "This Home Assistant has no person. Add yourself in Settings → People, then connect again.".to_owned()
            } else {
                format!("Say whom to follow with --person: {}.", people.join(", "))
            }));
        }
    };
    let synced = match choose(&vault, &instance.id, &person, &*store) {
        Ok(synced) => synced,
        Err(e) => {
            disconnect(&vault, &instance.id, &*store)?;
            return Err(e);
        }
    };
    for line in outcome_lines(&synced) {
        println!("{line}");
    }
    println!(
        "Connected {:?} ({}), following {person}. It syncs when `ipse sync --due` runs, or on its own while the app is open.",
        instance.name, instance.id
    );
    Ok(())
}

/// `ipse homeassistant list`: the Home Assistants connected and when they last synced.
pub(crate) fn list(vault: Option<PathBuf>) -> Result<()> {
    let vault = open(vault)?;
    let instances = Instances::load(vault.root())?;
    let state = SyncState::load(vault.root())?;
    let zone = vault.settings().time_zone().clone();
    let mut any = false;
    for instance in sources(&instances) {
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
        let person = chosen(instance).unwrap_or_else(|| "nobody followed".to_owned());
        println!("{}  {}  {person}  {last}", instance.id, instance.name);
    }
    if !any {
        println!(
            "No Home Assistant connected yet. Connect one with `ipse homeassistant add <address>`."
        );
    }
    Ok(())
}

/// `ipse homeassistant remove`: stops syncing Home Assistant. What it brought stays.
pub(crate) fn remove(id: &str, vault: Option<PathBuf>) -> Result<()> {
    let vault = open(vault)?;
    let instance = disconnect(&vault, id, &*secrets())?;
    println!(
        "Disconnected {:?}. What it brought stays in your memory.",
        instance.name
    );
    Ok(())
}

fn sync_once(
    vault: &Vault,
    instance: &Instance,
    secrets: &dyn SecretStore,
    full: bool,
) -> Result<Synced> {
    let options = SyncOptions {
        command: connector_command()?,
        language: "en".to_owned(),
        full,
        now: Timestamp::now(),
    };
    sync_instance(vault, instance, secrets, &options)
}

fn open(vault: Option<PathBuf>) -> Result<Vault> {
    Vault::open(&vault.unwrap_or_else(|| PathBuf::from(".")))
}

fn sources(instances: &Instances) -> impl Iterator<Item = &Instance> {
    instances
        .list()
        .iter()
        .filter(|i| i.connector == homeassistant::ID)
}

fn source(instances: &Instances, id: &str) -> Result<Instance> {
    instances
        .get(id)
        .filter(|i| i.connector == homeassistant::ID)
        .cloned()
        .ok_or_else(|| {
            Error::Refused(format!(
                "There is no Home Assistant {id:?}. `ipse homeassistant list` shows them."
            ))
        })
}

/// Removes what Ipse kept for `instance` outside the list: its token and its last run.
fn forget(root: &Path, instance: &Instance, secrets: &dyn SecretStore) -> Result<()> {
    secrets.delete(&secret_key(root, &instance.id, TOKEN_SECRET))?;
    SyncState::load(root)?.forget(&instance.id)
}
