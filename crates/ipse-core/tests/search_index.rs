//! The saved, incremental search index (#16). All data is invented.

// Test helpers may panic: a failed expectation is a failed test.
#![allow(clippy::expect_used, clippy::unwrap_used)]

use std::path::{Path, PathBuf};

use ipse_core::search::{INDEX_FILE, SearchIndex};
use ipse_core::time::TimeZone;
use ipse_core::vault::{
    Body, Category, Common, EntityFields, EventFields, EventTime, Fact, FileType, Language,
    LinkOrText, MemoryFile, Relation, WikiLink, body, naming,
};

/// A folder with French settings, as `ipse init` writes them.
fn vault(language: &str) -> tempfile::TempDir {
    let dir = tempfile::tempdir().unwrap();
    std::fs::create_dir_all(dir.path().join(".ipse")).unwrap();
    std::fs::write(
        dir.path().join(".ipse/vault.toml"),
        format!("format = 1\nlanguage = \"{language}\"\ntime_zone = \"Europe/Paris\"\n"),
    )
    .unwrap();
    dir
}

fn person(
    root: &Path,
    category: &str,
    name: &str,
    facts: &[&str],
    relations: &[(&str, &str)],
) -> PathBuf {
    let mut common = Common::new(FileType::Person, &TimeZone::UTC).unwrap();
    common.category = Category::new(category).unwrap();
    let path = naming::entity_path(root, FileType::Person, Some(name), &common.id).unwrap();
    let entity = EntityFields {
        name: Some(name.into()),
        ..EntityFields::default()
    };
    let sections = Body {
        facts: facts
            .iter()
            .map(|f| Fact {
                text: (*f).to_owned(),
                id: None,
            })
            .collect(),
        relations: relations
            .iter()
            .map(|(word, target)| Relation {
                word: (*word).to_owned(),
                link: WikiLink::parse(&format!("[[{target}]]")).unwrap(),
            })
            .collect(),
        notes: None,
    };
    MemoryFile::new(
        &path,
        common,
        Some(entity),
        body::render(&sections, Language::Fr),
    )
    .write()
    .unwrap();
    path
}

fn names(index: &SearchIndex, query: &str) -> Vec<String> {
    index
        .search(query, 50, |_| true)
        .into_iter()
        .map(|h| h.name)
        .collect()
}

#[test]
fn plurals_match_their_singular() {
    let dir = vault("fr");
    person(
        dir.path(),
        "people",
        "Marie Dupont",
        &["Élève des chevaux."],
        &[],
    );
    person(
        dir.path(),
        "people",
        "Paul Durand",
        &["Collectionne les journaux anciens."],
        &[],
    );
    let index = SearchIndex::load(dir.path()).unwrap();
    assert_eq!(names(&index, "cheval"), ["Marie Dupont"]);
    assert_eq!(names(&index, "chevaux"), ["Marie Dupont"]);
    assert_eq!(names(&index, "journal ancien"), ["Paul Durand"]);
    assert_eq!(names(&index, "Elodie"), Vec::<String>::new());

    let en = vault("en");
    person(
        en.path(),
        "people",
        "Ana Silva",
        &["Visited three cities this year."],
        &[],
    );
    let index = SearchIndex::load(en.path()).unwrap();
    assert_eq!(names(&index, "city"), ["Ana Silva"]);
}

#[test]
fn relations_events_and_summaries_are_searchable() {
    let dir = vault("fr");
    let root = dir.path();
    person(
        root,
        "identity",
        "Camille Durand",
        &[],
        &[("mère", "marie-dupont")],
    );
    person(root, "people", "Marie Dupont", &[], &[]);

    let mut common = Common::new(FileType::Event, &TimeZone::UTC).unwrap();
    common.category = Category::new("agenda").unwrap();
    let start = EventTime::parse("2026-11-14").unwrap();
    let mut fields = EventFields::new("Dîner du samedi", start);
    fields.participants = vec![
        LinkOrText::parse("[[marie-dupont]]"),
        LinkOrText::parse("Zoé"),
    ];
    let path = naming::event_path(root, "Dîner du samedi", &start, &common.id).unwrap();
    MemoryFile::new_event(
        &path,
        common,
        fields,
        "\n## Notes\n\nApporter le dessert.\n",
    )
    .write()
    .unwrap();

    let summary = root.join("summaries/day/2026/2026-11-14.md");
    std::fs::create_dir_all(summary.parent().unwrap()).unwrap();
    std::fs::write(
        &summary,
        "---\nid: 01jcx8m2p4q6r8t0v2w4y6z8a0\ntype: summary\ncategory: summaries\ncreated: 2026-11-14T23:00:00+01:00\nupdated: 2026-11-14T23:00:00+01:00\n---\n\n## Résumé\n\nUne journée calme, promenade au bord du lac.\n",
    )
    .unwrap();

    let index = SearchIndex::load(root).unwrap();
    assert_eq!(names(&index, "mère"), ["Camille Durand"], "relation word");
    assert_eq!(
        names(&index, "dessert zoé"),
        ["Dîner du samedi"],
        "notes and a participant's name"
    );
    let found = index.search("lac", 10, |_| true);
    assert_eq!(found.len(), 1, "{found:?}");
    assert_eq!(found[0].file_type, FileType::Summary);
    assert!(found[0].excerpt.contains("bord du lac"));
}

#[test]
fn the_category_filter_is_inside_the_query() {
    let dir = vault("fr");
    person(
        dir.path(),
        "people",
        "Marie Dupont",
        &["Joue du violon."],
        &[],
    );
    person(
        dir.path(),
        "health",
        "Marie Secret",
        &["Joue du violon."],
        &[],
    );
    let index = SearchIndex::load(dir.path()).unwrap();
    let people = |c: &Category| c.as_str() == "people";
    let found = index.search_with("violon", 1, people, |_| None);
    assert_eq!(found.hits.len(), 1);
    assert_eq!(found.hits[0].name, "Marie Dupont");
    assert!(!found.more, "a hidden file is not counted as one more");
}

#[test]
fn only_the_files_that_changed_are_read_again() {
    let dir = vault("fr");
    let marie = person(
        dir.path(),
        "people",
        "Marie Dupont",
        &["Joue du violon."],
        &[],
    );
    let paul = person(
        dir.path(),
        "people",
        "Paul Durand",
        &["Joue du piano."],
        &[],
    );
    let mut index = SearchIndex::load(dir.path()).unwrap();
    assert!(!index.update(dir.path()).unwrap(), "nothing changed");

    // Marie's fact is edited, as in an editor.
    let text = std::fs::read_to_string(&marie).unwrap();
    std::fs::write(&marie, text.replace("violon", "violoncelle et du tuba")).unwrap();
    // Paul's file changes too, but keeps its size and date: it is not read again.
    let modified = std::fs::metadata(&paul).unwrap().modified().unwrap();
    let text = std::fs::read_to_string(&paul).unwrap();
    std::fs::write(&paul, text.replace("piano", "banjo")).unwrap();
    std::fs::File::options()
        .write(true)
        .open(&paul)
        .unwrap()
        .set_modified(modified)
        .unwrap();

    assert!(index.changed(dir.path()).unwrap());
    assert!(index.update(dir.path()).unwrap());
    assert_eq!(names(&index, "tuba"), ["Marie Dupont"]);
    assert_eq!(names(&index, "violoncelle"), ["Marie Dupont"]);
    assert_eq!(names(&index, "piano"), ["Paul Durand"], "not read again");
    assert!(names(&index, "banjo").is_empty());

    // A new file and a removed one.
    person(dir.path(), "people", "Léa Moreau", &["Joue du tuba."], &[]);
    std::fs::remove_file(&marie).unwrap();
    assert!(index.update(dir.path()).unwrap());
    assert_eq!(names(&index, "tuba"), ["Léa Moreau"]);
    assert_eq!(index.len(), 2);
}

#[test]
fn the_saved_index_gives_the_same_results_as_a_rebuild() {
    let dir = vault("fr");
    let root = dir.path();
    for i in 0..300 {
        person(
            root,
            if i % 3 == 0 { "health" } else { "people" },
            &format!("Personne Numéro {i}"),
            &[
                &format!("Habite rue des Lilas, numéro {i}."),
                "Aime les châteaux.",
            ],
            &[("ami", &format!("personne-numero-{}", (i + 1) % 300))],
        );
    }
    let first = SearchIndex::load(root).unwrap();
    assert!(root.join(INDEX_FILE).is_file(), "saved");
    let saved = SearchIndex::load(root).unwrap();
    let rebuilt = SearchIndex::build(root).unwrap();
    let people = |c: &Category| c.as_str() == "people";
    for query in ["personne 42", "chateau", "lilas 7", "ami", "numero"] {
        let a = first.search_with(query, 50, people, |_| None);
        assert_eq!(a, saved.search_with(query, 50, people, |_| None), "{query}");
        assert_eq!(
            a,
            rebuilt.search_with(query, 50, people, |_| None),
            "{query}"
        );
    }
    assert_eq!(saved.len(), 300);

    // Deleting the index only means it is read again from the files.
    std::fs::remove_dir_all(root.join(".ipse/index")).unwrap();
    let again = SearchIndex::load(root).unwrap();
    assert!(root.join(INDEX_FILE).is_file());
    assert_eq!(
        again.search_with("chateau", 50, people, |_| None),
        first.search_with("chateau", 50, people, |_| None)
    );

    // A damaged index too.
    std::fs::write(root.join(INDEX_FILE), b"not an index").unwrap();
    assert_eq!(SearchIndex::load(root).unwrap().len(), 300);
}

#[test]
fn an_index_saved_in_another_language_is_read_again() {
    let dir = vault("en");
    person(
        dir.path(),
        "people",
        "Marie Dupont",
        &["Élève des chevaux."],
        &[],
    );
    assert!(names(&SearchIndex::load(dir.path()).unwrap(), "cheval").is_empty());
    std::fs::write(
        dir.path().join(".ipse/vault.toml"),
        "format = 1\nlanguage = \"fr\"\ntime_zone = \"Europe/Paris\"\n",
    )
    .unwrap();
    assert_eq!(
        names(&SearchIndex::load(dir.path()).unwrap(), "cheval"),
        ["Marie Dupont"]
    );
}
