//! Notes of the day (issue #277): written at once as the person's, one file a day, and
//! a mention links the person or place it names. All data is invented.

// Test helpers may panic: a failed expectation is a failed test.
#![allow(clippy::expect_used, clippy::unwrap_used)]

use ipse_core::notes;
use ipse_core::rights::{Clients, Gateway};
use ipse_core::time::TimeZone;
use ipse_core::vault::{DateTime, History, Language, Settings, Source, Vault};

const AARON: &str = "---
id: 01jcx8m2p4q6r8t0v2w4y6z811
type: person
category: people
created: 2026-01-10T09:00:00+01:00
updated: 2026-01-10T09:00:00+01:00
name: Aaron Petit
---
";

fn vault(dir: &std::path::Path) -> Vault {
    Vault::init(
        dir,
        "Inès Roux",
        Settings::new(Language::Fr, "Europe/Paris").unwrap(),
    )
    .unwrap();
    std::fs::write(dir.join("people/aaron-petit.md"), AARON).unwrap();
    History::open(dir).unwrap().record_outside_edits().unwrap();
    Vault::open(dir).unwrap()
}

fn today() -> String {
    jiff::Timestamp::now()
        .to_zoned(jiff::tz::TimeZone::get("Europe/Paris").unwrap())
        .date()
        .to_string()
}

#[test]
fn notes_of_the_day_go_in_one_file_by_the_person() {
    let dir = tempfile::tempdir().unwrap();
    let vault = vault(dir.path());
    let first = notes::add(&vault, "  Acheter du pain. ").unwrap();
    let second = notes::add(&vault, "Appeler le plombier.").unwrap();
    assert_eq!(first.day, today());
    assert_eq!(
        second.file,
        format!("notes/{}/{}.md", &today()[..4], today())
    );
    assert_eq!(second.lines.len(), 2);
    assert!(
        second.lines[0].ends_with(" : Acheter du pain."),
        "{:?}",
        second.lines
    );
    assert!(second.lines[1].ends_with(" : Appeler le plombier."));

    let text = std::fs::read_to_string(dir.path().join(&second.file)).unwrap();
    assert!(text.contains("type: note\n"), "{text}");
    assert!(
        text.contains("category: private\n"),
        "closed to AIs: {text}"
    );
    assert!(text.contains("## Notes\n\n- "), "{text}");
    assert_eq!(notes::read(dir.path(), &today()), Some(second));

    let entries = vault.history().unwrap().entries(2).unwrap();
    for entry in &entries {
        assert_eq!(entry.source, Some(Source::Person));
        assert_eq!(entry.summary, "Add a note of the day");
    }
    assert!(notes::add(&vault, "   ").is_err());
    assert!(notes::read(dir.path(), "2020-01-01").is_none());
}

#[test]
fn a_note_with_a_mention_links_the_file() {
    let dir = tempfile::tempdir().unwrap();
    let vault = vault(dir.path());
    notes::add(&vault, "[[aaron-petit|Aaron]] part vivre à Lyon en mars.").unwrap();

    let mut gateway = Gateway::open(dir.path()).unwrap();
    let owner = gateway.fresh_owner().unwrap();
    let back = owner.backlinks("01jcx8m2p4q6r8t0v2w4y6z811");
    assert_eq!(back.len(), 1, "{back:?}");
    assert_eq!(back[0].from.name, today());
    assert_eq!(back[0].from.file_type.as_deref(), Some("note"));
    let found = owner.search("Lyon", 10);
    assert!(
        found.hits.iter().any(|h| h.name == today()),
        "the note is searched"
    );

    // An AI does not see it until the person opens `private` to it.
    let mut clients = Clients::load(dir.path()).unwrap();
    let (_, token) = clients
        .add(
            "Claude",
            DateTime::now(&TimeZone::UTC),
            &ipse_core::rights::Categories::default(),
            false,
        )
        .unwrap();
    let ai = gateway.fresh_access(Some(token.expose())).unwrap();
    assert!(ai.search("Lyon", 10).hits.is_empty());
}
