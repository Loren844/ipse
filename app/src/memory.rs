//! Which memory the app opens: the one chosen last, remembered in the app's own
//! settings folder, and the checks before creating or opening one.

use std::path::{Path, PathBuf};
use std::process::Stdio;

use serde::{Deserialize, Serialize};

use crate::server;

/// The app's memory of which memory to open, in `memory.json` of the app's settings
/// folder. The memory's own settings live in the memory; the app's own are in `app.json`
/// ([`crate::prefs`]).
#[derive(Debug, Default, Serialize, Deserialize)]
pub(crate) struct Remembered {
    /// The memory's folder.
    pub(crate) folder: Option<PathBuf>,
}

impl Remembered {
    /// Reads it from `dir`; nothing remembered when absent or unreadable.
    pub(crate) fn read(dir: &Path) -> Remembered {
        std::fs::read(dir.join("memory.json"))
            .ok()
            .and_then(|bytes| serde_json::from_slice(&bytes).ok())
            .unwrap_or_default()
    }

    /// Writes it to `dir`, creating the folder.
    pub(crate) fn write(&self, dir: &Path) -> std::io::Result<()> {
        std::fs::create_dir_all(dir)?;
        let bytes = serde_json::to_vec_pretty(self).map_err(std::io::Error::other)?;
        std::fs::write(dir.join("memory.json"), bytes)
    }
}

/// Whether `folder` holds a memory: Ipse's settings file is there.
pub(crate) fn is_memory(folder: &Path) -> bool {
    folder.join(".ipse").join("vault.toml").is_file()
}

/// What stands in the way of creating a memory in `folder`.
#[derive(Debug, PartialEq, Eq)]
pub(crate) enum Taken {
    /// A memory is already there.
    Memory,
    /// Other files are there.
    NotEmpty,
}

/// Whether a new memory can be made in `folder`: it does not exist yet, or is empty.
pub(crate) fn can_create(folder: &Path) -> Result<(), Taken> {
    if is_memory(folder) {
        return Err(Taken::Memory);
    }
    let empty = std::fs::read_dir(folder).map_or(true, |mut entries| entries.next().is_none());
    if empty { Ok(()) } else { Err(Taken::NotEmpty) }
}

/// A new memory, as the first screen asks for it.
#[derive(Debug, Deserialize)]
pub(crate) struct New {
    /// The person's name; absent for the demo memory.
    pub(crate) name: Option<String>,
    /// `fr` or `en`.
    pub(crate) language: String,
    /// An IANA time zone, like `Europe/Paris`.
    pub(crate) time_zone: String,
    /// Where to make it.
    pub(crate) folder: PathBuf,
    /// The invented demo memory instead of the person's own.
    pub(crate) demo: bool,
}

/// Runs `ipse init` for `new`; on failure, what `ipse` said (in English).
pub(crate) fn create(new: &New) -> Result<(), String> {
    let program = server::program().ok_or_else(String::new)?;
    let mut command = server::command(&program);
    command.arg("init").arg(&new.folder).args([
        "--language",
        &new.language,
        "--time-zone",
        &new.time_zone,
    ]);
    match (&new.name, new.demo) {
        (_, true) => {
            command.arg("--demo");
        }
        (Some(name), false) => {
            command.args(["--name", name]);
        }
        (None, false) => return Err(String::new()),
    }
    let out = command
        .stdin(Stdio::null())
        .output()
        .map_err(|e| e.to_string())?;
    if out.status.success() {
        Ok(())
    } else {
        Err(String::from_utf8_lossy(&out.stderr).trim().to_owned())
    }
}

/// Where the first screen proposes to make a memory: `Ipse` in the person's home
/// folder, or `Ipse demo` for the demo, numbered when already taken.
pub(crate) fn proposed(home: &Path, demo: bool) -> PathBuf {
    let base = if demo { "Ipse demo" } else { "Ipse" };
    let mut folder = home.join(base);
    let mut n = 2;
    while can_create(&folder).is_err() {
        folder = home.join(format!("{base} {n}"));
        n += 1;
    }
    folder
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_chosen_memory_is_remembered() {
        let dir = tempfile::tempdir().unwrap();
        assert!(Remembered::read(dir.path()).folder.is_none());
        let settings = dir.path().join("settings");
        Remembered {
            folder: Some(PathBuf::from("/somewhere/Ipse")),
        }
        .write(&settings)
        .unwrap();
        assert_eq!(
            Remembered::read(&settings).folder,
            Some(PathBuf::from("/somewhere/Ipse"))
        );
        std::fs::write(settings.join("memory.json"), "not json").unwrap();
        assert!(Remembered::read(&settings).folder.is_none());
    }

    #[test]
    fn a_memory_is_made_only_in_a_new_or_empty_folder() {
        let dir = tempfile::tempdir().unwrap();
        let new = dir.path().join("Ipse");
        assert_eq!(can_create(&new), Ok(()));
        std::fs::create_dir(&new).unwrap();
        assert_eq!(can_create(&new), Ok(()));
        std::fs::write(new.join("notes.txt"), "mine").unwrap();
        assert_eq!(can_create(&new), Err(Taken::NotEmpty));
        let memory = dir.path().join("Memoire");
        std::fs::create_dir_all(memory.join(".ipse")).unwrap();
        std::fs::write(memory.join(".ipse/vault.toml"), "").unwrap();
        assert!(is_memory(&memory));
        assert_eq!(can_create(&memory), Err(Taken::Memory));
        assert_eq!(proposed(dir.path(), false), dir.path().join("Ipse 2"));
        assert_eq!(proposed(dir.path(), true), dir.path().join("Ipse demo"));
    }
}
