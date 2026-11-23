//! Ipse's icon in the menu bar (macOS) or the tray (Windows, Linux), which keeps the
//! memory up to date while the window is closed (issue #153): open the window, sync
//! now, quit.

use tauri::menu::{Menu, MenuEvent, MenuItem, PredefinedMenuItem};
use tauri::tray::{MouseButton, MouseButtonState, TrayIcon, TrayIconBuilder, TrayIconEvent};
use tauri::{AppHandle, Manager as _, Runtime};

use crate::State;
use crate::words::Words;

/// The menu item that shows the window.
const OPEN: &str = "tray-open";
/// The menu item that syncs every followed source now.
const SYNC: &str = "tray-sync";
/// The menu item that quits.
const QUIT: &str = "tray-quit";

/// Puts the icon in the menu bar or the tray.
///
/// # Errors
///
/// When the system has no place for it (a Linux desktop without a tray): the window
/// then quits the app when closed, as before.
pub(crate) fn build<R: Runtime>(app: &AppHandle<R>, words: &Words) -> tauri::Result<TrayIcon<R>> {
    let menu = Menu::with_items(
        app,
        &[
            &MenuItem::with_id(app, OPEN, words.tray_open, true, None::<&str>)?,
            &MenuItem::with_id(app, SYNC, words.tray_sync, true, None::<&str>)?,
            &PredefinedMenuItem::separator(app)?,
            &MenuItem::with_id(app, QUIT, words.tray_quit, true, None::<&str>)?,
        ],
    )?;
    let mut tray = TrayIconBuilder::with_id("ipse")
        .tooltip("Ipse")
        .menu(&menu)
        // The menu opens on a click on macOS, as menu bar icons do; elsewhere a
        // click opens the window and the menu is on the other button.
        .show_menu_on_left_click(cfg!(target_os = "macos"))
        .on_menu_event(chosen)
        .on_tray_icon_event(|tray, event| {
            if let TrayIconEvent::Click {
                button: MouseButton::Left,
                button_state: MouseButtonState::Up,
                ..
            } = event
            {
                show(tray.app_handle());
            }
        });
    if let Some(icon) = app.default_window_icon() {
        tray = tray.icon(icon.clone());
    }
    tray.build(app)
}

/// Answers an item of the icon's menu.
#[allow(clippy::needless_pass_by_value)] // the signature Tauri's menu handler takes
fn chosen<R: Runtime>(app: &AppHandle<R>, event: MenuEvent) {
    match event.id().as_ref() {
        OPEN => show(app),
        SYNC => {
            let state = app.state::<State>();
            let mut server = state
                .server
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            // No memory open (the first screen): nothing to sync.
            if let Some(server) = server.as_mut() {
                let _ = server.sync_now();
            }
        }
        QUIT => app.exit(0),
        _ => {}
    }
}

/// Shows the window, in front.
pub(crate) fn show<R: Runtime>(app: &AppHandle<R>) {
    if let Some(window) = app.get_webview_window("main") {
        let _ = window.unminimize();
        let _ = window.show();
        let _ = window.set_focus();
    }
}
