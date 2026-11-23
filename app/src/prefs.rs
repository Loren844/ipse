//! The app's own preferences, in `app.json` of the app's settings folder: whether it
//! shows system notifications, whether it checks for updates, and whether it already
//! said that closing the window keeps it running. Starting with the computer is the system's own setting, read
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
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub(crate) struct Prefs {
    /// Show a system notification for new proposals and for a source that fails.
    pub(crate) notifications: bool,
    /// The person was told, the first time they closed the window, that Ipse keeps
    /// running in the menu bar or the tray.
    pub(crate) told_closed: bool,
    /// Check for a new version at start and once a day (issue #156); `None` until the
    /// person answered the question the first start asks.
    pub(crate) updates: Option<bool>,
    /// The last version a notification announced, so that it is announced once.
    pub(crate) told_update: Option<String>,
}

impl Default for Prefs {
    fn default() -> Self {
        Self {
            notifications: true,
            told_closed: false,
            updates: None,
            told_update: None,
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
    pub(crate) fn write(&self, dir: &Path) -> std::io::Result<()> {
        std::fs::create_dir_all(dir)?;
        let bytes = serde_json::to_vec_pretty(self).map_err(std::io::Error::other)?;
        std::fs::write(dir.join(FILE), bytes)
    }
}

/// The app's preferences as they are now.
pub(crate) fn current<R: Runtime>(app: &AppHandle<R>) -> Prefs {
    app.path()
        .app_config_dir()
        .map(|dir| Prefs::read(&dir))
        .unwrap_or_default()
}

/// Saves `prefs`; not saving only means asking or saying again at the next start.
pub(crate) fn save<R: Runtime>(app: &AppHandle<R>, prefs: &Prefs) {
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
    // Without the update key (a development build), there is nothing to check.
    let updates = crate::updates::enabled(app).then_some(prefs.updates == Some(true));
    let _ = window.eval(script(prefs.notifications, start, updates));
    crate::updates::tell(app);
}

/// The script that hands the settings to the page; `updates` is `null` when this build
/// cannot update itself.
fn script(notifications: bool, start: bool, updates: Option<bool>) -> String {
    let updates = updates.map_or("null", |on| if on { "true" } else { "false" });
    format!(
        "if (window.ipseApp) {{ window.ipseApp.settings = {{ notifications: {notifications}, start: {start}, updates: {updates} }}; \
         window.dispatchEvent(new Event('ipse-app-settings')); }}"
    )
}

/// Changes what the person changed in the interface's Settings, then tells the page.
pub(crate) fn change<R: Runtime>(
    app: &AppHandle<R>,
    notifications: Option<bool>,
    start: Option<bool>,
    updates: Option<bool>,
) {
    if notifications.is_some() || updates.is_some() {
        let mut prefs = current(app);
        if let Some(notifications) = notifications {
            prefs.notifications = notifications;
        }
        if let Some(updates) = updates {
            prefs.updates = Some(updates);
        }
        save(app, &prefs);
    }
    if updates == Some(true) {
        crate::updates::check_soon(app);
    } else if updates == Some(false) {
        crate::updates::forget(app);
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
    save(app, &prefs);
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

        assert_eq!(Prefs::default().updates, None, "asked at the first start");
        let chosen = Prefs {
            notifications: false,
            told_closed: true,
            updates: Some(true),
            told_update: Some("0.3.0".to_owned()),
        };
        chosen.write(dir.path()).unwrap();
        assert_eq!(Prefs::read(dir.path()), chosen);

        std::fs::write(dir.path().join(FILE), "{\"told_closed\": true}").unwrap();
        let older = Prefs::read(dir.path());
        assert!(
            older.notifications && older.told_closed && older.updates.is_none(),
            "a missing key is its default"
        );
        std::fs::write(dir.path().join(FILE), "not json").unwrap();
        assert_eq!(Prefs::read(dir.path()), Prefs::default());
    }

    #[test]
    fn the_page_gets_the_settings_as_booleans() {
        let script = script(true, false, Some(true));
        assert!(
            script.contains("{ notifications: true, start: false, updates: true }"),
            "{script}"
        );
        assert!(script.contains("ipse-app-settings"));
        assert!(
            super::script(true, true, None).contains("updates: null"),
            "a build without the update key"
        );
    }
}
