//! What `ipse ui --app` does in the background for the desktop app (issue #153): the
//! followed sources sync on their interval, one at a time, and what the app should
//! tell the person goes to it as events, one JSON object per line on this command's
//! output, after the address:
//!
//! - `{"event":"waiting","count":4,"new":1}`: more proposals or questions wait for
//!   the person than before (`count` in all, `new` since the last event).
//! - `{"event":"sync_failed","source":"Perso"}`: a followed source keeps failing, or
//!   failed when the person asked to sync now. The Sources screen says what to do.
//!
//! The app asks for a sync of every followed source now with the line `sync` on this
//! command's input.
//!
//! Nothing runs between syncs: the scheduler sleeps until the next source is due, and
//! the folders where proposals and questions are written are watched by the system
//! (inotify on Linux, `FSEvents` on macOS, `ReadDirectoryChangesW` on Windows), not read again and again.

use std::io::Write as _;
use std::path::{Path, PathBuf};
use std::sync::mpsc::{self, Receiver, RecvTimeoutError};
use std::sync::{Arc, PoisonError};
use std::time::Duration;

use ipse_core::connector::{Instances, SyncState};
use ipse_core::sync::{SyncOptions, sync_instance};
use ipse_core::time::Timestamp;
use notify::{RecursiveMode, Watcher as _};
use serde::Serialize;

use super::State;
use crate::cmd::sync::connector_command;
use crate::keychain::secrets;

/// The longest the scheduler sleeps while sources are followed: the clock it sleeps on
/// stops while the computer sleeps on some systems, so a long sleep could miss the
/// time a source was due by hours.
const LONGEST_SLEEP: Duration = Duration::from_mins(15);

/// The shortest: a source that could not even start (its secret is not in the
/// keychain…) is not recorded as run, and is tried again after this.
const SHORTEST_SLEEP: Duration = Duration::from_mins(1);

/// How long the watcher lets a burst of writes settle before counting what waits.
const SETTLE: Duration = Duration::from_millis(500);

/// The folders where what waits for the person is written, relative to the vault.
const WATCHED: [&str; 2] = [".ipse/proposals", ".ipse/tasks"];

/// Failures in a row after which a source that syncs on its own is said to fail: one
/// failure is often the network coming back after sleep, and is tried again a minute
/// later.
const FAILING_AFTER: u32 = 2;

/// Why the scheduler wakes before the next source is due.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Wake {
    /// The person asked to sync every followed source now.
    Now,
    /// The followed sources changed.
    Changed,
}

/// What the app should tell the person.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(tag = "event", rename_all = "snake_case")]
pub(crate) enum Event {
    /// More proposals and questions wait than before.
    Waiting {
        /// How many wait in all.
        count: usize,
        /// How many more than at the last event.
        new: usize,
    },
    /// A followed source failed to sync.
    SyncFailed {
        /// The name the person gave it.
        source: String,
    },
}

/// Writes `event` on its own line of the output, for the app.
fn say(event: &Event) {
    let Ok(line) = serde_json::to_string(event) else {
        return;
    };
    // The app may be gone: nothing to do then, it stops this command anyway.
    let mut out = std::io::stdout().lock();
    let _ = writeln!(out, "{line}");
    let _ = out.flush();
}

/// Starts the scheduler and the watcher, each on its own thread, for the memory that
/// `state` serves; the scheduler also wakes on `wakes`, and stops when every sender
/// is gone.
pub(crate) fn start(state: &Arc<State>, wakes: Receiver<Wake>) {
    let root = state
        .gateway
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
        .vault()
        .root()
        .to_path_buf();
    let (scheduled, watched) = (Arc::clone(state), Arc::clone(state));
    let at = root.clone();
    std::thread::spawn(move || schedule(&scheduled, &at, &wakes));
    std::thread::spawn(move || watch(&watched, &root));
}

/// Syncs the sources as they come due, until every sender of `wakes` is gone.
fn schedule(state: &State, root: &Path, wakes: &Receiver<Wake>) {
    let mut all = false;
    loop {
        let woke = match sync_due(state, root, all) {
            None => wakes.recv().map_err(|_| RecvTimeoutError::Disconnected),
            Some(wait) => wakes.recv_timeout(wait),
        };
        all = match woke {
            Ok(Wake::Now) => true,
            Ok(Wake::Changed) | Err(RecvTimeoutError::Timeout) => false,
            Err(RecvTimeoutError::Disconnected) => return,
        };
    }
}

/// Syncs every followed source that is due, or every one with `all`, one after the
/// other. Returns how long to sleep until the next is due; `None` when no source is
/// followed, and only a [`Wake`] can bring work.
fn sync_due(state: &State, root: &Path, all: bool) -> Option<Duration> {
    let instances = match Instances::load(root) {
        Ok(instances) => instances,
        Err(error) => {
            // The details name files of the vault: they stay on the error output.
            eprintln!("{error}");
            return Some(LONGEST_SLEEP);
        }
    };
    if instances.list().is_empty() {
        return None;
    }
    let command = match connector_command() {
        Ok(command) => command,
        Err(error) => {
            eprintln!("{error}");
            return Some(LONGEST_SLEEP);
        }
    };
    let store = secrets();
    for instance in instances.list() {
        let now = Timestamp::now();
        let due = SyncState::load(root).is_ok_and(|s| s.due(instance, now));
        if !all && !due {
            continue;
        }
        let options = SyncOptions {
            command: command.clone(),
            language: "en".to_owned(),
            full: false,
            now,
        };
        // One sync at a time, also against the interface's own changes and syncs.
        let synced = {
            let gateway = state.gateway.lock().unwrap_or_else(PoisonError::into_inner);
            sync_instance(gateway.vault(), instance, &*store, &options)
        };
        let failed = match synced {
            Ok(synced) if synced.succeeded() => false,
            Ok(_) => {
                let failures = SyncState::load(root)
                    .ok()
                    .and_then(|s| s.last(&instance.id))
                    .map_or(0, |run| run.failures);
                all || failures == FAILING_AFTER
            }
            Err(error) => {
                eprintln!("{error}");
                all
            }
        };
        if failed {
            say(&Event::SyncFailed {
                source: instance.name.clone(),
            });
        }
    }
    let now = Timestamp::now();
    let Ok(sync_state) = SyncState::load(root) else {
        return Some(LONGEST_SLEEP);
    };
    let soonest = instances
        .list()
        .iter()
        .map(|instance| sync_state.due_at(instance).unwrap_or(now))
        .min()
        .unwrap_or(now);
    Some(sleep_until(soonest, now))
}

/// How long to sleep from `now` to `soonest`, between [`SHORTEST_SLEEP`] and
/// [`LONGEST_SLEEP`].
fn sleep_until(soonest: Timestamp, now: Timestamp) -> Duration {
    let wait = Duration::try_from(soonest.duration_since(now)).unwrap_or(Duration::ZERO);
    wait.clamp(SHORTEST_SLEEP, LONGEST_SLEEP)
}

/// Tells the app when more proposals or questions wait than before, for as long as
/// the system can watch the folders they are written to.
fn watch(state: &State, root: &Path) {
    let (sent, changes) = mpsc::channel();
    let watcher = notify::recommended_watcher(move |change: notify::Result<notify::Event>| {
        if change.is_ok() {
            let _ = sent.send(());
        }
    });
    let mut watcher = match watcher {
        Ok(watcher) => watcher,
        Err(error) => {
            eprintln!("Ipse cannot watch for new proposals: {error}");
            return;
        }
    };
    for folder in WATCHED {
        let path: PathBuf = root.join(folder);
        // Made now if missing, so that the first proposal is seen.
        let _ = std::fs::create_dir_all(&path);
        if let Err(error) = watcher.watch(&path, RecursiveMode::NonRecursive) {
            eprintln!("Ipse cannot watch for new proposals: {error}");
        }
    }
    let mut known = waiting(state);
    while changes.recv().is_ok() {
        // A proposal or a task is often several writes in a row.
        while changes.recv_timeout(SETTLE).is_ok() {}
        let count = waiting(state);
        if let Some(event) = news(known, count) {
            say(&event);
        }
        if count.is_some() {
            known = count;
        }
    }
}

/// What waits for the person now, as the home screen counts it; `None` when the
/// memory cannot be read.
fn waiting(state: &State) -> Option<usize> {
    let gateway = state.gateway.lock().unwrap_or_else(PoisonError::into_inner);
    super::api::waiting(&gateway).ok()
}

/// The event to send when what waits went from `before` to `now`: only when more
/// wait.
fn news(before: Option<usize>, now: Option<usize>) -> Option<Event> {
    let (before, now) = (before?, now?);
    (now > before).then(|| Event::Waiting {
        count: now,
        new: now - before,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn events_are_one_json_object_each() {
        let waiting = Event::Waiting { count: 4, new: 1 };
        assert_eq!(
            serde_json::to_string(&waiting).unwrap(),
            r#"{"event":"waiting","count":4,"new":1}"#
        );
        let failed = Event::SyncFailed {
            source: "Perso".to_owned(),
        };
        assert_eq!(
            serde_json::to_string(&failed).unwrap(),
            r#"{"event":"sync_failed","source":"Perso"}"#
        );
    }

    #[test]
    fn only_more_waiting_is_news() {
        assert_eq!(
            news(Some(2), Some(3)),
            Some(Event::Waiting { count: 3, new: 1 })
        );
        assert_eq!(news(Some(3), Some(3)), None);
        assert_eq!(news(Some(3), Some(1)), None, "the person decided some");
        assert_eq!(news(None, Some(3)), None);
        assert_eq!(news(Some(3), None), None);
    }

    #[test]
    fn the_scheduler_sleeps_between_one_and_fifteen_minutes() {
        let now: Timestamp = "2026-11-23T10:00:00Z".parse().unwrap();
        let at = |minutes: i64| {
            now.checked_add(ipse_core::time::SignedDuration::from_mins(minutes))
                .unwrap()
        };
        assert_eq!(sleep_until(at(5), now), Duration::from_mins(5));
        assert_eq!(sleep_until(at(-5), now), SHORTEST_SLEEP, "already due");
        assert_eq!(sleep_until(at(60), now), LONGEST_SLEEP);
    }
}
