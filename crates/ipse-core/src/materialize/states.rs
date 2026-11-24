//! Writing a state log, `states/<topic>/<year>.md` (`docs/spec/vault.md`, section 5),
//! from rows a source's records give.

use std::path::{Path, PathBuf};

use jiff::tz::TimeZone;

use crate::vault::state::{StateLogFields, StateRow, Topic, render_table};
use crate::vault::{Category, Common, DateTime, FileType, Id, MemoryFile, Vault, naming};
use crate::{Error, Result};

/// Rewrites `states/<topic>/<year>.md` with `rows`, creating it in `category` when it
/// is missing; returns its path when it changed. The file is not synced to disk: the
/// caller syncs what it wrote.
///
/// # Errors
///
/// [`Error::Refused`] for a topic or a category that is not a name; errors reading or
/// writing the file.
pub(crate) fn write_state_log(
    vault: &Vault,
    topic: &str,
    category: &str,
    year: i16,
    rows: &[StateRow],
    zone: &TimeZone,
) -> Result<Option<PathBuf>> {
    let topic = Topic::new(topic).ok_or_else(|| Error::Refused("bad topic".to_owned()))?;
    let path = naming::state_log_path(vault.root(), &topic, year);
    let body = render_table(rows);
    let mut file = match MemoryFile::read(&path) {
        Ok(file) => file,
        Err(Error::Io { source, .. }) if source.kind() == std::io::ErrorKind::NotFound => {
            let now = DateTime::now(zone);
            let common = Common {
                id: Id::generate(),
                file_type: FileType::StateLog,
                category: Category::new(category)
                    .ok_or_else(|| Error::Refused("bad category".to_owned()))?,
                created: now,
                updated: now,
                sources: Vec::new(),
            };
            create_dir(&path)?;
            MemoryFile::new_state_log(&path, common, StateLogFields { topic, year }, String::new())
        }
        Err(e) => return Err(e),
    };
    if file.body() == body {
        return Ok(None);
    }
    file.set_body(body);
    file.common_mut().updated = DateTime::now(zone);
    Ok(file.write_unsynced()?.then_some(path))
}

fn create_dir(path: &Path) -> Result<()> {
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir).map_err(|source| Error::Io {
            action: "create",
            path: dir.to_owned(),
            source,
        })?;
    }
    Ok(())
}
