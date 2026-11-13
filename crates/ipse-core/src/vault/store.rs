//! A vault on disk: creating it and opening it (`docs/spec/vault.md`, sections 1, 6 and 8).

use std::path::{Path, PathBuf};

use super::atomic::write_atomic;
use super::fields::{Category, Common, EntityFields, FileType};
use super::file::MemoryFile;
use super::naming;
use super::settings::Settings;
use crate::{Error, Result};

/// Where the settings live, relative to the vault's folder.
pub const SETTINGS_FILE: &str = ".ipse/vault.toml";

/// The folders of a new vault.
const FOLDERS: [&str; 15] = [
    "people",
    "places",
    "organizations",
    "projects",
    "things",
    "topics",
    "works",
    "events",
    "states",
    "summaries/day",
    "summaries/week",
    "summaries/month",
    "summaries/year",
    "journal",
    ".ipse",
];

/// An opened vault: its folder, its settings and its owner's file.
#[derive(Debug, Clone)]
pub struct Vault {
    root: PathBuf,
    settings: Settings,
    owner: PathBuf,
}

impl Vault {
    /// Creates a vault in `root`, which may be empty or already hold notes (they are
    /// left untouched), and the owner's own file named after `owner_name`.
    ///
    /// # Errors
    ///
    /// [`Error::Refused`] when `root` already holds a vault or the name is empty;
    /// [`Error::Io`] when a folder or file cannot be created.
    pub fn init(root: &Path, owner_name: &str, settings: Settings) -> Result<Self> {
        let owner_name = owner_name.trim();
        if owner_name.is_empty() {
            return Err(Error::Refused(
                "Your name is needed to create your own file.".to_owned(),
            ));
        }
        let settings_path = root.join(SETTINGS_FILE);
        if exists(&settings_path)? {
            return Err(Error::Refused(format!(
                "{} already holds an Ipse memory. Open it instead of creating a new one.",
                root.display()
            )));
        }

        for folder in FOLDERS {
            let path = root.join(folder);
            std::fs::create_dir_all(&path).map_err(|source| Error::Io {
                action: "create",
                path,
                source,
            })?;
        }

        let mut common = Common::new(FileType::Person, settings.time_zone())
            .ok_or_else(|| Error::Refused("cannot create the owner's file".to_owned()))?;
        if let Some(identity) = Category::new("identity") {
            common.category = identity;
        }
        let owner = naming::entity_path(root, FileType::Person, Some(owner_name), &common.id)?;
        let entity = EntityFields {
            name: Some(owner_name.to_owned()),
            owner: true,
            ..EntityFields::default()
        };
        MemoryFile::new(&owner, common, Some(entity), "").write()?;

        // Written last: a folder is a vault only once everything else is in place.
        write_atomic(&settings_path, settings.render().as_bytes())?;

        Ok(Self {
            root: root.to_owned(),
            settings,
            owner,
        })
    }

    /// Opens the vault in `root`: reads its settings and finds its owner's file.
    ///
    /// # Errors
    ///
    /// [`Error::Refused`] when `root` holds no vault, or when not exactly one file in
    /// `people/` has `owner: true`; errors of [`Settings::read`].
    pub fn open(root: &Path) -> Result<Self> {
        let settings_path = root.join(SETTINGS_FILE);
        if !exists(&settings_path)? {
            return Err(Error::Refused(format!(
                "{} does not hold an Ipse memory. Create one with `ipse init`.",
                root.display()
            )));
        }
        let settings = Settings::read(&settings_path)?;
        let owner = find_owner(root)?;
        Ok(Self {
            root: root.to_owned(),
            settings,
            owner,
        })
    }

    /// The vault's folder.
    #[must_use]
    pub fn root(&self) -> &Path {
        &self.root
    }

    /// The vault's settings.
    #[must_use]
    pub fn settings(&self) -> &Settings {
        &self.settings
    }

    /// The owner's own file.
    #[must_use]
    pub fn owner_file(&self) -> &Path {
        &self.owner
    }
}

/// The one file of `people/` with `owner: true`. Files that cannot be read are skipped.
fn find_owner(root: &Path) -> Result<PathBuf> {
    let people = root.join("people");
    let mut owners = Vec::new();
    let entries = match std::fs::read_dir(&people) {
        Ok(entries) => entries,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            return Err(no_owner());
        }
        Err(source) => {
            return Err(Error::Io {
                action: "read",
                path: people,
                source,
            });
        }
    };
    for entry in entries.flatten() {
        let path = entry.path();
        let is_note = path.extension().is_some_and(|e| e == "md")
            && !entry.file_name().to_string_lossy().starts_with('.');
        if is_note
            && MemoryFile::read(&path)
                .ok()
                .is_some_and(|f| f.entity().is_some_and(|e| e.owner))
        {
            owners.push(path);
        }
    }
    match owners.len() {
        1 => Ok(owners.remove(0)),
        0 => Err(no_owner()),
        _ => Err(Error::Refused(
            "Several files in people/ say `owner: true`. Keep it only on your own file.".to_owned(),
        )),
    }
}

fn no_owner() -> Error {
    Error::Refused(
        "No file in people/ says `owner: true`. Add `owner: true` to your own file.".to_owned(),
    )
}

fn exists(path: &Path) -> Result<bool> {
    path.try_exists().map_err(|source| Error::Io {
        action: "check",
        path: path.to_owned(),
        source,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::vault::Language;

    fn settings() -> Settings {
        Settings::new(Language::Fr, "Europe/Paris").unwrap()
    }

    #[test]
    fn init_creates_folders_settings_and_the_owner_file() {
        let dir = tempfile::tempdir().unwrap();
        let vault = Vault::init(dir.path(), "Camille Durand", settings()).unwrap();
        for folder in FOLDERS {
            assert!(dir.path().join(folder).is_dir(), "{folder} missing");
        }
        assert_eq!(
            vault.owner_file(),
            dir.path().join("people/camille-durand.md")
        );

        let owner = MemoryFile::read(vault.owner_file()).unwrap();
        assert_eq!(owner.common().category.as_str(), "identity");
        let entity = owner.entity().unwrap();
        assert!(entity.owner);
        assert_eq!(entity.name.as_deref(), Some("Camille Durand"));
        let created = owner.common().created.to_string();
        assert!(
            created.ends_with("+01:00") || created.ends_with("+02:00"),
            "dates use the vault's time zone: {created}"
        );

        let opened = Vault::open(dir.path()).unwrap();
        assert_eq!(opened.owner_file(), vault.owner_file());
        assert_eq!(opened.settings().language(), Language::Fr);
    }

    #[test]
    fn init_leaves_existing_notes_untouched() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(dir.path().join("people")).unwrap();
        std::fs::write(dir.path().join("people/carnet.md"), "Mes notes.\n").unwrap();
        std::fs::write(dir.path().join("journal.md"), "Journal intime.\n").unwrap();

        Vault::init(dir.path(), "Camille Durand", settings()).unwrap();
        assert_eq!(
            std::fs::read_to_string(dir.path().join("people/carnet.md")).unwrap(),
            "Mes notes.\n"
        );
        assert_eq!(
            std::fs::read_to_string(dir.path().join("journal.md")).unwrap(),
            "Journal intime.\n"
        );
        assert!(
            Vault::open(dir.path()).is_ok(),
            "notes without front matter are skipped"
        );
    }

    #[test]
    fn init_twice_is_refused() {
        let dir = tempfile::tempdir().unwrap();
        Vault::init(dir.path(), "Camille Durand", settings()).unwrap();
        let err = Vault::init(dir.path(), "Camille Durand", settings()).unwrap_err();
        assert!(err.to_string().contains("already holds an Ipse memory"));
    }

    #[test]
    fn empty_name_is_refused() {
        let dir = tempfile::tempdir().unwrap();
        assert!(matches!(
            Vault::init(dir.path(), "  ", settings()),
            Err(Error::Refused(_))
        ));
        assert!(!dir.path().join(SETTINGS_FILE).exists());
    }

    #[test]
    fn open_refuses_a_newer_format() {
        let dir = tempfile::tempdir().unwrap();
        Vault::init(dir.path(), "Camille Durand", settings()).unwrap();
        std::fs::write(dir.path().join(SETTINGS_FILE), "format = 2\n").unwrap();
        assert!(matches!(
            Vault::open(dir.path()),
            Err(Error::UnsupportedVersion { found: 2, .. })
        ));
    }

    #[test]
    fn open_needs_exactly_one_owner() {
        let dir = tempfile::tempdir().unwrap();
        let vault = Vault::init(dir.path(), "Camille Durand", settings()).unwrap();
        let owner = std::fs::read_to_string(vault.owner_file()).unwrap();

        std::fs::write(dir.path().join("people/copie.md"), &owner).unwrap();
        let err = Vault::open(dir.path()).unwrap_err().to_string();
        assert!(err.contains("Several files"), "{err}");

        std::fs::remove_file(dir.path().join("people/copie.md")).unwrap();
        std::fs::write(vault.owner_file(), owner.replace("owner: true\n", "")).unwrap();
        let err = Vault::open(dir.path()).unwrap_err().to_string();
        assert!(err.contains("No file in people/"), "{err}");
    }

    #[test]
    fn open_an_empty_folder() {
        let dir = tempfile::tempdir().unwrap();
        let err = Vault::open(dir.path()).unwrap_err().to_string();
        assert!(err.contains("ipse init"), "{err}");
    }
}
