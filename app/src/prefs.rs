//! The app's own preferences, in `app.json` of the app's settings folder: whether it
//! shows system notifications, and whether it already said that closing the window
//! keeps it running. Starting with the computer is the system's own setting, read
//! and changed through the autostart plugin, not kept here.

use std::path::Path;

use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Manager as _, Runtime};
use tauri_plugin_autostart::ManagerExt as _;
use tauri_plugin_dialog::{DialogExt as _, MessageDialogKind};
use tauri_plugin_notification::NotificationExt as _;

use crate::server::Event;
use crate::words::Words;

/// The file, in the app's settings folder.
const FILE: &str = "app.json";

/// What the app keeps of the person's choices.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub(crate) struct Prefs {
    /// Show a system notification for new proposals and for a source that fails.
    pub(crate) notifications: bool,
    /// The person was told, the first time they closed the window, that Ipse keeps
    /// running in the menu bar or the tray.
    pub(crate) told_closed: bool,
}

impl Default for Prefs {
    fn default() -> Self {
        Self {
            notifications: true,
            told_closed: false,
        }
    }
}

impl Prefs {
    /// Reads them from `dir`; the defaults when absent or unreadable.
    pub(crate) fn read(dir: &Path) -> Prefs {
        std::fs::read(dir.join(FILE))
            .ok()
            .and_then(|bytes| serde_json::from_slice(&bytes).ok())
            .unwrap_or_default()
    }

    /// Writes them to `dir`, creating the folder.
    pub(crate) fn write(self, dir: &Path) -> std::io::Result<()> {
        std::fs::create_dir_all(dir)?;
        let bytes = serde_json::to_vec_pretty(&self).map_err(std::io::Error::other)?;
        std::fs::write(dir.join(FILE), bytes)
    }
}

/// The app's preferences as they are now.
fn current<R: Runtime>(app: &AppHandle<R>) -> Prefs {
    app.path()
        .app_config_dir()
        .map(|dir| Prefs::read(&dir))
        .unwrap_or_default()
}

/// Saves `prefs`; not saving only means asking or saying again at the next start.
fn save<R: Runtime>(app: &AppHandle<R>, prefs: Prefs) {
    if let Ok(dir) = app.path().app_config_dir() {
        let _ = prefs.write(&dir);
    }
}

/// Gives the page in the window the app's settings, for the interface's Settings:
/// `window.ipseApp.settings`, then an `ipse-app-settings` event.
pub(crate) fn tell<R: Runtime>(app: &AppHandle<R>) {
    let Some(window) = app.get_webview_window("main") else {
        return;
    };
    let prefs = current(app);
    let start = app.autolaunch().is_enabled().unwrap_or(false);
    let _ = window.eval(script(prefs.notifications, start));
}

/// The script that hands the settings to the page.
fn script(notifications: bool, start: bool) -> String {
    format!(
        "if (window.ipseApp) {{ window.ipseApp.settings = {{ notifications: {notifications}, start: {start} }}; \
         window.dispatchEvent(new Event('ipse-app-settings')); }}"
    )
}

/// Changes what the person changed in the interface's Settings, then tells the page.
pub(crate) fn change<R: Runtime>(
    app: &AppHandle<R>,
    notifications: Option<bool>,
    start: Option<bool>,
) {
    if let Some(notifications) = notifications {
        let mut prefs = current(app);
        prefs.notifications = notifications;
        save(app, prefs);
    }
    if let Some(start) = start {
        let autostart = app.autolaunch();
        let _ = if start {
            autostart.enable()
        } else {
            autostart.disable()
        };
    }
    tell(app);
}

/// Says, the first time only, that closing the window keeps Ipse running.
pub(crate) fn told_closed<R: Runtime>(app: &AppHandle<R>) {
    let mut prefs = current(app);
    if prefs.told_closed {
        return;
    }
    let words = Words::here();
    app.dialog()
        .message(words.still_running)
        .title(words.still_running_title)
        .kind(MessageDialogKind::Info)
        .show(|_| {});
    prefs.told_closed = true;
    save(app, prefs);
}

/// Shows `event` as a system notification, unless the person turned them off.
pub(crate) fn notify<R: Runtime>(app: &AppHandle<R>, event: &Event) {
    if !current(app).notifications {
        return;
    }
    let words = Words::here();
    let body = match event {
        Event::Waiting { new } => (words.waiting)(*new),
        Event::SyncFailed { source } => (words.sync_failed)(source),
    };
    let _ = app.notification().builder().title("Ipse").body(body).show();
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn notifications_are_on_until_turned_off() {
        let dir = tempfile::tempdir().unwrap();
        assert_eq!(Prefs::read(dir.path()), Prefs::default());
        assert!(Prefs::default().notifications);

        let chosen = Prefs {
            notifications: false,
            told_closed: true,
        };
        chosen.write(dir.path()).unwrap();
        assert_eq!(Prefs::read(dir.path()), chosen);

        std::fs::write(dir.path().join(FILE), "{\"told_closed\": true}").unwrap();
        let older = Prefs::read(dir.path());
        assert!(
            older.notifications && older.told_closed,
            "a missing key is its default"
        );
        std::fs::write(dir.path().join(FILE), "not json").unwrap();
        assert_eq!(Prefs::read(dir.path()), Prefs::default());
    }

    #[test]
    fn the_page_gets_the_settings_as_booleans() {
        let script = script(true, false);
        assert!(
            script.contains("{ notifications: true, start: false }"),
            "{script}"
        );
        assert!(script.contains("ipse-app-settings"));
    }
}
