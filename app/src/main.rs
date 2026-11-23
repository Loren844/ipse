//! Ipse, the desktop app (decisions 0001, 0007 and 0015): the memory in its own window.
//!
//! The app holds no memory code of its own. It ships the `ipse` command and runs it:
//!
//! - `ipse ui --app` serves the interface on `127.0.0.1` and prints its one-time
//!   address, which the window opens: the same interface as `ipse ui` in a browser,
//!   with the same protections (`crates/ipse/src/ui.rs`). The command stops when the
//!   app closes its input, so the server never outlives the app.
//! - `ipse init` creates a memory from the app's first screen (`welcome/`).
//!
//! Running the command rather than linking the core keeps one way in: connectors,
//! imports and the AI apps' configuration all start that same `ipse` program.

#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod commands;
mod memory;
mod navigation;
mod prefs;
mod server;
mod tray;
mod words;

#[cfg(target_os = "macos")]
mod menu;

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, PoisonError};

use tauri::webview::PageLoadEvent;
use tauri::{Manager as _, RunEvent, WebviewWindowBuilder, WindowEvent};
use tauri_plugin_autostart::MacosLauncher;

use crate::navigation::Go;
use crate::server::Server;

/// What the app keeps while it runs.
pub(crate) struct State {
    /// The `ipse ui` command serving the memory open in the window, if any.
    pub(crate) server: Mutex<Option<Server>>,
    /// The port of that server, read on every navigation of the window.
    pub(crate) port: Arc<Mutex<Option<u16>>>,
    /// Ipse's icon is in the menu bar or the tray: closing the window keeps the app
    /// running there.
    pub(crate) tray: AtomicBool,
}

impl State {
    /// Stops the server, if one runs.
    pub(crate) fn stop(&self) {
        let server = self
            .server
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .take();
        *self.port.lock().unwrap_or_else(PoisonError::into_inner) = None;
        if let Some(server) = server {
            server.stop();
        }
    }
}

/// The argument the system starts the app with when it starts with the computer: the
/// window stays closed, Ipse works from its icon.
const HIDDEN: &str = "--hidden";

/// Runs before any script of a page in the window, the interface's included: tells the
/// interface it is in the app, and gives Windows and Linux the shortcuts the menu gives
/// on macOS. `/app/…` addresses are answered by the app ([`navigation`]).
const IN_THE_WINDOW: &str = r"
window.ipseApp = { version: '__VERSION__' };
document.addEventListener('keydown', (event) => {
  if (navigator.platform.startsWith('Mac') || !event.ctrlKey || event.altKey) return;
  const key = event.key.toLowerCase();
  if (key === 'r') { event.preventDefault(); location.reload(); }
  else if (key === 'q') { event.preventDefault(); location.href = location.origin + '/app/quit'; }
});
";

fn main() {
    let port: Arc<Mutex<Option<u16>>> = Arc::default();
    let builder = tauri::Builder::default()
        .plugin(tauri_plugin_single_instance::init(|app, _, _| {
            tray::show(app);
        }))
        .plugin(tauri_plugin_window_state::Builder::default().build())
        .plugin(tauri_plugin_notification::init())
        .plugin(tauri_plugin_autostart::init(
            MacosLauncher::LaunchAgent,
            Some(vec![HIDDEN]),
        ))
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_opener::init())
        .manage(State {
            server: Mutex::new(None),
            port: Arc::clone(&port),
            tray: AtomicBool::new(false),
        })
        .invoke_handler(tauri::generate_handler![
            commands::start,
            commands::create,
            commands::open,
        ]);
    #[cfg(target_os = "macos")]
    let builder = builder
        .menu(|app| menu::build(app, &words::Words::here()))
        .on_menu_event(menu::chosen);

    let app = builder
        .setup(move |app| {
            match tray::build(app.handle(), &words::Words::here()) {
                Ok(_) => app.state::<State>().tray.store(true, Ordering::Relaxed),
                Err(error) => eprintln!("Ipse has no icon in the menu bar or tray: {error}"),
            }
            let config = app
                .config()
                .app
                .windows
                .first()
                .cloned()
                .ok_or("the app's window is not configured")?;
            let handle = app.handle().clone();
            let ports = Arc::clone(&port);
            let window = WebviewWindowBuilder::from_config(app.handle(), &config)?
                .initialization_script(
                    IN_THE_WINDOW.replace("__VERSION__", env!("CARGO_PKG_VERSION")),
                )
                .on_navigation(move |url| {
                    let port = *ports.lock().unwrap_or_else(PoisonError::into_inner);
                    match navigation::decide(url, port) {
                        Go::Allow => true,
                        Go::Refuse => false,
                        go => {
                            navigation::act(&handle, go);
                            false
                        }
                    }
                })
                .on_page_load(|window, payload| {
                    if payload.event() == PageLoadEvent::Finished {
                        prefs::tell(window.app_handle());
                    }
                })
                .build()?;
            let hidden = std::env::args().any(|arg| arg == HIDDEN);
            if !hidden || !app.state::<State>().tray.load(Ordering::Relaxed) {
                let _ = window.show();
            }
            Ok(())
        })
        .on_window_event(|window, event| {
            if let WindowEvent::CloseRequested { api, .. } = event {
                let app = window.app_handle();
                if app.state::<State>().tray.load(Ordering::Relaxed) {
                    api.prevent_close();
                    let _ = window.hide();
                    prefs::told_closed(app);
                }
            }
        })
        .build(tauri::generate_context!());
    let app = match app {
        Ok(app) => app,
        Err(error) => {
            eprintln!("Ipse could not start: {error}");
            std::process::exit(1);
        }
    };
    app.run(|handle, event| match event {
        RunEvent::Exit => handle.state::<State>().stop(),
        // The app's icon in the Dock, clicked while the window is closed.
        #[cfg(target_os = "macos")]
        RunEvent::Reopen { .. } => tray::show(handle),
        _ => {}
    });
}

/// The address of the app's own first screen, where a memory is chosen; with
/// `choose`, it lists the choices at once instead of opening the last memory.
pub(crate) fn welcome(choose: bool) -> Option<tauri::Url> {
    // Tauri serves the app's own pages from this origin (`http://tauri.localhost` on
    // Windows, where the webview cannot use a custom scheme).
    let origin = if cfg!(windows) {
        "http://tauri.localhost"
    } else {
        "tauri://localhost"
    };
    let page = if choose {
        "/index.html?choose"
    } else {
        "/index.html"
    };
    tauri::Url::parse(&format!("{origin}{page}")).ok()
}
