//! Syncing the sources the person subscribed to (`docs/spec/connectors.md`, section 7):
//! runs an instance's connector in mode `sync`, turns what it sent into vault files,
//! records the change in the history and remembers when it ran.

use std::collections::BTreeMap;

use jiff::Timestamp;

use crate::Result;
use crate::connector::{
    Instance, Launch, Outcome, RunRecord, SecretStore, SyncState, record_run, run_session,
    secret_key,
};
use crate::journal::Journal;
use crate::materialize::{
    EventsSummary, HomeSummary, PHOTOS, PRESENCE, PeopleSummary, PhotosSummary, WATCHED,
    WatchingSummary, calendar_to_events, contacts_to_people, home_to_vault, photos_to_vault,
    watching_to_vault,
};
use crate::tasks::Tasks;
use crate::vault::{DateTime, Source, Vault};

/// What the core needs to know about a built-in connector that syncs.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Syncable {
    /// The connector: `calendar`.
    pub connector: &'static str,
    /// The record kinds it sends.
    pub kinds: &'static [&'static str],
    /// The secrets it reads from the keychain.
    pub secrets: &'static [&'static str],
}

/// The connectors that sync, built into Ipse.
pub const SYNCABLE: [Syncable; 5] = [
    Syncable {
        connector: "calendar",
        kinds: &["calendar.event"],
        secrets: &["url"],
    },
    // An account on a CalDAV and CardDAV server: its agenda and its contacts
    // (`docs/connectors/dav.md`).
    Syncable {
        connector: "dav",
        kinds: &["calendar.event", "contact.card"],
        secrets: &["password"],
    },
    // The person's own Home Assistant: when they are at home, and in which zones
    // (`docs/connectors/homeassistant.md`).
    Syncable {
        connector: "homeassistant",
        kinds: &[PRESENCE],
        secrets: &["token"],
    },
    // The person's own Jellyfin: the films and series they watch
    // (`docs/connectors/jellyfin.md`).
    Syncable {
        connector: "jellyfin",
        kinds: &[WATCHED],
        secrets: &["key"],
    },
    // The person's own Immich: who is on their photos (`docs/connectors/immich.md`).
    Syncable {
        connector: "immich",
        kinds: &[PHOTOS],
        secrets: &["key"],
    },
];

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
    /// The stays at home and in zones, for Home Assistant.
    pub home: Option<HomeSummary>,
    /// The films and episodes watched, for Jellyfin.
    pub watching: Option<WatchingSummary>,
    /// The people of the photos, for Immich.
    pub photos: Option<PhotosSummary>,
    /// Tasks made for the person's AI from what the sync brought.
    pub tasks: usize,
    /// Possible duplicates proposed to the person after the sync.
    pub duplicates: usize,
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

/// What the records of the servers followed for one of their people became: the
/// `home`, `watching` and `photos` state logs.
type StateLogs = (
    Option<HomeSummary>,
    Option<WatchingSummary>,
    Option<PhotosSummary>,
);

/// Turns the records of `kinds` that make state logs into them.
fn to_state_logs(
    vault: &Vault,
    journal: &Journal,
    instance: &str,
    kinds: &[&str],
    change: &mut crate::vault::Change<'_>,
) -> Result<StateLogs> {
    let home = if kinds.contains(&PRESENCE) {
        Some(home_to_vault(vault, journal, instance, change)?)
    } else {
        None
    };
    let watching = if kinds.contains(&WATCHED) {
        Some(watching_to_vault(vault, journal, instance, change)?)
    } else {
        None
    };
    let photos = if kinds.contains(&PHOTOS) {
        Some(photos_to_vault(vault, journal, instance, change)?)
    } else {
        None
    };
    Ok((home, watching, photos))
}

/// Syncs `instance` of the vault: runs its connector, writes what it sent to the
/// journal, then to the vault, and records the run in `.ipse/sync.toml` and in the log
/// of runs ([`record_run`]).
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
    launch.kinds = Some(syncable.kinds.iter().map(|&k| k.to_owned()).collect());
    launch.config.clone_from(&instance.config);
    launch.full = options.full;

    let mut journal = Journal::new(root);
    let report = run_session(&mut journal, &launch)?;

    // The person's own edits are recorded first, so that the sync never hides them.
    let history = vault.history()?;
    let mut change = history.change(Source::Connector(instance.id.clone()));
    let kinds = syncable.kinds;
    let events = if kinds.contains(&"calendar.event") {
        Some(calendar_to_events(
            vault,
            &journal,
            &instance.id,
            &mut change,
            options.now,
        )?)
    } else {
        None
    };
    let people = if kinds.contains(&"contact.card") {
        Some(contacts_to_people(
            vault,
            &journal,
            &instance.id,
            &mut change,
        )?)
    } else {
        None
    };
    let (home, watching, photos) =
        to_state_logs(vault, &journal, &instance.id, kinds, &mut change)?;
    let touched = change.paths();
    change.commit(&format!("Sync {}", instance.id))?;
    let tasks = Tasks::new(vault).extract_from(&touched)?;

    let mut run = RunRecord::new(
        DateTime::at(options.now, vault.settings().time_zone()).to_string(),
        &instance.id,
        "sync",
        &report,
        journal.recovered(),
    );
    if let Some(events) = &events {
        run.events(events);
    }
    if let Some(people) = &people {
        run.people(people);
    }
    run.tasks = tasks as u64;
    record_run(root, &run)?;

    let duplicates = crate::duplicates::propose(vault)?;
    let synced = Synced {
        duplicates,
        outcome: report.outcome,
        removed: report.removed,
        progress: report.progress,
        events,
        people,
        home,
        watching,
        photos,
        tasks,
    };
    SyncState::load(root)?.record(&instance.id, options.now, synced.succeeded())?;
    Ok(synced)
}
