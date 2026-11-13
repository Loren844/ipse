//! Searching a vault's entities.

// Test helpers may panic: a failed expectation is a failed test.
#![allow(clippy::expect_used, clippy::unwrap_used)]

use std::path::Path;

use ipse_core::search::{MAX_LIMIT, SearchIndex};
use ipse_core::time::TimeZone;
use ipse_core::vault::{
    Body, Category, Common, EntityFields, Fact, FileType, Language, MemoryFile, body, naming,
};

/// Writes an entity file and returns its stem.
fn entity(
    root: &Path,
    file_type: FileType,
    category: &str,
    name: &str,
    aliases: &[&str],
    facts: &[&str],
    notes: Option<&str>,
) -> String {
    let mut common = Common::new(file_type, &TimeZone::UTC).unwrap();
    common.category = Category::new(category).unwrap();
    let path = naming::entity_path(root, file_type, Some(name), &common.id).unwrap();
    let entity = EntityFields {
        name: Some(name.into()),
        aliases: aliases.iter().map(|a| (*a).to_owned()).collect(),
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
        relations: Vec::new(),
        notes: notes.map(str::to_owned),
    };
    MemoryFile::new(
        &path,
        common,
        Some(entity),
        body::render(&sections, Language::Fr),
    )
    .write()
    .unwrap();
    path.file_stem().unwrap().to_string_lossy().into_owned()
}

fn names(hits: &[ipse_core::search::Hit]) -> Vec<&str> {
    hits.iter().map(|h| h.name.as_str()).collect()
}

fn all(_: &Category) -> bool {
    true
}

#[test]
fn accents_and_case_do_not_matter() {
    let dir = tempfile::tempdir().unwrap();
    entity(
        dir.path(),
        FileType::Person,
        "people",
        "Élodie Martin",
        &[],
        &[],
        None,
    );
    let index = SearchIndex::load(dir.path()).unwrap();
    for query in ["elodie", "ÉLODIE", "Elodie MARTIN", "élo"] {
        assert_eq!(
            names(&index.search(query, 10, all)),
            ["Élodie Martin"],
            "{query}"
        );
    }
    assert!(index.search("marie", 10, all).is_empty());
}

#[test]
fn names_and_aliases_rank_before_facts_and_notes() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    entity(
        root,
        FileType::Person,
        "people",
        "Paul Durand",
        &[],
        &[],
        Some("A connu Marie au lycée."),
    );
    entity(
        root,
        FileType::Person,
        "people",
        "Léa Moreau",
        &[],
        &["Cousine de Marie."],
        None,
    );
    entity(
        root,
        FileType::Person,
        "people",
        "Maman",
        &["Marie"],
        &[],
        None,
    );
    entity(
        root,
        FileType::Person,
        "people",
        "Marie Dupont",
        &[],
        &[],
        None,
    );

    let hits = SearchIndex::load(root).unwrap().search("marie", 10, all);
    assert_eq!(
        names(&hits),
        ["Marie Dupont", "Maman", "Léa Moreau", "Paul Durand"]
    );
    assert_eq!(hits[2].excerpt, "Cousine de Marie.");
    assert_eq!(hits[3].excerpt, "A connu Marie au lycée.");
}

#[test]
fn every_word_of_the_query_must_match() {
    let dir = tempfile::tempdir().unwrap();
    entity(
        dir.path(),
        FileType::Person,
        "people",
        "Marie Dupont",
        &[],
        &["Infirmière à Lyon."],
        None,
    );
    entity(
        dir.path(),
        FileType::Person,
        "people",
        "Marie Curie",
        &[],
        &[],
        None,
    );
    let index = SearchIndex::load(dir.path()).unwrap();
    assert_eq!(
        names(&index.search("marie lyon", 10, all)),
        ["Marie Dupont"]
    );
}

#[test]
fn files_outside_the_allowed_categories_are_never_returned() {
    let dir = tempfile::tempdir().unwrap();
    entity(
        dir.path(),
        FileType::Person,
        "people",
        "Marie Dupont",
        &[],
        &[],
        None,
    );
    entity(
        dir.path(),
        FileType::Place,
        "home",
        "Chez Marie",
        &[],
        &[],
        None,
    );
    entity(
        dir.path(),
        FileType::Person,
        "private",
        "Marie Secret",
        &[],
        &[],
        None,
    );
    let index = SearchIndex::load(dir.path()).unwrap();

    let only_people = |c: &Category| c.as_str() == "people";
    let hits = index.search("marie", 10, only_people);
    assert_eq!(names(&hits), ["Marie Dupont"]);
    assert!(hits.iter().all(|h| h.category.as_str() == "people"));
    assert!(index.named("Chez Marie", only_people).is_empty());
    assert_eq!(index.named("chez marie", all).len(), 1);
}

#[test]
fn the_limit_is_respected_and_capped() {
    let dir = tempfile::tempdir().unwrap();
    for i in 0..60 {
        entity(
            dir.path(),
            FileType::Person,
            "people",
            &format!("Marie {i}"),
            &[],
            &[],
            None,
        );
    }
    let index = SearchIndex::load(dir.path()).unwrap();
    assert_eq!(index.len(), 60);
    assert_eq!(index.search("marie", 3, all).len(), 3);
    assert_eq!(index.search("marie", 1000, all).len(), MAX_LIMIT);
    assert_eq!(index.search("marie", 0, all).len(), 1);

    let id = |_: &str| String::new();
    assert!(index.search_with("marie", 59, all, id).more);
    assert!(!index.search_with("marie 1", 50, all, id).more, "11 match");
    let both = |c: &Category| c.as_str() == "people";
    let cut = index.search_with("marie", 50, both, id);
    assert_eq!((cut.hits.len(), cut.more), (MAX_LIMIT, true));
}

#[test]
fn changes_to_entity_files_are_noticed() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    let stem = entity(root, FileType::Person, "people", "Marie", &[], &[], None);
    let index = SearchIndex::load(root).unwrap();
    assert!(!index.changed(root).unwrap());

    std::fs::write(root.join("notes.md"), "not an entity").unwrap();
    std::fs::write(root.join("people/.draft.md"), "hidden file").unwrap();
    assert!(!index.changed(root).unwrap(), "other files do not count");

    let path = root.join("people").join(format!("{stem}.md"));
    let text = std::fs::read_to_string(&path).unwrap();
    std::fs::write(&path, text.replace("Marie", "Mariette")).unwrap();
    assert!(index.changed(root).unwrap(), "modified");

    let index = SearchIndex::load(root).unwrap();
    entity(root, FileType::Place, "places", "Lyon", &[], &[], None);
    assert!(index.changed(root).unwrap(), "added");

    let index = SearchIndex::load(root).unwrap();
    std::fs::rename(&path, root.join("people/marie-2.md")).unwrap();
    assert!(index.changed(root).unwrap(), "renamed");

    let index = SearchIndex::load(root).unwrap();
    std::fs::remove_file(root.join("people/marie-2.md")).unwrap();
    assert!(index.changed(root).unwrap(), "removed");
}

#[test]
fn results_say_what_and_where() {
    let dir = tempfile::tempdir().unwrap();
    let stem = entity(
        dir.path(),
        FileType::Place,
        "places",
        "Café de l'Œuvre",
        &[],
        &[],
        None,
    );
    let hits = SearchIndex::load(dir.path())
        .unwrap()
        .search("oeuvre", 10, all);
    assert_eq!(hits[0].file_type, FileType::Place);
    assert_eq!(hits[0].category.as_str(), "places");
    assert_eq!(hits[0].path, Path::new("places").join(format!("{stem}.md")));
    assert_eq!(hits[0].excerpt, "Café de l'Œuvre");
}

#[test]
fn long_notes_are_shortened_around_the_match() {
    let dir = tempfile::tempdir().unwrap();
    let notes = format!(
        "{} Rencontrée au club d'escalade. {}",
        "Avant. ".repeat(60),
        "Après. ".repeat(60)
    );
    entity(
        dir.path(),
        FileType::Person,
        "people",
        "Ana Silva",
        &[],
        &[],
        Some(&notes),
    );
    let hits = SearchIndex::load(dir.path())
        .unwrap()
        .search("escalade", 10, all);
    let excerpt = &hits[0].excerpt;
    assert!(excerpt.contains("escalade"), "{excerpt}");
    assert!(excerpt.starts_with('…') && excerpt.ends_with('…'));
    assert!(excerpt.chars().count() <= 162);
}

#[test]
fn unreadable_files_are_skipped() {
    let dir = tempfile::tempdir().unwrap();
    entity(
        dir.path(),
        FileType::Person,
        "people",
        "Ana Silva",
        &[],
        &[],
        None,
    );
    std::fs::write(dir.path().join("people/broken.md"), "no front matter").unwrap();
    std::fs::write(dir.path().join("people/.ipse-1.tmp"), "temporary").unwrap();
    let index = SearchIndex::load(dir.path()).unwrap();
    assert_eq!((index.len(), index.skipped()), (1, 1));
}

#[test]
fn an_empty_query_finds_nothing() {
    let dir = tempfile::tempdir().unwrap();
    entity(
        dir.path(),
        FileType::Person,
        "people",
        "Ana Silva",
        &[],
        &[],
        None,
    );
    let index = SearchIndex::load(dir.path()).unwrap();
    assert!(index.search("  ", 10, all).is_empty());
    assert!(index.search("!?", 10, all).is_empty());
}
