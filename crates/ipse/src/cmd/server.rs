//! A self-hosted server followed with a token, and one of its people chosen: Home
//! Assistant and its person (`ipse homeassistant`), Jellyfin and its user
//! (`ipse jellyfin`). Connect, choose, list and remove, for the command line and the
//! Sources screen.
//!
//! The token goes straight to the system keychain: it is never written to a file, a
//! log, the history or a message.

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
use crate::connectors::rest::{self, Found, Service};
use crate::keychain::secrets;

/// What differs between the servers.
pub(crate) struct Kind {
    /// The connector: `homeassistant`.
    pub(crate) connector: &'static str,
    /// What its messages say of the service.
    pub(crate) service: Service,
    /// The secret holding the token: `token`.
    pub(crate) secret: &'static str,
    /// The config key of whom is followed: `person`.
    pub(crate) key: &'static str,
    /// What the terminal asks: `Long-lived access token`.
    pub(crate) prompt: &'static str,
    /// The token, in an error: `the token`.
    pub(crate) token: &'static str,
    /// Whom is followed, in a message: `person`, `user`.
    pub(crate) who: &'static str,
    /// What to do when the server has nobody to follow.
    pub(crate) nobody: &'static str,
    /// The source's name when the person gives none.
    pub(crate) default_name: &'static str,
    /// How often it syncs by default, and at most.
    pub(crate) every: SignedDuration,
    pub(crate) min_every: SignedDuration,
}

/// The servers Ipse follows this way.
static KINDS: [&Kind; 3] = [
    &super::homeassistant::KIND,
    &super::jellyfin::KIND,
    &super::immich::KIND,
];

/// The server behind `connector`, when it is one.
#[must_use]
pub(crate) fn kind_of(connector: &str) -> Option<&'static Kind> {
    KINDS.into_iter().find(|k| k.connector == connector)
}

/// A server now connected, with the people found on it.
pub(crate) struct Connected {
    pub(crate) instance: Instance,
    pub(crate) found: Vec<Found>,
    /// Its address is `http://`, on the home network: the token is sent unencrypted
    /// there.
    pub(crate) plain: bool,
}

/// Why a server is not connected.
pub(crate) enum NotConnected {
    /// The address or the token cannot be used; why, in English.
    BadAddress(String),
    /// The same server is already connected, under this name.
    Already(String),
    /// The server answered, but not as the service Ipse can read.
    Failed(Box<Synced>),
    /// The memory, the history or the keychain failed.
    Other(Error),
}

impl From<Error> for NotConnected {
    fn from(error: Error) -> Self {
        Self::Other(error)
    }
}

/// Connects the server at `address`: keeps the token in `secrets`, finds its people,
/// and keeps the source when that worked. Nothing is read until [`choose`].
pub(crate) fn connect(
    kind: &Kind,
    vault: &Vault,
    address: &str,
    token: &str,
    name: Option<&str>,
    every: SignedDuration,
    secrets: &dyn SecretStore,
) -> std::result::Result<Connected, NotConnected> {
    let (url, plain) = rest::address(address, &kind.service)
        .map_err(|f| NotConnected::BadAddress(f.message(false)))?;
    let token = token.trim();
    if token.is_empty() {
        return Err(NotConnected::BadAddress(format!(
            "Give {} of your {}.",
            kind.token, kind.service.name
        )));
    }
    let root = vault.root();
    let mut instances = Instances::load(root)?;
    if let Some(existing) = sources(kind, &instances)
        .find(|i| i.config.get("url").and_then(Value::as_str) == Some(&url))
    {
        return Err(NotConnected::Already(existing.name.clone()));
    }
    let mut instance =
        instances.new_instance(kind.connector, name.unwrap_or(kind.default_name), every)?;
    instance.config.insert("url".to_owned(), Value::String(url));
    secrets.set(
        &secret_key(root, &instance.id, kind.secret),
        &Secret::new(token),
    )?;
    let synced = match sync_once(vault, &instance, secrets, true) {
        Ok(synced) => synced,
        Err(e) => {
            forget(kind, root, &instance, secrets)?;
            return Err(NotConnected::Other(e));
        }
    };
    if !matches!(synced.outcome, Outcome::Done(_)) {
        forget(kind, root, &instance, secrets)?;
        return Err(NotConnected::Failed(Box::new(synced)));
    }
    let found = found(vault, &instance.id)?;
    instances.add(instance.clone())?;
    let history = vault.history()?;
    let mut change = history.change(Source::Person)?;
    change.touched(&root.join(INSTANCES_FILE));
    change.commit(&format!("Connect {}", kind.service.name))?;
    Ok(Connected {
        instance,
        found,
        plain,
    })
}

/// The people found on the server `id` at its last session, from the connector's
/// cursor (`found`).
///
/// # Errors
///
/// Errors reading the journal.
pub(crate) fn found(vault: &Vault, id: &str) -> Result<Vec<Found>> {
    let cursor = Journal::new(vault.root()).cursor(id)?;
    Ok(cursor
        .as_ref()
        .and_then(|c| c.get("found"))
        .and_then(|f| serde_json::from_value::<Vec<Found>>(f.clone()).ok())
        .unwrap_or_default())
}

/// Whom `instance` follows; `None` before the person chose.
#[must_use]
pub(crate) fn chosen(kind: &Kind, instance: &Instance) -> Option<String> {
    instance
        .config
        .get(kind.key)
        .and_then(Value::as_str)
        .map(str::to_owned)
}

/// Follows `who` (one of [`found`]) on the server `id`, then syncs it.
///
/// # Errors
///
/// [`Error::Refused`] when no such server is connected under `id`, or `who` was not
/// found on it; errors of the memory, the history or the keychain.
pub(crate) fn choose(
    kind: &Kind,
    vault: &Vault,
    id: &str,
    who: &str,
    secrets: &dyn SecretStore,
) -> Result<Synced> {
    let root = vault.root();
    let mut instances = Instances::load(root)?;
    let mut instance = source(kind, &instances, id)?;
    if !found(vault, id)?.iter().any(|f| f.id == who) {
        return Err(Error::Refused(format!(
            "{who:?} is not a {} of this {}.",
            kind.who, kind.service.name
        )));
    }
    instance
        .config
        .insert(kind.key.to_owned(), Value::String(who.to_owned()));
    instances.set_config(id, instance.config.clone())?;
    let history = vault.history()?;
    let mut change = history.change(Source::Person)?;
    change.touched(&root.join(INSTANCES_FILE));
    change.commit(&format!("Choose whom to follow in {}", kind.service.name))?;
    sync_once(vault, &instance, secrets, false)
}

/// The config key that asks Home Assistant for the phone's positions too (issue #208).
pub(crate) const POSITIONS: &str = "positions";

/// Whether the server `instance` also reads the phone's positions.
pub(crate) fn reads_positions(instance: &Instance) -> bool {
    instance.config.get(POSITIONS).and_then(Value::as_bool) == Some(true)
}

/// Reads, or stops reading, the positions of the phone from Home Assistant `id`
/// (issue #208), then syncs it. The stays already in the memory stay.
///
/// # Errors
///
/// [`Error::Refused`] when no Home Assistant is connected under `id`; errors of the
/// memory, the history or the keychain.
pub(crate) fn set_positions(
    vault: &Vault,
    id: &str,
    on: bool,
    secrets: &dyn SecretStore,
) -> Result<Option<Synced>> {
    let kind = &super::homeassistant::KIND;
    let root = vault.root();
    let mut instances = Instances::load(root)?;
    let mut instance = source(kind, &instances, id)?;
    if reads_positions(&instance) == on {
        return Ok(None);
    }
    if on {
        instance
            .config
            .insert(POSITIONS.to_owned(), Value::Bool(true));
    } else {
        instance.config.remove(POSITIONS);
    }
    instances.set_config(id, instance.config.clone())?;
    let history = vault.history()?;
    let mut change = history.change(Source::Person)?;
    change.touched(&root.join(INSTANCES_FILE));
    change.commit(if on {
        "Read the phone's positions from Home Assistant"
    } else {
        "Stop reading the phone's positions from Home Assistant"
    })?;
    sync_once(vault, &instance, secrets, false).map(Some)
}

/// Stops following the server `id`: its token leaves the keychain, what it brought
/// stays.
///
/// # Errors
///
/// [`Error::Refused`] when no such server is connected under `id`; errors of the
/// keychain, the list of sources or the history.
pub(crate) fn disconnect(
    kind: &Kind,
    vault: &Vault,
    id: &str,
    secrets: &dyn SecretStore,
) -> Result<Instance> {
    let root = vault.root();
    let mut instances = Instances::load(root)?;
    let instance = source(kind, &instances, id)?;
    instances.remove(id)?;
    forget(kind, root, &instance, secrets)?;
    let history = vault.history()?;
    let mut change = history.change(Source::Person)?;
    change.touched(&root.join(INSTANCES_FILE));
    change.commit(&format!("Disconnect {}", kind.service.name))?;
    Ok(instance)
}

/// `ipse <server> add`: connects the server, asking its token, and follows `who`, or
/// the only one it has.
pub(crate) fn add(
    kind: &Kind,
    address: &str,
    who: Option<&str>,
    name: Option<&str>,
    every: Option<&str>,
    vault: Option<PathBuf>,
) -> Result<()> {
    let every = match every {
        None => kind.every,
        Some(text) => duration(text)
            .filter(|d| *d >= kind.min_every)
            .ok_or_else(|| {
                let least = match kind.min_every.as_mins() {
                    1 => "1 minute".to_owned(),
                    n => format!("{n} minutes"),
                };
                Error::Refused(format!(
                    "--every takes a duration of at least {least}, like 5m or 15m."
                ))
            })?,
    };
    let vault = open(vault)?;
    let token = ask_secret(kind.prompt, kind.token)?;
    let store = secrets();
    let name_of = kind.service.name;
    let Connected {
        instance,
        found,
        plain,
    } = match connect(kind, &vault, address, &token, name, every, &*store) {
        Ok(connected) => connected,
        Err(NotConnected::BadAddress(message)) => return Err(Error::Refused(message)),
        Err(NotConnected::Already(name)) => {
            return Err(Error::Refused(format!(
                "This {name_of} is already connected, as {name:?}."
            )));
        }
        Err(NotConnected::Failed(synced)) => {
            return Err(Error::Refused(outcome_lines(&synced).join(" ")));
        }
        Err(NotConnected::Other(error)) => return Err(error),
    };
    if plain {
        println!(
            "This address uses http://, so the token goes unencrypted over your home network. Use https:// if your {name_of} offers it."
        );
    }
    let who = match (who, found.as_slice()) {
        (Some(who), _) => who.to_owned(),
        (None, [only]) => only.id.clone(),
        (None, _) => {
            disconnect(kind, &vault, &instance.id, &*store)?;
            let people: Vec<String> = found
                .iter()
                .map(|f| format!("{} ({})", f.id, f.name))
                .collect();
            return Err(Error::Refused(if people.is_empty() {
                kind.nobody.to_owned()
            } else {
                format!(
                    "Say whom to follow with --{}: {}.",
                    kind.who,
                    people.join(", ")
                )
            }));
        }
    };
    let synced = match choose(kind, &vault, &instance.id, &who, &*store) {
        Ok(synced) => synced,
        Err(e) => {
            disconnect(kind, &vault, &instance.id, &*store)?;
            return Err(e);
        }
    };
    for line in outcome_lines(&synced) {
        println!("{line}");
    }
    println!(
        "Connected {:?} ({}), following {who}. It syncs when `ipse sync --due` runs, or on its own while the app is open.",
        instance.name, instance.id
    );
    Ok(())
}

/// `ipse <server> list`: the servers connected and when they last synced.
pub(crate) fn list(kind: &Kind, vault: Option<PathBuf>) -> Result<()> {
    let vault = open(vault)?;
    let instances = Instances::load(vault.root())?;
    let state = SyncState::load(vault.root())?;
    let zone = vault.settings().time_zone().clone();
    let mut any = false;
    for instance in sources(kind, &instances) {
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
        let who = chosen(kind, instance).unwrap_or_else(|| "nobody followed".to_owned());
        println!("{}  {}  {who}  {last}", instance.id, instance.name);
    }
    if !any {
        println!(
            "No {} connected yet. Connect one with `ipse {} add <address>`.",
            kind.service.name, kind.connector
        );
    }
    Ok(())
}

/// `ipse <server> remove`: stops syncing the server. What it brought stays.
pub(crate) fn remove(kind: &Kind, id: &str, vault: Option<PathBuf>) -> Result<()> {
    let vault = open(vault)?;
    let instance = disconnect(kind, &vault, id, &*secrets())?;
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

fn sources<'a>(kind: &Kind, instances: &'a Instances) -> impl Iterator<Item = &'a Instance> {
    let connector = kind.connector;
    instances
        .list()
        .iter()
        .filter(move |i| i.connector == connector)
}

fn source(kind: &Kind, instances: &Instances, id: &str) -> Result<Instance> {
    instances
        .get(id)
        .filter(|i| i.connector == kind.connector)
        .cloned()
        .ok_or_else(|| {
            Error::Refused(format!(
                "There is no {} {id:?}. `ipse {} list` shows them.",
                kind.service.name, kind.connector
            ))
        })
}

/// Removes what Ipse kept for `instance` outside the list: its token and its last run.
fn forget(kind: &Kind, root: &Path, instance: &Instance, secrets: &dyn SecretStore) -> Result<()> {
    secrets.delete(&secret_key(root, &instance.id, kind.secret))?;
    SyncState::load(root)?.forget(&instance.id)
}
