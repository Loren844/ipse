//! `ipse dav`: connect a CalDAV and CardDAV account, list and remove accounts
//! (`docs/connectors/dav.md`).
//!
//! The app password goes straight to the system keychain: it is never written to a
//! file, a log, the history or a message.

use std::io::{BufRead as _, IsTerminal as _, Write as _};
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

use super::calendar::DEFAULT_EVERY;
use super::sync::{connector_command, outcome_lines};
use crate::connectors::dav::{self, Cursor, Found, PASSWORD_SECRET};
use crate::keychain::secrets;

/// Servers are asked at most this often.
const MIN_EVERY: SignedDuration = SignedDuration::from_mins(5);

/// An account now connected, with the calendars and address books found on it.
pub(crate) struct Connected {
    pub(crate) instance: Instance,
    pub(crate) found: Vec<Found>,
}

/// Why an account is not connected.
pub(crate) enum NotConnected {
    /// The address cannot be used; why, in English.
    BadAddress(String),
    /// The same account is already connected, under this name.
    Already(String),
    /// The server answered, but not as an account Ipse can read; what the session did.
    Failed(Box<Synced>),
    /// The memory, the history or the keychain failed.
    Other(Error),
}

impl From<Error> for NotConnected {
    fn from(error: Error) -> Self {
        Self::Other(error)
    }
}

/// Connects the account `user` at `address`: keeps the password in `secrets`, finds
/// the calendars and address books, and keeps the account when that worked. Nothing is
/// followed until [`choose`].
pub(crate) fn connect(
    vault: &Vault,
    address: &str,
    user: &str,
    password: &str,
    name: Option<&str>,
    every: SignedDuration,
    secrets: &dyn SecretStore,
) -> std::result::Result<Connected, NotConnected> {
    let (url, _) = dav::address(address).map_err(|f| NotConnected::BadAddress(f.message(false)))?;
    let user = user.trim();
    if user.is_empty() || password.is_empty() {
        return Err(NotConnected::BadAddress(
            "Give the user name and the app password of the account.".to_owned(),
        ));
    }
    let root = vault.root();
    let mut instances = Instances::load(root)?;
    if let Some(existing) = accounts(&instances).find(|i| {
        i.config.get("url").and_then(Value::as_str) == Some(url.as_str())
            && i.config.get("user").and_then(Value::as_str) == Some(user)
    }) {
        return Err(NotConnected::Already(existing.name.clone()));
    }

    let mut instance = instances.new_instance(dav::ID, name.unwrap_or("Account"), every)?;
    instance.config.insert("url".to_owned(), Value::String(url));
    instance
        .config
        .insert("user".to_owned(), Value::String(user.to_owned()));
    secrets.set(
        &secret_key(root, &instance.id, PASSWORD_SECRET),
        &Secret::new(password),
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
    change.commit("Connect a calendars and contacts account")?;
    Ok(Connected { instance, found })
}

/// The calendars and address books found on the account `id` at its last session.
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

/// The collections followed on the account `id`; `None` before the person chose.
#[must_use]
pub(crate) fn chosen(instance: &Instance) -> Option<Vec<String>> {
    instance
        .config
        .get("collections")
        .and_then(Value::as_array)
        .map(|items| {
            items
                .iter()
                .filter_map(Value::as_str)
                .map(str::to_owned)
                .collect()
        })
}

/// Follows `collections` of the account `id`, then syncs it.
///
/// # Errors
///
/// [`Error::Refused`] when no account is connected under `id`, or a collection was
/// not found on it; errors of the memory, the history or the keychain.
pub(crate) fn choose(
    vault: &Vault,
    id: &str,
    collections: &[String],
    secrets: &dyn SecretStore,
) -> Result<Synced> {
    let root = vault.root();
    let mut instances = Instances::load(root)?;
    let mut instance = account(&instances, id)?;
    let found = found(vault, id)?;
    if let Some(unknown) = collections
        .iter()
        .find(|c| !found.iter().any(|f| &f.url == *c))
    {
        return Err(Error::Refused(format!(
            "{unknown:?} is not a calendar or an address book of this account."
        )));
    }
    let list = collections.iter().cloned().map(Value::String).collect();
    instance
        .config
        .insert("collections".to_owned(), Value::Array(list));
    instances.set_config(id, instance.config.clone())?;
    let history = vault.history()?;
    let mut change = history.change(Source::Person);
    change.touched(&root.join(INSTANCES_FILE));
    change.commit("Choose the calendars and address books to follow")?;
    sync_once(vault, &instance, secrets, false)
}

/// Stops following the account `id`: its password leaves the keychain, its events and
/// contacts stay.
///
/// # Errors
///
/// [`Error::Refused`] when no account is connected under `id`; errors of the keychain,
/// the list of sources or the history.
pub(crate) fn disconnect(vault: &Vault, id: &str, secrets: &dyn SecretStore) -> Result<Instance> {
    let root = vault.root();
    let mut instances = Instances::load(root)?;
    let instance = account(&instances, id)?;
    instances.remove(id)?;
    forget(root, &instance, secrets)?;
    let history = vault.history()?;
    let mut change = history.change(Source::Person);
    change.touched(&root.join(INSTANCES_FILE));
    change.commit("Disconnect a calendars and contacts account")?;
    Ok(instance)
}

/// `ipse dav add`: connects the account, asking its password, and follows every
/// calendar and address book found.
pub(crate) fn add(
    address: &str,
    user: &str,
    name: Option<&str>,
    every: Option<&str>,
    vault: Option<PathBuf>,
) -> Result<()> {
    let every = match every {
        None => DEFAULT_EVERY,
        Some(text) => duration(text).filter(|d| *d >= MIN_EVERY).ok_or_else(|| {
            Error::Refused(
                "--every takes a duration of at least 5 minutes, like 15m or 1h.".to_owned(),
            )
        })?,
    };
    let vault = open(vault)?;
    let password = ask_password()?;
    let store = secrets();
    let Connected { instance, found } =
        match connect(&vault, address, user, &password, name, every, &*store) {
            Ok(connected) => connected,
            Err(NotConnected::BadAddress(message)) => return Err(Error::Refused(message)),
            Err(NotConnected::Already(name)) => {
                return Err(Error::Refused(format!(
                    "This account is already connected, as {name:?}."
                )));
            }
            Err(NotConnected::Failed(synced)) => {
                return Err(Error::Refused(outcome_lines(&synced).join(" ")));
            }
            Err(NotConnected::Other(error)) => return Err(error),
        };
    for f in &found {
        let kind = match f.kind {
            dav::Kind::Calendar => "calendar",
            dav::Kind::Contacts => "address book",
        };
        println!("Found the {kind} {:?}.", f.name);
    }
    let all: Vec<String> = found.iter().map(|f| f.url.clone()).collect();
    let synced = choose(&vault, &instance.id, &all, &*store)?;
    for line in outcome_lines(&synced) {
        println!("{line}");
    }
    println!(
        "Connected {:?} ({}). It syncs when `ipse sync --due` runs, or on its own while the app is open.",
        instance.name, instance.id
    );
    Ok(())
}

/// `ipse dav list`: the accounts and when they last synced.
pub(crate) fn list(vault: Option<PathBuf>) -> Result<()> {
    let vault = open(vault)?;
    let instances = Instances::load(vault.root())?;
    let state = SyncState::load(vault.root())?;
    let zone = vault.settings().time_zone().clone();
    let mut any = false;
    for instance in accounts(&instances) {
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
        let followed = chosen(instance).map_or(0, |c| c.len());
        println!(
            "{}  {}  {followed} followed  {last}",
            instance.id, instance.name
        );
    }
    if !any {
        println!(
            "No account connected yet. Connect one with `ipse dav add <address> --user <name>`."
        );
    }
    Ok(())
}

/// `ipse dav remove`: stops syncing an account. Its events and contacts stay.
pub(crate) fn remove(id: &str, vault: Option<PathBuf>) -> Result<()> {
    let vault = open(vault)?;
    let instance = disconnect(&vault, id, &*secrets())?;
    println!(
        "Disconnected {:?}. Its events and contacts stay in your memory.",
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

/// Reads the password from the terminal without showing it, or from the input.
fn ask_password() -> Result<String> {
    let stdin = std::io::stdin();
    let terminal = stdin.is_terminal();
    if terminal {
        eprint!("App password (not shown): ");
        let _ = std::io::stderr().flush();
    }
    let hidden = terminal && echo(false);
    let mut line = String::new();
    let read = stdin.lock().read_line(&mut line);
    if hidden {
        echo(true);
        eprintln!();
    }
    read.map_err(|source| Error::Io {
        action: "read",
        path: PathBuf::from("the password"),
        source,
    })?;
    let password = line.trim_end_matches(['\r', '\n']).to_owned();
    if password.is_empty() {
        return Err(Error::Refused("No password given.".to_owned()));
    }
    Ok(password)
}

/// Turns the terminal's echo on or off; `false` when it cannot.
fn echo(on: bool) -> bool {
    if cfg!(unix) {
        std::process::Command::new("stty")
            .arg(if on { "echo" } else { "-echo" })
            .stdin(std::process::Stdio::inherit())
            .status()
            .is_ok_and(|s| s.success())
    } else {
        false
    }
}

fn open(vault: Option<PathBuf>) -> Result<Vault> {
    Vault::open(&vault.unwrap_or_else(|| PathBuf::from(".")))
}

fn accounts(instances: &Instances) -> impl Iterator<Item = &Instance> {
    instances.list().iter().filter(|i| i.connector == dav::ID)
}

fn account(instances: &Instances, id: &str) -> Result<Instance> {
    instances
        .get(id)
        .filter(|i| i.connector == dav::ID)
        .cloned()
        .ok_or_else(|| {
            Error::Refused(format!(
                "There is no account {id:?}. `ipse dav list` shows them."
            ))
        })
}

/// Removes what Ipse kept for `instance` outside the list: its password and its last
/// run.
fn forget(root: &Path, instance: &Instance, secrets: &dyn SecretStore) -> Result<()> {
    secrets.delete(&secret_key(root, &instance.id, PASSWORD_SECRET))?;
    SyncState::load(root)?.forget(&instance.id)
}
