//! Changing the memory's language and time zone (`docs/spec/vault.md`, section 8):
//! the rest of the settings file is kept, and the change is the person's.

// Test helpers may panic: a failed expectation is a failed test.
#![allow(clippy::expect_used, clippy::unwrap_used)]

use ipse_core::Error;
use ipse_core::rights::Categories;
use ipse_core::vault::{History, Language, SETTINGS_FILE, Settings, Source, Vault};

#[test]
fn the_language_and_time_zone_change_and_nothing_else() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    Vault::init(
        root,
        "Inès Roux",
        Settings::new(Language::Fr, "Europe/Paris").unwrap(),
    )
    .unwrap();
    Categories::declare(root, "Études", false).unwrap();
    let path = root.join(SETTINGS_FILE);
    let before = std::fs::read_to_string(&path).unwrap();

    let mut vault = Vault::open(root).unwrap();
    let settings = vault
        .change_settings(Language::En, "America/Montreal")
        .unwrap();
    assert_eq!(settings.language(), Language::En);
    assert_eq!(settings.time_zone_name(), "America/Montreal");

    let after = std::fs::read_to_string(&path).unwrap();
    assert_eq!(
        after,
        before
            .replace("language = \"fr\"", "language = \"en\"")
            .replace(
                "time_zone = \"Europe/Paris\"",
                "time_zone = \"America/Montreal\""
            ),
        "the comment, the format and the category stay as they were"
    );
    let reopened = Vault::open(root).unwrap();
    assert_eq!(reopened.settings().language(), Language::En);

    let history = History::open(root).unwrap();
    let last = &history.entries(1).unwrap()[0];
    assert_eq!(last.source, Some(Source::Person));
    assert_eq!(last.summary, "Change the settings");
    assert_eq!(last.files, [".ipse/vault.toml"]);

    // The same settings again: nothing to record.
    vault
        .change_settings(Language::En, "America/Montreal")
        .unwrap();
    assert_eq!(history.entries(1).unwrap()[0].id, last.id);
}

#[test]
fn an_unknown_time_zone_is_refused_and_nothing_changes() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    let mut vault = Vault::init(
        root,
        "Inès Roux",
        Settings::new(Language::Fr, "Europe/Paris").unwrap(),
    )
    .unwrap();
    let before = std::fs::read_to_string(root.join(SETTINGS_FILE)).unwrap();
    for bad in ["Paris", "Europe/Atlantis", ""] {
        assert!(
            matches!(
                vault.change_settings(Language::En, bad),
                Err(Error::Refused(_))
            ),
            "{bad:?}"
        );
    }
    assert_eq!(
        std::fs::read_to_string(root.join(SETTINGS_FILE)).unwrap(),
        before
    );
    assert_eq!(vault.settings().language(), Language::Fr);
}
