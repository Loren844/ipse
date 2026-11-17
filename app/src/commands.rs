//! What the first screen (`welcome/`) asks of the app.

use std::path::PathBuf;

use serde::Serialize;
use tauri::{AppHandle, Manager as _};

use crate::State;
use crate::memory::{self, New, Remembered, Taken};
use crate::server::{Failure, Server};
use crate::words::Words;

/// What the first screen shows.
#[derive(Debug, Serialize)]
#[serde(tag = "kind", rename_all = "lowercase")]
pub(crate) enum Start {
    /// The memory is open: the window is on its way to it.
    Opened,
    /// A memory must be chosen or made.
    Choose(Choice),
}

/// The first screen's choices.
#[derive(Debug, Serialize)]
pub(crate) struct Choice {
    /// `fr` or `en`: the words of the screen.
    language: &'static str,
    /// Where a new memory would go.
    folder: PathBuf,
    /// Where the demo memory would go.
    demo_folder: PathBuf,
    /// The memory opened last, when it still exists.
    last: Option<PathBuf>,
    /// Why the last memory was not opened, if it was not.
    problem: Option<String>,
}

/// Opens the memory chosen last, unless `choose`; otherwise says what to choose from.
#[tauri::command]
pub(crate) async fn start(app: AppHandle, choose: bool) -> Start {
    let words = Words::here();
    let settings = settings_dir(&app);
    let last = settings
        .as_ref()
        .and_then(|dir| Remembered::read(dir).folder);
    let mut problem = None;
    if let Some(folder) = last.as_ref().filter(|_| !choose) {
        if memory::is_memory(folder) {
            match open_in_window(&app, folder.clone(), words).await {
                Ok(()) => return Start::Opened,
                Err(said) => problem = Some(said),
            }
        } else {
            problem = Some(words.gone.to_owned());
        }
    }
    let home = app.path().home_dir().unwrap_or_else(|_| PathBuf::from("."));
    Start::Choose(Choice {
        language: words.code,
        folder: memory::proposed(&home, false),
        demo_folder: memory::proposed(&home, true),
        last: last.filter(|folder| memory::is_memory(folder)),
        problem,
    })
}

/// Makes a new memory, then opens it.
#[tauri::command]
pub(crate) async fn create(app: AppHandle, new: New) -> Result<(), String> {
    let words = Words::here();
    match memory::can_create(&new.folder) {
        Err(Taken::Memory) => return Err(words.memory_there.to_owned()),
        Err(Taken::NotEmpty) => return Err(words.not_empty.to_owned()),
        Ok(()) => {}
    }
    let folder = new.folder.clone();
    tauri::async_runtime::spawn_blocking(move || memory::create(&new))
        .await
        .map_err(|_| words.not_created.to_owned())?
        .map_err(|said| with_details(words.not_created, &said))?;
    open_in_window(&app, folder, words).await
}

/// Opens the memory in `folder`.
#[tauri::command]
pub(crate) async fn open(app: AppHandle, folder: PathBuf) -> Result<(), String> {
    let words = Words::here();
    if !memory::is_memory(&folder) {
        return Err(words.no_memory_here.to_owned());
    }
    open_in_window(&app, folder, words).await
}

/// Starts the server on `folder`, remembers the choice, and takes the window to it.
///
/// The app navigates the window itself: a page cannot, as the interface only accepts
/// its one-time address from the person (no other site may send them there), and a
/// navigation the app makes counts as theirs.
async fn open_in_window(app: &AppHandle, folder: PathBuf, words: Words) -> Result<(), String> {
    let state = app.state::<State>();
    state.stop();
    let opened = folder.clone();
    let (server, address) = tauri::async_runtime::spawn_blocking(move || Server::start(&opened))
        .await
        .map_err(|_| words.not_opened.to_owned())?
        .map_err(|failure| match failure {
            Failure::Missing => words.missing.to_owned(),
            Failure::Start(error) => with_details(words.not_opened, &error.to_string()),
            Failure::Said(said) => with_details(words.not_opened, &said),
        })?;
    *state
        .port
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner) = Some(server.port);
    *state
        .server
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner) = Some(server);
    if let Some(dir) = settings_dir(app) {
        // Not remembering only means choosing again at the next start.
        let _ = Remembered {
            folder: Some(folder),
        }
        .write(&dir);
    }
    let window = app
        .get_webview_window("main")
        .ok_or_else(|| words.not_opened.to_owned())?;
    let url = tauri::Url::parse(&address).map_err(|_| words.not_opened.to_owned())?;
    window
        .navigate(url)
        .map_err(|e| with_details(words.not_opened, &e.to_string()))
}

/// The app's settings folder.
fn settings_dir(app: &AppHandle) -> Option<PathBuf> {
    app.path().app_config_dir().ok()
}

/// `message`, then what `ipse` said, when it said something.
fn with_details(message: &str, said: &str) -> String {
    if said.is_empty() {
        message.to_owned()
    } else {
        format!("{message}\n{said}")
    }
}
