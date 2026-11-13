//! Rights: which AI may read what (`docs/spec/rights.md`).
//!
//! Every connected AI is a client with its own access code. Every read goes through
//! [`Access`], which only returns what the client's categories allow, masks links to
//! anything else, and never hints that hidden data exists.

mod access;
mod clients;

pub use access::{Access, Entry, Gateway, RelationOut};
pub use clients::{Client, Clients, Token};

/// The categories of rights format 1, and whether each is sensitive.
pub const CATEGORIES: [(&str, bool); 12] = [
    ("identity", false),
    ("people", false),
    ("agenda", false),
    ("places", false),
    ("media", false),
    ("work", false),
    ("things", false),
    ("summaries", false),
    ("location", true),
    ("home", true),
    ("health", true),
    ("private", true),
];

/// The category a file's category counts as: itself when known, `private` otherwise.
#[must_use]
pub fn effective(category: &str) -> &str {
    if CATEGORIES.iter().any(|(name, _)| *name == category) {
        category
    } else {
        "private"
    }
}

/// The categories a new client may read: every category that is not sensitive.
#[must_use]
pub fn default_read() -> Vec<String> {
    CATEGORIES
        .iter()
        .filter(|(_, sensitive)| !sensitive)
        .map(|(name, _)| (*name).to_owned())
        .collect()
}

/// The sensitive categories, closed until the person opens them for one client.
#[must_use]
pub fn sensitive() -> Vec<&'static str> {
    CATEGORIES
        .iter()
        .filter(|(_, sensitive)| *sensitive)
        .map(|(name, _)| *name)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unknown_categories_count_as_private() {
        assert_eq!(effective("people"), "people");
        assert_eq!(effective("studies"), "private");
        assert_eq!(effective(""), "private");
    }

    #[test]
    fn new_clients_read_everything_but_sensitive_categories() {
        let read = default_read();
        assert_eq!(read.len(), 8);
        for closed in sensitive() {
            assert!(!read.iter().any(|c| c == closed), "{closed}");
        }
    }
}
