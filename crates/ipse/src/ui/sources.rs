//! The Sources screen's API (issue #152): what feeds the memory, adding to it and
//! following it, as `ipse import`, `ipse calendar` and `ipse sync` do.
//!
//! A calendar's secret link arrives in a request body and goes straight to the system
//! keychain: it is never written to a file, a log or an answer.

use std::path::{Path, PathBuf};

use ipse_core::connector::{Instances, SyncState, runs};
use ipse_core::rights::Gateway;
use ipse_core::sync::{SyncOptions, sync_instance};
use ipse_core::time::Timestamp;
use ipse_core::{Error, Result};
use serde::{Deserialize, Serialize};

use super::history::{RunView, run_view};
use crate::cmd::calendar::{DEFAULT_EVERY, NotFollowed, follow, unfollow};
use crate::cmd::sync::connector_command;
use crate::keychain::secrets;

/// Runs read to list the sources and their last import.
const RUNS: usize = 2000;

/// Files the screen imports, by their extension.
pub(crate) const IMPORTABLE: [&str; 6] = ["vcf", "vcard", "ics", "ical", "ifb", "json"];

/// One source of the memory.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[cfg_attr(
    test,
    derive(ts_rs::TS),
    ts(export, export_to = "../../../ui/src/lib/types/")
)]
pub(crate) struct SourceView {
    /// The source's name in the journal: `contacts`, `calendar-2`, `google-timeline`…
    instance: String,
    /// The connector behind it: `contacts`, `calendar`, `google-timeline`…
    connector: String,
    /// The name the person gave a followed source.
    name: Option<String>,
    /// Whether Ipse follows it (syncs it on its own), rather than importing files.
    followed: bool,
    /// How often a followed source syncs, in minutes.
    every_minutes: Option<u32>,
    /// Failures in a row of a followed source.
    failures: u32,
    /// Its last import or sync.
    last: Option<RunView>,
    /// How many imports and syncs it had.
    runs: u32,
}

/// What `GET /api/sources` answers, and every change of the screen.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[cfg_attr(
    test,
    derive(ts_rs::TS),
    ts(export, export_to = "../../../ui/src/lib/types/")
)]
pub(crate) struct SourcesView {
    /// Followed sources first, then the others, the most recent first.
    sources: Vec<SourceView>,
}

/// What a change of the screen did.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[cfg_attr(
    test,
    derive(ts_rs::TS),
    ts(export, export_to = "../../../ui/src/lib/types/")
)]
pub(crate) struct SourcesChange {
    /// `imported`, `followed` (and synced), `kept` (followed, the calendar could not be
    /// reached: Ipse tries again), `synced`, `unfollowed`; or why not: `not_supported`
    /// (a file Ipse cannot import), `bad_file`, `bad_link`, `already`, `failed`,
    /// `keychain`.
    outcome: String,
    /// For `already`: the name the calendar is followed under; for `unfollowed`, the
    /// name it had. For `failed`: the connector's error code, as in the runs.
    detail: Option<String>,
    /// The source the change was about, as it now is.
    source: Option<String>,
    /// Every source, after the change.
    sources: SourcesView,
}

/// `POST /api/sources/follow`.
#[derive(Debug, Deserialize)]
struct FollowBody {
    link: String,
    name: Option<String>,
}

/// `POST /api/sources/sync` and `/api/sources/unfollow`.
#[derive(Debug, Deserialize)]
struct InstanceBody {
    instance: String,
}

/// The sources of the memory.
///
/// # Errors
///
/// Errors reading the subscriptions, their state or the log of runs.
pub(crate) fn view(gateway: &Gateway) -> Result<SourcesView> {
    let root = gateway.vault().root();
    let instances = Instances::load(root)?;
    let state = SyncState::load(root)?;
    let runs = runs(root, RUNS)?;
    let mut sources: Vec<SourceView> = instances
        .list()
        .iter()
        .map(|instance| SourceView {
            instance: instance.id.clone(),
            connector: instance.connector.clone(),
            name: Some(instance.name.clone()),
            followed: true,
            every_minutes: u32::try_from(instance.every.as_secs() / 60).ok(),
            failures: state.last(&instance.id).map_or(0, |run| run.failures),
            last: None,
            runs: 0,
        })
        .collect();
    // The log is newest first: the first run of an instance is its last. A source
    // that synced and is no longer followed is gone: its runs stay in the history.
    for run in runs {
        let at = sources.iter().position(|s| s.instance == run.instance);
        let source = if let Some(at) = at {
            &mut sources[at]
        } else {
            if run.mode != "import" {
                continue;
            }
            sources.push(SourceView {
                connector: connector_of(&run.instance).to_owned(),
                instance: run.instance.clone(),
                name: None,
                followed: false,
                every_minutes: None,
                failures: 0,
                last: None,
                runs: 0,
            });
            let last = sources.len() - 1;
            &mut sources[last]
        };
        source.runs = source.runs.saturating_add(1);
        if source.last.is_none() {
            source.last = Some(run_view(run));
        }
    }
    Ok(SourcesView { sources })
}

/// The connector of an instance: `calendar-2` is a `calendar`.
fn connector_of(instance: &str) -> &str {
    match instance.rsplit_once('-') {
        Some((connector, n)) if !n.is_empty() && n.bytes().all(|b| b.is_ascii_digit()) => connector,
        _ => instance,
    }
}

/// A change of the screen: `follow`, `sync` or `unfollow`.
///
/// # Errors
///
/// [`Error::Refused`] for a body that cannot be read or a source that is not
/// followed; errors of the memory, the history or the keychain.
pub(crate) fn change(
    gateway: &mut Gateway,
    action: &str,
    body: &[u8],
) -> Result<Option<SourcesChange>> {
    match action {
        "follow" => follow_calendar(gateway, body).map(Some),
        "sync" => {
            let body: InstanceBody = read(body)?;
            sync_now(gateway, &body.instance).map(Some)
        }
        "unfollow" => {
            let body: InstanceBody = read(body)?;
            let instance = unfollow(gateway.vault(), &body.instance, &*secrets())?;
            answer(
                gateway,
                "unfollowed",
                Some(instance.name),
                Some(body.instance),
            )
            .map(Some)
        }
        _ => Ok(None),
    }
}

fn read<'a, T: Deserialize<'a>>(body: &'a [u8]) -> Result<T> {
    serde_json::from_slice(body)
        .map_err(|_| Error::Refused("The request could not be read.".to_owned()))
}

fn answer(
    gateway: &Gateway,
    outcome: &str,
    detail: Option<String>,
    source: Option<String>,
) -> Result<SourcesChange> {
    Ok(SourcesChange {
        outcome: outcome.to_owned(),
        detail,
        source,
        sources: view(gateway)?,
    })
}

/// Follows a calendar by its secret link, as `ipse calendar subscribe`.
fn follow_calendar(gateway: &mut Gateway, body: &[u8]) -> Result<SourcesChange> {
    let body: FollowBody = read(body)?;
    let name = body
        .name
        .as_deref()
        .map(str::trim)
        .filter(|n| !n.is_empty());
    let vault = gateway.vault();
    match follow(vault, body.link.trim(), name, DEFAULT_EVERY, &*secrets()) {
        Ok(followed) => {
            let outcome = if followed.synced.succeeded() {
                "followed"
            } else {
                "kept"
            };
            answer(gateway, outcome, None, Some(followed.instance.id))
        }
        Err(NotFollowed::BadLink(_)) => answer(gateway, "bad_link", None, None),
        Err(NotFollowed::Already(name)) => answer(gateway, "already", Some(name), None),
        Err(NotFollowed::Failed(_)) => {
            // The run that failed is the last one logged; its code says why.
            let code = runs(vault.root(), 1)?
                .into_iter()
                .next()
                .and_then(|run| run.code);
            answer(gateway, "failed", code, None)
        }
        Err(NotFollowed::Other(Error::Refused(_))) => answer(gateway, "keychain", None, None),
        Err(NotFollowed::Other(error)) => Err(error),
    }
}

/// Syncs a followed source now, as `ipse sync`.
fn sync_now(gateway: &mut Gateway, id: &str) -> Result<SourcesChange> {
    let vault = gateway.vault();
    let instances = Instances::load(vault.root())?;
    let instance = instances
        .get(id)
        .ok_or_else(|| Error::Refused("This source is not followed.".to_owned()))?;
    let options = SyncOptions {
        command: connector_command()?,
        language: "en".to_owned(),
        full: false,
        now: Timestamp::now(),
    };
    let synced = sync_instance(vault, instance, &*secrets(), &options)?;
    let outcome = if synced.succeeded() {
        "synced"
    } else {
        "failed"
    };
    let code = if synced.succeeded() {
        None
    } else {
        runs(vault.root(), 1)?
            .into_iter()
            .next()
            .and_then(|run| run.code)
    };
    answer(gateway, outcome, code, Some(id.to_owned()))
}

/// Imports the file at `path` (a copy of what the person picked, named `name`), as
/// `ipse import`; the copy is removed after.
///
/// # Errors
///
/// Errors of the memory, the journal, the history or the log of runs. A file Ipse
/// cannot read is an answer, not an error.
pub(crate) fn import(gateway: &mut Gateway, path: &Path, name: &str) -> Result<SourcesChange> {
    let result = crate::cmd::import::import_into(gateway.vault(), path);
    let _ = std::fs::remove_file(path);
    match result {
        Ok(_) => {
            let instance = runs(gateway.vault().root(), 1)?
                .into_iter()
                .next()
                .map(|run| run.instance);
            answer(gateway, "imported", None, instance)
        }
        Err(Error::Refused(_)) if !importable(name) => answer(gateway, "not_supported", None, None),
        Err(Error::Refused(_)) => answer(gateway, "bad_file", extension(name), None),
        Err(error) => Err(error),
    }
}

/// The extension of `name`, in lower case.
pub(crate) fn extension(name: &str) -> Option<String> {
    Path::new(name)
        .extension()
        .and_then(|e| e.to_str())
        .map(str::to_ascii_lowercase)
}

/// Whether the screen imports a file named `name`.
pub(crate) fn importable(name: &str) -> bool {
    extension(name).is_some_and(|e| IMPORTABLE.contains(&e.as_str()))
}

/// Where an upload is written while it is imported: the system's temporary folder,
/// never the memory's.
pub(crate) fn upload_path(random: &str, name: &str) -> PathBuf {
    let extension = extension(name).unwrap_or_default();
    std::env::temp_dir().join(format!("ipse-import-{random}.{extension}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_the_files_ipse_reads_are_imported() {
        assert!(importable("contacts.vcf"));
        assert!(importable("Agenda.ICS"));
        assert!(importable("Timeline.json"));
        assert!(!importable("photo.jpg"));
        assert!(!importable("no-extension"));
        assert_eq!(connector_of("calendar-2"), "calendar");
        assert_eq!(connector_of("google-timeline"), "google-timeline");
        assert_eq!(connector_of("contacts"), "contacts");
        let path = upload_path("abc", "Timeline.json");
        assert!(path.starts_with(std::env::temp_dir()));
        assert!(path.to_string_lossy().ends_with("ipse-import-abc.json"));
    }
}
