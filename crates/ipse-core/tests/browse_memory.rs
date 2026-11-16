//! Browsing the whole memory (issue #128): every kind of entity, counted and listed,
//! and what links to a file, within each reader's rights. All data is invented.

// Test helpers may panic: a failed expectation is a failed test.
#![allow(clippy::expect_used, clippy::unwrap_used)]

use std::path::Path;

use ipse_core::rights::{Categories, Clients, Gateway};
use ipse_core::vault::{DateTime, FileType, History, Language, Settings, Vault};

fn write(root: &Path, rel: &str, i: usize, front: &str, body: &str) {
    let path = root.join(rel);
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(
        &path,
        format!(
            "---\nid: 01jcx8m2p4q6r8t0v2w4y6z8{i:02}\n{front}\ncreated: 2026-01-10T09:00:00+01:00\nupdated: 2026-01-10T09:00:00+01:00\n---\n\n{body}"
        ),
    )
    .unwrap();
}

fn memory(root: &Path) {
    Vault::init(
        root,
        "Inès Roux",
        Settings::new(Language::Fr, "Europe/Paris").unwrap(),
    )
    .unwrap();
    write(
        root,
        "people/marie-durand.md",
        1,
        "type: person\ncategory: people\nname: Marie Durand",
        "## Relations\n\n- travaille chez : [[atelier-nord]]\n",
    );
    write(
        root,
        "people/paul-roux.md",
        2,
        "type: person\ncategory: people\nname: Paul Roux",
        "## Relations\n\n- sœur : [[marie-durand|Marie]]\n- ami : [[marie-durand-2]]\n",
    );
    write(
        root,
        "people/medecin.md",
        3,
        "type: person\ncategory: health\nname: Dr Morel",
        "## Relations\n\n- patiente : [[marie-durand]]\n",
    );
    write(
        root,
        "places/chez-marie.md",
        4,
        "type: place\ncategory: places\nname: Chez Marie",
        "",
    );
    write(
        root,
        "organizations/atelier-nord.md",
        5,
        "type: organization\ncategory: work\nname: Atelier Nord",
        "",
    );
    write(
        root,
        "works/les-jours-de-sel.md",
        6,
        "type: work\ncategory: media\nname: Les Jours de sel",
        "",
    );
    write(
        root,
        "events/2026/2026-11-14-diner.md",
        7,
        "type: event\ncategory: agenda\ntitle: Dîner\nstart: 2026-11-14T20:00:00+01:00\nplace: \"[[chez-marie]]\"\nparticipants:\n  - \"[[marie-durand]]\"",
        "",
    );
    write(
        root,
        "events/2026/2026-10-01-cafe.md",
        8,
        "type: event\ncategory: agenda\ntitle: Café\nstart: 2026-10-01T09:00:00+02:00\nparticipants:\n  - \"[[marie-durand]]\"",
        "",
    );
    History::open(root).unwrap().record_outside_edits().unwrap();
}

#[test]
fn every_kind_is_counted_listed_and_linked_back() {
    let dir = tempfile::tempdir().unwrap();
    memory(dir.path());
    let mut gateway = Gateway::open(dir.path()).unwrap();
    let owner = gateway.fresh_owner().unwrap();

    let kinds: Vec<(FileType, usize)> = owner.kinds();
    let count = |t: FileType| kinds.iter().find(|(k, _)| *k == t).unwrap().1;
    assert_eq!(count(FileType::Person), 4, "with the owner and the doctor");
    assert_eq!(count(FileType::Place), 1);
    assert_eq!(count(FileType::Organization), 1);
    assert_eq!(count(FileType::Work), 1);
    assert_eq!(count(FileType::Thing), 0);

    let places = owner.entities(FileType::Place, None, 50, 0);
    assert_eq!(places.items[0].name, "Chez Marie");
    let found = owner.entities(FileType::Work, Some("sel"), 50, 0);
    assert_eq!(found.items[0].name, "Les Jours de sel");
    assert!(
        owner
            .entities(FileType::Place, Some("sel"), 50, 0)
            .items
            .is_empty()
    );

    let marie = owner.people(Some("Marie Durand"), 1, 0).items[0].id.clone();
    let back = owner.backlinks(&marie);
    let shown: Vec<(&str, Option<&str>)> = back
        .iter()
        .map(|b| (b.from.name.as_str(), b.word.as_deref()))
        .collect();
    assert_eq!(
        shown,
        [
            ("Dr Morel", Some("patiente")),
            ("Paul Roux", Some("sœur")),
            ("Dîner", None),
            ("Café", None),
        ],
        "relations by name (a display name counts, `marie-durand-2` does not), then events, most recent first"
    );
    let place = owner.entities(FileType::Place, None, 1, 0).items[0]
        .id
        .clone();
    let at = owner.backlinks(&place);
    assert_eq!(at.len(), 1);
    assert_eq!(at[0].from.name, "Dîner");
    assert_eq!(at[0].from.file_type.as_deref(), Some("event"));
}

#[test]
fn an_ai_sees_only_what_links_from_what_it_may_read() {
    let dir = tempfile::tempdir().unwrap();
    memory(dir.path());
    let vault = Vault::open(dir.path()).unwrap();
    let mut clients = Clients::load(dir.path()).unwrap();
    let categories = Categories::of(vault.settings());
    let (_, token) = clients
        .add(
            "Claude",
            DateTime::now(vault.settings().time_zone()),
            &categories,
            false,
        )
        .unwrap();
    let gateway = Gateway::open(dir.path()).unwrap();
    let ai = gateway.access(Some(token.expose())).unwrap();
    let count = |t: FileType| ai.kinds().into_iter().find(|(k, _)| *k == t).unwrap().1;
    assert_eq!(count(FileType::Person), 3, "not the doctor, in health");
    let marie = ai.people(Some("Marie Durand"), 1, 0).items[0].id.clone();
    let names: Vec<String> = ai
        .backlinks(&marie)
        .into_iter()
        .map(|b| b.from.name)
        .collect();
    assert_eq!(names, ["Paul Roux", "Dîner", "Café"]);
}
