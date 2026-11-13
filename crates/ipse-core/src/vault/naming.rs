//! Choosing the file name of a new entity (`docs/spec/vault.md`, section 2.3).

use std::path::{Path, PathBuf};

use super::event::EventTime;
use super::fields::{FileType, Id};
use super::state::Topic;
use crate::slug::to_file_stem;
use crate::{Error, Result};

/// Highest suffix tried before giving up: `marie-dupont-9999.md`.
const MAX_SUFFIX: u32 = 9_999;

/// The path of a new entity file in `vault`: its type's folder, a stem made from `name`
/// (or the `id` when the name gives nothing usable), and `-2`, `-3`… when the name is
/// taken.
///
/// # Errors
///
/// [`Error::Io`] when the folder cannot be checked; [`Error::Invalid`] when every
/// suffix up to 9 999 is taken.
pub fn entity_path(
    vault: &Path,
    file_type: FileType,
    name: Option<&str>,
    id: &Id,
) -> Result<PathBuf> {
    let stem = name
        .and_then(to_file_stem)
        .unwrap_or_else(|| id.to_string());
    free_path(&vault.join(file_type.folder()), &stem)
}

/// The path of a new event file: `events/<YYYY>/<YYYY-MM-DD>-<stem>.md`, the date being
/// the start's day in the event's own offset, the stem made from `title` (or the `id`),
/// with `-2`, `-3`… when taken.
///
/// # Errors
///
/// As [`entity_path`].
pub fn event_path(vault: &Path, title: &str, start: &EventTime, id: &Id) -> Result<PathBuf> {
    let day = start.date();
    let stem = to_file_stem(title).unwrap_or_else(|| id.to_string());
    let dir = vault
        .join(FileType::Event.folder())
        .join(format!("{:04}", day.year()));
    free_path(&dir, &format!("{day}-{stem}"))
}

/// The path of the state log of `topic` for `year`: `states/<topic>/<YYYY>.md`.
#[must_use]
pub fn state_log_path(vault: &Path, topic: &Topic, year: i16) -> PathBuf {
    vault
        .join(FileType::StateLog.folder())
        .join(topic.as_str())
        .join(format!("{year:04}.md"))
}

/// `dir/stem.md`, or the first free `dir/stem-N.md`.
///
/// # Errors
///
/// As [`entity_path`].
pub fn free_path(dir: &Path, stem: &str) -> Result<PathBuf> {
    let candidate = dir.join(format!("{stem}.md"));
    if !exists(&candidate)? {
        return Ok(candidate);
    }
    for n in 2..=MAX_SUFFIX {
        let candidate = dir.join(format!("{stem}-{n}.md"));
        if !exists(&candidate)? {
            return Ok(candidate);
        }
    }
    Err(Error::Invalid {
        path: dir.to_owned(),
        line: None,
        problem: "too many files share the same name in this folder".to_owned(),
    })
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

    #[test]
    fn stem_from_name_then_suffixes() {
        let vault = tempfile::tempdir().unwrap();
        let id = Id::generate();
        let first = entity_path(vault.path(), FileType::Person, Some("Marie Dupont"), &id).unwrap();
        assert_eq!(first, vault.path().join("people/marie-dupont.md"));

        std::fs::create_dir_all(first.parent().unwrap()).unwrap();
        std::fs::write(&first, "").unwrap();
        let second =
            entity_path(vault.path(), FileType::Person, Some("Marie DUPONT"), &id).unwrap();
        assert_eq!(second, vault.path().join("people/marie-dupont-2.md"));

        std::fs::write(&second, "").unwrap();
        let third = entity_path(vault.path(), FileType::Person, Some("marie dupont"), &id).unwrap();
        assert_eq!(third, vault.path().join("people/marie-dupont-3.md"));
    }

    #[test]
    fn event_and_state_paths() {
        let vault = tempfile::tempdir().unwrap();
        let id = Id::generate();
        let start = EventTime::parse("2026-11-14T23:30:00-05:00").unwrap();
        assert_eq!(
            event_path(vault.path(), "Dîner chez Julie", &start, &id).unwrap(),
            vault
                .path()
                .join("events/2026/2026-11-14-diner-chez-julie.md"),
            "the start's day in its own offset"
        );
        let day = EventTime::parse("2027-01-01").unwrap();
        assert_eq!(
            event_path(vault.path(), "", &day, &id).unwrap(),
            vault.path().join(format!("events/2027/2027-01-01-{id}.md"))
        );
        let music = Topic::new("music").unwrap();
        assert_eq!(
            state_log_path(vault.path(), &music, 2026),
            vault.path().join("states/music/2026.md")
        );
    }

    #[test]
    fn id_when_the_name_gives_nothing() {
        let vault = tempfile::tempdir().unwrap();
        let id = Id::generate();
        let path = entity_path(vault.path(), FileType::Thing, Some("🎉"), &id).unwrap();
        assert_eq!(path, vault.path().join(format!("things/{id}.md")));
        let path = entity_path(vault.path(), FileType::Thing, None, &id).unwrap();
        assert_eq!(path, vault.path().join(format!("things/{id}.md")));
    }
}
