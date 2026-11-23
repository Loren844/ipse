//! Updates (issue #156): the app looks for a new version at start and once a day, when
//! the person agreed at the first start (or later in Settings), and installs it only
//! when they say so in the window. The only host it asks is GitHub's release download,
//! for the manifest the release publishes (`.github/scripts/update_manifest.py`); the
//! update is verified against the public key in `tauri.conf.json` before it is
//! installed, and must carry the version it was signed for.
//!
//! A release whose memory format is newer than this version's says so before it is
//! installed: before 1.0, a minor version may change it (CHANGELOG.md).

use std::sync::{Mutex, PoisonError};
use std::time::Duration;

use serde::Serialize;
use tauri::{AppHandle, Manager as _, Runtime};
use tauri_plugin_dialog::{DialogExt as _, MessageDialogButtons, MessageDialogKind};
use tauri_plugin_notification::NotificationExt as _;
use tauri_plugin_updater::{Update, UpdaterExt as _};

use crate::prefs;
use crate::words::Words;

/// How often the app looks again while it runs.
const DAY: Duration = Duration::from_hours(24);

/// The memory format this version reads and writes: `FORMAT` in
/// `crates/ipse-core/src/vault/settings.rs`, read by `build.rs`.
const MEMORY_FORMAT: &str = env!("IPSE_MEMORY_FORMAT");

/// The update found, until the person installs it or the next check.
#[derive(Default)]
pub(crate) struct Pending {
    update: Mutex<Option<Update>>,
    /// The person said "later": the window stops showing it until the next check.
    hidden: Mutex<bool>,
}

/// What the window is told about an update, as `window.ipseApp.update`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
struct Offer {
    version: String,
    /// The release's notes: what changed, for the people who use Ipse.
    notes: Option<String>,
    /// The new version changes the memory's format.
    format_changes: bool,
}

/// True when this build can update itself: it was built with the update key.
pub(crate) fn enabled<R: Runtime>(app: &AppHandle<R>) -> bool {
    app.config()
        .plugins
        .0
        .get("updater")
        .and_then(|updater| updater.get("pubkey"))
        .and_then(serde_json::Value::as_str)
        .is_some_and(|key| !key.trim().is_empty())
}

/// At start: asks the first time, then looks now and once a day while the app runs.
pub(crate) fn start(app: &AppHandle) {
    if !enabled(app) {
        return;
    }
    #[cfg(feature = "update-test")]
    test::install_now(app);
    #[cfg(not(feature = "update-test"))]
    watch(app);
}

/// Asks the first time, then looks now and once a day while the app runs.
#[cfg_attr(feature = "update-test", allow(dead_code))]
fn watch(app: &AppHandle) {
    if prefs::current(app).updates.is_none() {
        ask(app);
    }
    let app = app.clone();
    std::thread::spawn(move || {
        loop {
            if prefs::current(&app).updates == Some(true) {
                tauri::async_runtime::block_on(check(&app));
            }
            std::thread::sleep(DAY);
        }
    });
}

/// The first start's question: check for updates? Yes is the default answer.
#[cfg_attr(feature = "update-test", allow(dead_code))]
fn ask(app: &AppHandle) {
    let words = Words::here();
    let handle = app.clone();
    app.dialog()
        .message(words.updates_question)
        .title(words.updates_title)
        .kind(MessageDialogKind::Info)
        .buttons(MessageDialogButtons::OkCancelCustom(
            words.updates_yes.to_owned(),
            words.updates_no.to_owned(),
        ))
        .show(move |yes| {
            let mut chosen = prefs::current(&handle);
            chosen.updates = Some(yes);
            prefs::save(&handle, &chosen);
            prefs::tell(&handle);
            if yes {
                check_soon(&handle);
            }
        });
}

/// Looks for an update now, in the background: when the person turns it on.
pub(crate) fn check_soon<R: Runtime>(app: &AppHandle<R>) {
    if !enabled(app) {
        return;
    }
    let app = app.clone();
    tauri::async_runtime::spawn(async move { check(&app).await });
}

/// Forgets the update found: when the person turns checking off.
pub(crate) fn forget<R: Runtime>(app: &AppHandle<R>) {
    if let Some(pending) = app.try_state::<Pending>() {
        *pending
            .update
            .lock()
            .unwrap_or_else(PoisonError::into_inner) = None;
    }
    tell(app);
}

/// Asks for the manifest; keeps an update when there is one, and tells the window
/// and, once per version, the system's notifications.
async fn check<R: Runtime>(app: &AppHandle<R>) {
    let Ok(updater) = app.updater() else {
        return;
    };
    // A failure (no network, GitHub down) is not the person's concern: next time.
    let Ok(Some(update)) = updater.check().await else {
        return;
    };
    let version = update.version.clone();
    if let Some(pending) = app.try_state::<Pending>() {
        *pending
            .update
            .lock()
            .unwrap_or_else(PoisonError::into_inner) = Some(update);
        *pending
            .hidden
            .lock()
            .unwrap_or_else(PoisonError::into_inner) = false;
    }
    tell(app);
    let mut chosen = prefs::current(app);
    if chosen.told_update.as_deref() != Some(version.as_str()) {
        if chosen.notifications {
            let words = Words::here();
            let _ = app
                .notification()
                .builder()
                .title("Ipse")
                .body((words.update_ready)(&version))
                .show();
        }
        chosen.told_update = Some(version);
        prefs::save(app, &chosen);
    }
}

/// What the window is told of `update`.
fn offer(update: &Update) -> Offer {
    Offer {
        version: update.version.clone(),
        notes: update.body.clone(),
        format_changes: changes_format(&update.raw_json),
    }
}

/// True when the release's manifest says its memory format is newer than this one's.
fn changes_format(manifest: &serde_json::Value) -> bool {
    let ours: u64 = MEMORY_FORMAT.parse().unwrap_or(0);
    manifest
        .get("memory_format")
        .and_then(serde_json::Value::as_u64)
        .is_some_and(|theirs| theirs > ours)
}

/// Gives the page in the window the update found, or `null`:
/// `window.ipseApp.update`, then an `ipse-app-update` event.
pub(crate) fn tell<R: Runtime>(app: &AppHandle<R>) {
    let Some(window) = app.get_webview_window("main") else {
        return;
    };
    let shown = app.try_state::<Pending>().and_then(|pending| {
        if *pending
            .hidden
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
        {
            return None;
        }
        pending
            .update
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .as_ref()
            .map(offer)
    });
    let _ = window.eval(script(shown.as_ref()));
}

/// The script that hands the update to the page.
fn script(offer: Option<&Offer>) -> String {
    let json = offer
        .and_then(|offer| serde_json::to_string(offer).ok())
        .unwrap_or_else(|| "null".to_owned());
    format!(
        "if (window.ipseApp) {{ window.ipseApp.update = {json}; \
         window.dispatchEvent(new Event('ipse-app-update')); }}"
    )
}

/// The person said "later": the window stops showing the update until the next check.
pub(crate) fn later<R: Runtime>(app: &AppHandle<R>) {
    if let Some(pending) = app.try_state::<Pending>() {
        *pending
            .hidden
            .lock()
            .unwrap_or_else(PoisonError::into_inner) = true;
    }
    tell(app);
}

/// The person said "install": downloads the update, verifies its signature, installs
/// it and starts the new version. If it fails, the window says so and the app goes on.
pub(crate) fn install(app: &AppHandle) {
    let update = app.try_state::<Pending>().and_then(|pending| {
        pending
            .update
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .take()
    });
    let Some(update) = update else {
        return;
    };
    let app = app.clone();
    tauri::async_runtime::spawn(async move {
        match update.download_and_install(|_, _| {}, || {}).await {
            Ok(()) => {
                // The memory's server stops with this version; the new one starts its own.
                app.state::<crate::State>().stop();
                app.restart();
            }
            Err(error) => {
                eprintln!("Ipse could not install the update: {error}");
                if let Some(window) = app.get_webview_window("main") {
                    let _ = window.eval(
                        "if (window.ipseApp) { window.ipseApp.update = { failed: true }; \
                         window.dispatchEvent(new Event('ipse-app-update')); }",
                    );
                }
            }
        }
    });
}

/// The test of an update in CI (`.github/workflows/app-update.yml`), built only with
/// the `update-test` feature: the app installs what the manifest offers at once,
/// without asking, then quits. Never in a release.
#[cfg(feature = "update-test")]
mod test {
    use tauri::AppHandle;
    use tauri_plugin_updater::UpdaterExt as _;

    /// Installs the update the test's manifest offers, then quits: 0 when installed,
    /// 2 when there was none, 1 when it failed.
    pub(super) fn install_now(app: &AppHandle) {
        let app = app.clone();
        tauri::async_runtime::spawn(async move {
            let code = match app.updater() {
                Ok(updater) => match updater.check().await {
                    Ok(Some(update)) => match update.download_and_install(|_, _| {}, || {}).await {
                        Ok(()) => {
                            println!("installed {}", update.version);
                            0
                        }
                        Err(error) => {
                            eprintln!("install failed: {error}");
                            1
                        }
                    },
                    Ok(None) => {
                        eprintln!("no update offered");
                        2
                    }
                    Err(error) => {
                        eprintln!("check failed: {error}");
                        1
                    }
                },
                Err(error) => {
                    eprintln!("no updater: {error}");
                    1
                }
            };
            app.exit(code);
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_newer_memory_format_is_said() {
        let ours: u64 = MEMORY_FORMAT.parse().unwrap();
        assert!(ours >= 1, "read from the core: {MEMORY_FORMAT}");
        assert!(changes_format(
            &serde_json::json!({ "version": "9.0.0", "memory_format": ours + 1 })
        ));
        assert!(!changes_format(
            &serde_json::json!({ "version": "9.0.0", "memory_format": ours })
        ));
        assert!(
            !changes_format(&serde_json::json!({ "version": "9.0.0" })),
            "a manifest without it changes nothing"
        );
    }

    #[test]
    fn the_page_gets_the_update_or_null() {
        let offer = Offer {
            version: "0.3.0".to_owned(),
            notes: Some("Des « guillemets » et \"quotes\".".to_owned()),
            format_changes: true,
        };
        let script = script(Some(&offer));
        assert!(script.contains(r#""version":"0.3.0""#), "{script}");
        assert!(script.contains(r#"\"quotes\""#), "notes are escaped JSON");
        assert!(script.contains(r#""format_changes":true"#));
        assert!(script.contains("ipse-app-update"));
        assert!(super::script(None).contains("window.ipseApp.update = null;"));
    }
}
