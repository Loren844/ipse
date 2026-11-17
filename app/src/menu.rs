//! The app's menu on macOS, where every app has one: the usual items, and opening
//! another memory. Windows and Linux get the same shortcuts from the window itself
//! (`IN_THE_WINDOW` in `main.rs`).

use tauri::menu::{Menu, MenuEvent, MenuItem, PredefinedMenuItem, Submenu};
use tauri::{AppHandle, Manager as _, Runtime};

use crate::navigation::{self, Go};
use crate::words::Words;

/// The menu item that goes back to the first screen.
const OTHER_MEMORY: &str = "other-memory";
/// The menu item that reloads the page.
const RELOAD: &str = "reload";

/// Builds the menu bar.
pub(crate) fn build<R: Runtime>(app: &AppHandle<R>, words: Words) -> tauri::Result<Menu<R>> {
    let ipse = Submenu::with_items(
        app,
        "Ipse",
        true,
        &[
            &PredefinedMenuItem::about(app, None, None)?,
            &PredefinedMenuItem::separator(app)?,
            &PredefinedMenuItem::hide(app, None)?,
            &PredefinedMenuItem::hide_others(app, None)?,
            &PredefinedMenuItem::show_all(app, None)?,
            &PredefinedMenuItem::separator(app)?,
            &PredefinedMenuItem::quit(app, None)?,
        ],
    )?;
    let file = Submenu::with_items(
        app,
        words.file,
        true,
        &[
            &MenuItem::with_id(
                app,
                OTHER_MEMORY,
                words.other_memory,
                true,
                Some("CmdOrCtrl+O"),
            )?,
            &PredefinedMenuItem::separator(app)?,
            &PredefinedMenuItem::close_window(app, None)?,
        ],
    )?;
    let edit = Submenu::with_items(
        app,
        words.edit,
        true,
        &[
            &PredefinedMenuItem::undo(app, None)?,
            &PredefinedMenuItem::redo(app, None)?,
            &PredefinedMenuItem::separator(app)?,
            &PredefinedMenuItem::cut(app, None)?,
            &PredefinedMenuItem::copy(app, None)?,
            &PredefinedMenuItem::paste(app, None)?,
            &PredefinedMenuItem::select_all(app, None)?,
        ],
    )?;
    let view = Submenu::with_items(
        app,
        words.view,
        true,
        &[
            &MenuItem::with_id(app, RELOAD, words.reload, true, Some("CmdOrCtrl+R"))?,
            &PredefinedMenuItem::separator(app)?,
            &PredefinedMenuItem::fullscreen(app, None)?,
        ],
    )?;
    let window = Submenu::with_items(
        app,
        words.window,
        true,
        &[
            &PredefinedMenuItem::minimize(app, None)?,
            &PredefinedMenuItem::maximize(app, None)?,
        ],
    )?;
    Menu::with_items(app, &[&ipse, &file, &edit, &view, &window])
}

/// Answers a menu item of the app's own.
#[allow(clippy::needless_pass_by_value)] // the signature Tauri's menu handler takes
pub(crate) fn chosen(app: &AppHandle, event: MenuEvent) {
    match event.id().as_ref() {
        OTHER_MEMORY => navigation::act(app, Go::Switch),
        RELOAD => {
            if let Some(window) = app.get_webview_window("main") {
                let _ = window.eval("location.reload()");
            }
        }
        _ => {}
    }
}
