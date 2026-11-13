//! Connector instances the person set up to sync on their own, and when they last ran
//! (`docs/spec/connectors.md`, sections 1 and 7).
//!
//! What the person chose (which connector, its name, how often) is in
//! `.ipse/connectors.toml`, part of the vault's history. When each instance last ran
//! changes every few minutes and is in `.ipse/sync.toml`, left out of the history.
//! Secrets are in neither: they stay in the system keychain ([`SecretStore`]).

use std::cell::RefCell;
use std::collections::{BTreeMap, HashMap};
use std::fmt::Write as _;
use std::path::{Path, PathBuf};

use ipse_protocol::Secret;
use jiff::{SignedDuration, Timestamp};
use serde::Deserialize;

use crate::vault::write_atomic;
use crate::{Error, Result};

/// The instances the person set up, relative to the vault's folder.
pub const INSTANCES_FILE: &str = ".ipse/connectors.toml";

/// When each instance last ran, relative to the vault's folder.
pub const STATE_FILE: &str = ".ipse/sync.toml";

/// Waits after 1, 2, 3 and 4 or more failures in a row (`unavailable`, spec section
/// 5.9).
const BACKOFF_MINUTES: [i64; 4] = [1, 5, 15, 60];

/// One instance of a connector that syncs: "my Google calendar".
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Instance {
    /// Also its journal source: `calendar-2`.
    pub id: String,
    /// The connector: `calendar`.
    pub connector: String,
    /// The name the person gave it: "Perso".
    pub name: String,
    /// How often it syncs.
    pub every: SignedDuration,
}

#[derive(Deserialize)]
struct File {
    #[serde(default)]
    instance: Vec<RawInstance>,
}

#[derive(Deserialize)]
struct RawInstance {
    id: String,
    connector: String,
    name: String,
    every: String,
}

/// The instances of one vault.
#[derive(Debug, Clone)]
pub struct Instances {
    path: PathBuf,
    list: Vec<Instance>,
}

impl Instances {
    /// Reads the instances of the vault in `root`. No file means none.
    ///
    /// # Errors
    ///
    /// [`Error::Io`] when the file cannot be read; [`Error::Invalid`] when it is not
    /// valid.
    pub fn load(root: &Path) -> Result<Self> {
        let path = root.join(INSTANCES_FILE);
        let text = match std::fs::read_to_string(&path) {
            Ok(text) => text,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                return Ok(Self {
                    path,
                    list: Vec::new(),
                });
            }
            Err(source) => {
                return Err(Error::Io {
                    action: "read",
                    path,
                    source,
                });
            }
        };
        let invalid = |problem: &str| Error::Invalid {
            path: path.clone(),
            line: None,
            problem: problem.to_owned(),
        };
        let file: File = toml::from_str(&text)
            .map_err(|_| invalid("the list of synced sources is not valid TOML"))?;
        let list = file
            .instance
            .into_iter()
            .map(|raw| {
                let every = duration(&raw.every)
                    .ok_or_else(|| invalid("`every` must be a duration like \"15m\" or \"1h\""))?;
                Ok(Instance {
                    id: raw.id,
                    connector: raw.connector,
                    name: raw.name,
                    every,
                })
            })
            .collect::<Result<_>>()?;
        Ok(Self { path, list })
    }

    /// The instances, in the order they were added.
    #[must_use]
    pub fn list(&self) -> &[Instance] {
        &self.list
    }

    /// The instance `id`.
    #[must_use]
    pub fn get(&self, id: &str) -> Option<&Instance> {
        self.list.iter().find(|i| i.id == id)
    }

    /// Adds an instance of `connector` named `name`, and saves. Its id is the
    /// connector's with the first free number from 2 (`calendar-2`): the bare id stays
    /// for files the person imports.
    ///
    /// # Errors
    ///
    /// [`Error::Refused`] for an empty name; [`Error::Io`] when the list cannot be
    /// saved.
    pub fn add(&mut self, connector: &str, name: &str, every: SignedDuration) -> Result<Instance> {
        let name = name.trim();
        if name.is_empty() {
            return Err(Error::Refused("Give it a name, like \"Perso\".".to_owned()));
        }
        // At most `len + 1` numbers are tried, so one is always free.
        let id = (2..=self.list.len() + 2)
            .map(|n| format!("{connector}-{n}"))
            .find(|id| self.get(id).is_none())
            .unwrap_or_else(|| connector.to_owned());
        let instance = Instance {
            id,
            connector: connector.to_owned(),
            name: name.to_owned(),
            every,
        };
        self.list.push(instance.clone());
        self.save()?;
        Ok(instance)
    }

    /// Removes the instance `id` and saves. Returns `false` when there is none.
    ///
    /// # Errors
    ///
    /// [`Error::Io`] when the list cannot be saved.
    pub fn remove(&mut self, id: &str) -> Result<bool> {
        let before = self.list.len();
        self.list.retain(|i| i.id != id);
        if self.list.len() == before {
            return Ok(false);
        }
        self.save()?;
        Ok(true)
    }

    fn save(&self) -> Result<()> {
        let mut out = String::from(
            "# Sources that sync on their own. Their links and passwords are in the system keychain, never here.\n",
        );
        for i in &self.list {
            let _ = write!(
                out,
                "\n[[instance]]\nid = {}\nconnector = {}\nname = {}\nevery = {}\n",
                quote(&i.id),
                quote(&i.connector),
                quote(&i.name),
                quote(&format_duration(i.every)),
            );
        }
        write_atomic(&self.path, out.as_bytes())
    }
}

/// When each instance last ran and how many times in a row it failed.
#[derive(Debug, Clone)]
pub struct SyncState {
    path: PathBuf,
    runs: BTreeMap<String, Run>,
}

/// The last run of one instance.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
pub struct Run {
    /// When it last ran, successful or not.
    pub at: Timestamp,
    /// Failures in a row; 0 after a success.
    #[serde(default)]
    pub failures: u32,
}

impl SyncState {
    /// Reads the state of the vault in `root`. No file, or a damaged one, means no run
    /// yet: every instance is due.
    ///
    /// # Errors
    ///
    /// [`Error::Io`] when the file exists but cannot be read.
    pub fn load(root: &Path) -> Result<Self> {
        let path = root.join(STATE_FILE);
        let runs = match std::fs::read_to_string(&path) {
            Ok(text) => toml::from_str(&text).unwrap_or_default(),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => BTreeMap::new(),
            Err(source) => {
                return Err(Error::Io {
                    action: "read",
                    path,
                    source,
                });
            }
        };
        Ok(Self { path, runs })
    }

    /// The last run of `id`.
    #[must_use]
    pub fn last(&self, id: &str) -> Option<Run> {
        self.runs.get(id).copied()
    }

    /// True when `instance` should run at `now`: never ran, its interval passed, or,
    /// after failures, the backoff of `unavailable` passed (1, 5, 15, then 60 minutes).
    #[must_use]
    pub fn due(&self, instance: &Instance, now: Timestamp) -> bool {
        let Some(run) = self.last(&instance.id) else {
            return true;
        };
        let wait = match run.failures {
            0 => instance.every,
            n => {
                let step = usize::try_from(n - 1).unwrap_or(usize::MAX).min(3);
                SignedDuration::from_mins(BACKOFF_MINUTES[step])
            }
        };
        now.duration_since(run.at) >= wait
    }

    /// Records a run of `id` at `at` and saves.
    ///
    /// # Errors
    ///
    /// [`Error::Io`] when the state cannot be saved.
    pub fn record(&mut self, id: &str, at: Timestamp, succeeded: bool) -> Result<()> {
        let failures = if succeeded {
            0
        } else {
            self.last(id).map_or(1, |r| r.failures.saturating_add(1))
        };
        self.runs.insert(id.to_owned(), Run { at, failures });
        self.save()
    }

    /// Forgets `id`, after it was removed.
    ///
    /// # Errors
    ///
    /// [`Error::Io`] when the state cannot be saved.
    pub fn forget(&mut self, id: &str) -> Result<()> {
        if self.runs.remove(id).is_some() {
            self.save()?;
        }
        Ok(())
    }

    fn save(&self) -> Result<()> {
        let mut out = String::from("# When each synced source last ran. Written by Ipse.\n");
        for (id, run) in &self.runs {
            let _ = write!(
                out,
                "\n[{}]\nat = {}\nfailures = {}\n",
                quote(id),
                quote(&run.at.to_string()),
                run.failures
            );
        }
        write_atomic(&self.path, out.as_bytes())
    }
}

/// Where secrets live: the system keychain, or memory in tests.
pub trait SecretStore {
    /// The secret stored under `key`, if any.
    ///
    /// # Errors
    ///
    /// [`Error::Refused`] when the keychain cannot be reached, saying what to do.
    fn get(&self, key: &str) -> Result<Option<Secret>>;

    /// Stores `secret` under `key`, replacing any other.
    ///
    /// # Errors
    ///
    /// As [`SecretStore::get`].
    fn set(&self, key: &str, secret: &Secret) -> Result<()>;

    /// Removes the secret stored under `key`; nothing when there is none.
    ///
    /// # Errors
    ///
    /// As [`SecretStore::get`].
    fn delete(&self, key: &str) -> Result<()>;
}

/// The keychain key of the secret `name` of `instance` in the vault at `root`, so that
/// two vaults never share secrets.
#[must_use]
pub fn secret_key(root: &Path, instance: &str, name: &str) -> String {
    let root = std::fs::canonicalize(root).unwrap_or_else(|_| root.to_owned());
    let vault = blake3::hash(root.to_string_lossy().as_bytes()).to_hex();
    format!("{}/{instance}/{name}", &vault[..16])
}

/// Secrets kept in memory, for tests.
#[derive(Debug, Default)]
pub struct MemorySecrets {
    secrets: RefCell<HashMap<String, Secret>>,
}

impl SecretStore for MemorySecrets {
    fn get(&self, key: &str) -> Result<Option<Secret>> {
        Ok(self.secrets.borrow().get(key).cloned())
    }

    fn set(&self, key: &str, secret: &Secret) -> Result<()> {
        self.secrets
            .borrow_mut()
            .insert(key.to_owned(), secret.clone());
        Ok(())
    }

    fn delete(&self, key: &str) -> Result<()> {
        self.secrets.borrow_mut().remove(key);
        Ok(())
    }
}

/// `15m`, `1h`, `1h30m`, `2 days`: a positive duration, days counting 24 hours.
#[must_use]
pub fn duration(text: &str) -> Option<SignedDuration> {
    let span: jiff::Span = text.trim().parse().ok()?;
    span.to_duration(jiff::SpanRelativeTo::days_are_24_hours())
        .ok()
        .filter(SignedDuration::is_positive)
}

/// A duration as `15m`, `1h`, `1h30m`.
fn format_duration(d: SignedDuration) -> String {
    let minutes = d.as_secs() / 60;
    match (minutes / 60, minutes % 60) {
        (0, m) => format!("{m}m"),
        (h, 0) => format!("{h}h"),
        (h, m) => format!("{h}h{m}m"),
    }
}

/// A TOML basic string. JSON's string escapes are all valid in TOML.
fn quote(text: &str) -> String {
    serde_json::to_string(text).unwrap_or_else(|_| "\"\"".to_owned())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn at(text: &str) -> Timestamp {
        text.parse().unwrap()
    }

    #[test]
    fn instances_are_added_listed_and_removed() {
        let dir = tempfile::tempdir().unwrap();
        let mut instances = Instances::load(dir.path()).unwrap();
        let perso = instances
            .add("calendar", " Perso ", SignedDuration::from_mins(15))
            .unwrap();
        let pro = instances
            .add("calendar", "Pro \"équipe\"", SignedDuration::from_mins(90))
            .unwrap();
        assert_eq!(
            (perso.id.as_str(), pro.id.as_str()),
            ("calendar-2", "calendar-3")
        );
        assert_eq!(perso.name, "Perso");

        let back = Instances::load(dir.path()).unwrap();
        assert_eq!(back.list(), instances.list());
        let text = std::fs::read_to_string(dir.path().join(INSTANCES_FILE)).unwrap();
        assert!(text.contains("every = \"1h30m\""), "{text}");

        assert!(instances.remove("calendar-2").unwrap());
        assert!(!instances.remove("calendar-2").unwrap());
        let next = instances
            .add("calendar", "Club", SignedDuration::from_hours(1))
            .unwrap();
        assert_eq!(next.id, "calendar-2", "a freed number is used again");
        assert!(
            instances
                .add("calendar", "  ", SignedDuration::from_mins(15))
                .is_err()
        );
    }

    #[test]
    fn instances_files_with_errors_are_reported() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join(INSTANCES_FILE);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(
            &path,
            "[[instance]]\nid = \"calendar-2\"\nconnector = \"calendar\"\nname = \"P\"\nevery = \"soon\"\n",
        )
        .unwrap();
        let error = Instances::load(dir.path()).unwrap_err().to_string();
        assert!(error.contains("`every` must be a duration"), "{error}");
    }

    #[test]
    fn due_after_the_interval_or_the_backoff() {
        let dir = tempfile::tempdir().unwrap();
        let instance = Instance {
            id: "calendar-2".into(),
            connector: "calendar".into(),
            name: "Perso".into(),
            every: SignedDuration::from_mins(15),
        };
        let mut state = SyncState::load(dir.path()).unwrap();
        assert!(
            state.due(&instance, at("2026-11-14T10:00:00Z")),
            "never ran"
        );

        state
            .record("calendar-2", at("2026-11-14T10:00:00Z"), true)
            .unwrap();
        let state = SyncState::load(dir.path()).unwrap();
        assert!(!state.due(&instance, at("2026-11-14T10:14:59Z")));
        assert!(state.due(&instance, at("2026-11-14T10:15:00Z")));

        let mut state = state;
        for (failures, wait) in [(1, 1), (2, 5), (3, 15), (4, 60), (5, 60)] {
            state
                .record("calendar-2", at("2026-11-14T11:00:00Z"), false)
                .unwrap();
            assert_eq!(state.last("calendar-2").unwrap().failures, failures);
            let before = at("2026-11-14T11:00:00Z")
                .checked_add(SignedDuration::from_mins(wait) - SignedDuration::from_secs(1))
                .unwrap();
            assert!(!state.due(&instance, before), "{failures}");
            let after = at("2026-11-14T11:00:00Z")
                .checked_add(SignedDuration::from_mins(wait))
                .unwrap();
            assert!(state.due(&instance, after), "{failures}");
        }
        state.forget("calendar-2").unwrap();
        assert!(
            SyncState::load(dir.path())
                .unwrap()
                .last("calendar-2")
                .is_none()
        );
    }

    #[test]
    fn a_damaged_state_means_due() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join(STATE_FILE);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(&path, "not toml [").unwrap();
        assert!(
            SyncState::load(dir.path())
                .unwrap()
                .last("calendar-2")
                .is_none()
        );
    }

    #[test]
    fn secrets_are_keyed_per_vault() {
        let a = tempfile::tempdir().unwrap();
        let b = tempfile::tempdir().unwrap();
        let key = secret_key(a.path(), "calendar-2", "url");
        assert!(key.ends_with("/calendar-2/url"));
        assert_ne!(key, secret_key(b.path(), "calendar-2", "url"));
        assert!(
            !key.contains(&*a.path().to_string_lossy()),
            "no path in the keychain's list"
        );

        let store = MemorySecrets::default();
        store
            .set(&key, &Secret::new("https://example.org/x.ics"))
            .unwrap();
        assert_eq!(
            store.get(&key).unwrap().unwrap().expose(),
            "https://example.org/x.ics"
        );
        store.delete(&key).unwrap();
        assert!(store.get(&key).unwrap().is_none());
    }

    #[test]
    fn durations() {
        assert_eq!(duration("15m"), Some(SignedDuration::from_mins(15)));
        assert_eq!(duration("2 days"), Some(SignedDuration::from_hours(48)));
        assert_eq!(duration("0m"), None);
        assert_eq!(format_duration(SignedDuration::from_mins(90)), "1h30m");
        assert_eq!(format_duration(SignedDuration::from_mins(15)), "15m");
    }
}
