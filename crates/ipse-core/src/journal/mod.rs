//! The raw journal: everything connectors bring back, append-only (`docs/spec/journal.md`).
//!
//! Records are JSON Lines, one segment per source and per month:
//! `journal/<source>/<YYYY>/<YYYY-MM>.jsonl`, sealed and compressed with Zstandard once
//! the month is over (`<YYYY-MM>.jsonl.zst`, with its SHA-256). Only the core writes
//! here; connectors send their records through the connector protocol.

mod canonical;
mod latest;
mod read;
mod record;
mod segment;
mod write;

pub use canonical::data_hash;
pub use read::Reader;
pub use record::{JournalRecord, RECORD_VERSION};
pub use write::Journal;

/// The sealed months of every source of the vault in `vault_root` that do not match
/// their checksum. They are kept as they are, never overwritten, and skipped when read.
///
/// # Errors
///
/// [`crate::Error::Io`] when the journal's folder exists but cannot be listed.
pub fn damaged_segments(vault_root: &std::path::Path) -> crate::Result<Vec<std::path::PathBuf>> {
    let root = vault_root.join("journal");
    let sources = match std::fs::read_dir(&root) {
        Ok(entries) => entries,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(source) => {
            return Err(crate::Error::Io {
                action: "read",
                path: root,
                source,
            });
        }
    };
    let mut damaged = Vec::new();
    for source in sources.flatten() {
        if source.file_type().is_ok_and(|t| t.is_dir()) {
            for s in segment::list(&source.path())? {
                if s.sealed && !segment::verify(&s) {
                    damaged.push(s.path);
                }
            }
        }
    }
    damaged.sort();
    Ok(damaged)
}

/// Checks a source name: lowercase letters, digits and inner dashes, like `calendar-2`.
#[must_use]
pub fn is_source_name(name: &str) -> bool {
    !name.is_empty()
        && !name.starts_with('-')
        && !name.ends_with('-')
        && name
            .bytes()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-')
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn source_names() {
        assert!(is_source_name("calendar"));
        assert!(is_source_name("calendar-2"));
        for bad in ["", "Calendar", "../x", "a b", "-a", "a-", "a/b"] {
            assert!(!is_source_name(bad), "{bad:?}");
        }
    }
}
