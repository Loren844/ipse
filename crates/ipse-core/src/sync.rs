//! Syncing the sources the person subscribed to (`docs/spec/connectors.md`, section 7):
//! runs an instance's connector in mode `sync`, turns what it sent into vault files,
//! records the change in the history and remembers when it ran.

use std::collections::BTreeMap;

use jiff::Timestamp;

use crate::Result;
use crate::connector::{
    Instance, Launch, Outcome, SecretStore, SyncState, run_session, secret_key,
};
use crate::journal::Journal;
use crate::materialize::{EventsSummary, PeopleSummary, calendar_to_events, contacts_to_people};
use crate::vault::{Source, Vault};

/// What the core needs to know about a built-in connector that syncs.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Syncable {
    /// The connector: `calendar`.
    pub connector: &'static str,
    /// The record kind it sends.
    pub kind: &'static str,
    /// The secrets it reads from the keychain.
    pub secrets: &'static [&'static str],
}

/// The connectors that sync, built into Ipse.
pub const SYNCABLE: [Syncable; 1] = [Syncable {
    connector: "calendar",
    kind: "calendar.event",
    secrets: &["url"],
}];

/// The built-in connector `connector`, if it syncs.
#[must_use]
pub fn syncable(connector: &str) -> Option<Syncable> {
    SYNCABLE.into_iter().find(|s| s.connector == connector)
}

/// What one sync did.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Synced {
    /// How the connector's session ended.
    pub outcome: Outcome,
    /// Items the source no longer holds, recorded as removed.
    pub removed: u64,
    /// The connector's progress messages.
    pub progress: Vec<String>,
    /// What changed in the events, for a calendar.
    pub events: Option<EventsSummary>,
    /// What changed in the people, for contacts.
    pub people: Option<PeopleSummary>,
}

impl Synced {
    /// True when the connector sent everything it had to.
    #[must_use]
    pub fn succeeded(&self) -> bool {
        matches!(self.outcome, Outcome::Done(_))
    }
}

/// How to sync.
#[derive(Debug, Clone)]
pub struct SyncOptions {
    /// How to start a built-in connector, without its name: `["ipse", "connector"]`.
    pub command: Vec<String>,
    /// The language of the connector's messages: `en` for the command line.
    pub language: String,
    /// Fetch everything, ignoring what the connector remembers.
    pub full: bool,
    /// The time of the sync.
    pub now: Timestamp,
}

/// Syncs `instance` of the vault: runs its connector, writes what it sent to the
/// journal, then to the vault, and records the run in `.ipse/sync.toml`.
///
/// What the connector sent is written to the vault even when the session failed
/// midway: it is valid, and removals only come from a session that sent everything.
///
/// # Errors
///
/// [`Error::Refused`](crate::Error::Refused) for a connector that does not sync;
/// keychain errors from `secrets`; [`Error::Io`](crate::Error::Io) when the connector
/// cannot start or the journal, the vault or its history cannot be written. A failure
/// of the source itself (unreachable, link reset…) is in [`Synced::outcome`].
pub fn sync_instance(
    vault: &Vault,
    instance: &Instance,
    secrets: &dyn SecretStore,
    options: &SyncOptions,
) -> Result<Synced> {
    let syncable = syncable(&instance.connector).ok_or_else(|| {
        crate::Error::Refused(format!(
            "Ipse cannot sync {} sources: they are imported from files.",
            instance.connector
        ))
    })?;
    let root = vault.root();
    let mut found = BTreeMap::new();
    for name in syncable.secrets {
        if let Some(secret) = secrets.get(&secret_key(root, &instance.id, name))? {
            found.insert((*name).to_owned(), secret);
        }
    }

    let mut command = options.command.clone();
    command.push(instance.connector.clone());
    let mut launch = Launch::new(
        command,
        &instance.id,
        &options.language,
        vault.settings().time_zone_name(),
    );
    launch.mode = ipse_protocol::Mode::Sync;
    launch.secrets = found;
    launch.kinds = Some(vec![syncable.kind.to_owned()]);
    launch.full = options.full;

    let mut journal = Journal::new(root);
    let report = run_session(&mut journal, &launch)?;

    // The person's own edits are recorded first, so that the sync never hides them.
    let history = vault.history()?;
    let mut change = history.change(Source::Connector(instance.id.clone()));
    let (events, people) = match syncable.connector {
        "calendar" => (
            Some(calendar_to_events(
                vault,
                &journal,
                &instance.id,
                &mut change,
                options.now,
            )?),
            None,
        ),
        _ => (
            None,
            Some(contacts_to_people(
                vault,
                &journal,
                &instance.id,
                &mut change,
            )?),
        ),
    };
    change.commit(&format!("Sync {}", instance.id))?;

    let synced = Synced {
        outcome: report.outcome,
        removed: report.removed,
        progress: report.progress,
        events,
        people,
    };
    SyncState::load(root)?.record(&instance.id, options.now, synced.succeeded())?;
    Ok(synced)
}
