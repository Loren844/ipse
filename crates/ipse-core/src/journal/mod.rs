//! The raw journal: everything connectors bring back, append-only (`docs/spec/journal.md`).
//!
//! Records are JSON Lines, one segment per source and per month:
//! `journal/<source>/<YYYY>/<YYYY-MM>.jsonl`. Only the core writes here; connectors
//! send their records through the connector protocol.

mod canonical;
mod read;
mod record;
mod write;

pub use canonical::data_hash;
pub use read::Reader;
pub use record::{JournalRecord, RECORD_VERSION};
pub use write::Journal;

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
