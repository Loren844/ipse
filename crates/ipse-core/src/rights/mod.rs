//! Rights: which AI may read what (`docs/spec/rights.md`).
//!
//! Every connected AI is a client with its own access code. Every read goes through
//! [`Access`], which only returns what the client's categories allow, masks links to
//! anything else, and never hints that hidden data exists.

mod access;
mod clients;
mod log;

pub use access::{
    Access, Entry, Events, FULL_CHECK_EVERY, Gateway, MAX_EVENTS, MAX_STATES, RelationOut,
    StateOut, States,
};
pub use clients::{Client, Clients, Token};
pub use log::{ACCESS_DIR, KEEP_MONTHS};

use std::collections::BTreeMap;

use crate::vault::Settings;

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

/// The categories of one vault: those of the spec, and those the person declared in
/// `.ipse/vault.toml` (`docs/spec/rights.md`, section 1.1). One of the spec's names
/// declared again keeps the spec's meaning.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Categories {
    /// The person's own categories: name, and whether it is sensitive.
    custom: BTreeMap<String, bool>,
}

impl Categories {
    /// The categories of a vault with these settings.
    #[must_use]
    pub fn of(settings: &Settings) -> Self {
        let custom = settings
            .categories()
            .iter()
            .filter(|(name, _)| !is_builtin(name))
            .map(|(name, c)| (name.clone(), c.sensitive))
            .collect();
        Self { custom }
    }

    /// The category a file's category counts as: itself when known, `private`
    /// otherwise.
    #[must_use]
    pub fn effective<'a>(&self, category: &'a str) -> &'a str {
        if is_builtin(category) || self.custom.contains_key(category) {
            category
        } else {
            "private"
        }
    }

    /// The categories a new client may read: every one that is not sensitive.
    #[must_use]
    pub fn default_read(&self) -> Vec<String> {
        CATEGORIES
            .iter()
            .map(|(name, sensitive)| (*name, *sensitive))
            .chain(self.custom.iter().map(|(n, s)| (n.as_str(), *s)))
            .filter(|(_, sensitive)| !sensitive)
            .map(|(name, _)| name.to_owned())
            .collect()
    }

    /// The sensitive categories, closed until the person opens them for one client.
    #[must_use]
    pub fn sensitive(&self) -> Vec<&str> {
        CATEGORIES
            .iter()
            .map(|(name, sensitive)| (*name, *sensitive))
            .chain(self.custom.iter().map(|(n, s)| (n.as_str(), *s)))
            .filter(|(_, sensitive)| *sensitive)
            .map(|(name, _)| name)
            .collect()
    }
}

fn is_builtin(category: &str) -> bool {
    CATEGORIES.iter().any(|(name, _)| *name == category)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::vault::Language;

    fn with(declared: &str) -> Categories {
        let text = format!("format = 1\n{declared}");
        Categories::of(&Settings::parse(std::path::Path::new("vault.toml"), &text).unwrap())
    }

    #[test]
    fn unknown_categories_count_as_private() {
        let categories = Categories::default();
        assert_eq!(categories.effective("people"), "people");
        assert_eq!(categories.effective("studies"), "private");
        assert_eq!(categories.effective(""), "private");
        let declared = with("[categories.studies]\nsensitive = false\n");
        assert_eq!(declared.effective("studies"), "studies");
        assert_eq!(declared.effective("sport"), "private");
    }

    #[test]
    fn new_clients_read_everything_but_sensitive_categories() {
        let categories = Categories::of(&Settings::new(Language::En, "UTC").unwrap());
        let read = categories.default_read();
        assert_eq!(read.len(), 8);
        for closed in categories.sensitive() {
            assert!(!read.iter().any(|c| c == closed), "{closed}");
        }
        let declared = with(
            "[categories.studies]\nsensitive = false\n[categories.therapy]\nsensitive = true\n[categories.club]\n",
        );
        let read = declared.default_read();
        assert!(read.contains(&"studies".to_owned()));
        assert!(!read.contains(&"therapy".to_owned()), "sensitive");
        assert!(
            !read.contains(&"club".to_owned()),
            "sensitive when left out"
        );
        assert!(declared.sensitive().contains(&"therapy"));
    }

    #[test]
    fn the_spec_categories_cannot_be_redefined() {
        let declared = with("[categories.health]\nsensitive = false\n");
        assert!(declared.sensitive().contains(&"health"));
        assert!(!declared.default_read().contains(&"health".to_owned()));
    }
}
