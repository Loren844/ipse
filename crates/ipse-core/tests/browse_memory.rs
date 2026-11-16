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

/// The names of a graph's files, with their steps away.
fn shown(graph: &ipse_core::rights::GraphOut) -> Vec<(&str, u8)> {
    graph
        .nodes
        .iter()
        .map(|n| (n.name.as_str(), n.depth))
        .collect()
}

#[test]
fn the_graph_of_a_file_is_its_neighbourhood() {
    let dir = tempfile::tempdir().unwrap();
    memory(dir.path());
    let mut gateway = Gateway::open(dir.path()).unwrap();
    let owner = gateway.fresh_owner().unwrap();
    let marie = owner.people(Some("Marie Durand"), 1, 0).items[0].id.clone();

    let one = owner.graph(&marie, 1, &[], None, 100).unwrap();
    assert_eq!(
        shown(&one),
        [
            ("Marie Durand", 0),
            ("Atelier Nord", 1),
            ("Dr Morel", 1),
            ("Paul Roux", 1),
            ("Dîner", 1),
            ("Café", 1),
        ],
        "what it links to and what links to it; entities by name, then events, most recent first"
    );
    assert!(one.nodes[2].sensitive, "health is marked");
    let words: Vec<(&str, &str, Option<&str>)> = one
        .edges
        .iter()
        .map(|e| {
            (
                one.nodes[e.from].name.as_str(),
                one.nodes[e.to].name.as_str(),
                e.word.as_deref(),
            )
        })
        .collect();
    assert!(words.contains(&("Marie Durand", "Atelier Nord", Some("travaille chez"))));
    assert!(words.contains(&("Paul Roux", "Marie Durand", Some("sœur"))));
    assert!(words.contains(&("Dîner", "Marie Durand", None)));
    assert_eq!(one.more, 0);

    let two = owner.graph(&marie, 2, &[], None, 100).unwrap();
    assert!(
        shown(&two).contains(&("Chez Marie", 2)),
        "the dinner's place, two steps away"
    );
    assert!(
        two.edges
            .iter()
            .any(|e| two.nodes[e.from].name == "Dîner" && two.nodes[e.to].name == "Chez Marie")
    );

    // Filters: the file at the center stays.
    let people = owner
        .graph(&marie, 2, &[FileType::Person], None, 100)
        .unwrap();
    assert_eq!(
        shown(&people),
        [("Marie Durand", 0), ("Dr Morel", 1), ("Paul Roux", 1)]
    );
    let work = owner
        .graph(&marie, 1, &[], Some(&["work".to_owned()]), 100)
        .unwrap();
    assert_eq!(shown(&work), [("Marie Durand", 0), ("Atelier Nord", 1)]);
    let place = owner.entities(FileType::Place, None, 1, 0).items[0]
        .id
        .clone();
    let alone = owner
        .graph(&place, 1, &[FileType::Person], None, 100)
        .unwrap();
    assert_eq!(shown(&alone), [("Chez Marie", 0)]);

    // A limit keeps the nearest, and says how many are left out.
    let small = owner.graph(&marie, 2, &[], None, 3).unwrap();
    assert_eq!(
        shown(&small),
        [("Marie Durand", 0), ("Atelier Nord", 1), ("Dr Morel", 1)]
    );
    assert_eq!(small.more, 3);

    assert!(
        owner
            .graph("01jcx8m2p4q6r8t0v2w4y6z899", 1, &[], None, 10)
            .is_none()
    );
    assert!(owner.graph("nothing", 1, &[], None, 10).is_none());
}

#[test]
fn an_ai_graph_holds_only_what_it_may_read() {
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
    let marie = ai.people(Some("Marie Durand"), 1, 0).items[0].id.clone();
    let graph = ai.graph(&marie, 2, &[], None, 100).unwrap();
    assert!(!graph.nodes.iter().any(|n| n.name == "Dr Morel"));
    let doctor = "01jcx8m2p4q6r8t0v2w4y6z803";
    assert!(
        ai.graph(doctor, 1, &[], None, 10).is_none(),
        "a hidden file has no graph"
    );
}
