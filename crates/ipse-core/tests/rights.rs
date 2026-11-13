//! What a connected AI can and cannot read (`docs/spec/rights.md`, section 6).

// Test helpers may panic: a failed expectation is a failed test.
#![allow(clippy::expect_used, clippy::unwrap_used)]

use std::path::{Path, PathBuf};

use ipse_core::Error;
use ipse_core::rights::{Clients, Gateway};
use ipse_core::time::TimeZone;
use ipse_core::vault::{
    Body, Category, Common, DateTime, EntityFields, Fact, FileType, Language, MemoryFile, Relation,
    Settings, Vault, WikiLink, body, naming,
};

struct Person<'a> {
    file_type: FileType,
    category: &'a str,
    name: &'a str,
    facts: &'a [&'a str],
    relations: &'a [(&'a str, &'a str)],
}

fn write(root: &Path, p: &Person<'_>) -> (String, PathBuf) {
    let mut common = Common::new(p.file_type, &TimeZone::UTC).unwrap();
    common.category = Category::new(p.category).unwrap();
    let id = common.id.to_string();
    let path = naming::entity_path(root, p.file_type, Some(p.name), &common.id).unwrap();
    let sections = Body {
        facts: p
            .facts
            .iter()
            .map(|f| Fact {
                text: (*f).to_owned(),
                id: None,
            })
            .collect(),
        relations: p
            .relations
            .iter()
            .map(|(word, target)| Relation {
                word: (*word).to_owned(),
                link: WikiLink::parse(&format!("[[{target}]]")).unwrap(),
            })
            .collect(),
        notes: None,
    };
    let entity = EntityFields {
        name: Some(p.name.to_owned()),
        ..EntityFields::default()
    };
    MemoryFile::new(
        &path,
        common,
        Some(entity),
        body::render(&sections, Language::Fr),
    )
    .write()
    .unwrap();
    (id, path)
}

struct Fixture {
    _dir: tempfile::TempDir,
    root: PathBuf,
    token: String,
    lea: String,
    marie: PathBuf,
}

fn fixture() -> Fixture {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path().to_owned();
    Vault::init(
        &root,
        "Camille Durand",
        Settings::new(Language::Fr, "Europe/Paris").unwrap(),
    )
    .unwrap();
    let person = |category, name, facts, relations| Person {
        file_type: FileType::Person,
        category,
        name,
        facts,
        relations,
    };
    let (_, marie) = write(
        &root,
        &person(
            "people",
            "Marie Dupont",
            &[
                "Cousine de [[lea-secret]].",
                "Voisine de [[paul-durand|Paul]].",
            ],
            &[
                ("sœur", "lea-secret"),
                ("ami", "paul-durand"),
                ("prof", "nulle-part"),
            ],
        ),
    );
    write(&root, &person("people", "Paul Durand", &[], &[]));
    let (lea, _) = write(
        &root,
        &person("health", "Léa Secret", &["Suivie à l'hôpital."], &[]),
    );
    write(&root, &person("studies", "Prof Martin", &[], &[]));

    let mut clients = Clients::load(&root).unwrap();
    let (_, token) = clients
        .add("Claude", DateTime::now(&TimeZone::UTC))
        .unwrap();
    Fixture {
        root,
        token: token.expose().to_owned(),
        lea,
        marie,
        _dir: dir,
    }
}

fn names(hits: &[ipse_core::search::Hit]) -> Vec<&str> {
    hits.iter().map(|h| h.name.as_str()).collect()
}

#[test]
fn a_missing_or_wrong_code_is_refused() {
    let f = fixture();
    let gateway = Gateway::open(&f.root).unwrap();
    for token in [None, Some(""), Some("ipse_0000"), Some("Claude")] {
        assert!(
            matches!(gateway.access(token), Err(Error::Refused(_))),
            "{token:?}"
        );
    }
    assert!(gateway.access(Some(&f.token)).is_ok());
}

#[test]
fn a_denied_category_returns_nothing() {
    let f = fixture();
    let gateway = Gateway::open(&f.root).unwrap();
    let access = gateway.access(Some(&f.token)).unwrap();

    for query in ["lea", "secret", "hopital", "léa secret"] {
        assert!(access.search(query, 10).hits.is_empty(), "{query}");
    }
    assert!(access.get(&f.lea).is_none());
    assert!(access.get_by_name("Léa Secret").is_empty());
    assert_eq!(names(&access.search("marie", 10).hits), ["Marie Dupont"]);
}

#[test]
fn lines_about_hidden_files_are_left_out() {
    let f = fixture();
    let text = std::fs::read_to_string(&f.marie).unwrap();
    let notes = "\n## Notes\n\nRencontrée au club.\nPartie en vacances avec [[lea-secret]].\nAime le jazz.\n";
    std::fs::write(&f.marie, text + notes).unwrap();

    let gateway = Gateway::open(&f.root).unwrap();
    let access = gateway.access(Some(&f.token)).unwrap();
    let marie = access.get_by_name("marie dupont").remove(0);

    assert_eq!(marie.facts, ["Voisine de [[paul-durand|Paul]]."]);
    let relations: Vec<_> = marie
        .relations
        .iter()
        .map(|r| (r.word.as_str(), r.target.as_str()))
        .collect();
    assert_eq!(
        relations,
        [("ami", "[[paul-durand]]")],
        "hidden and unknown targets left out"
    );
    assert_eq!(
        marie.notes.as_deref(),
        Some("Rencontrée au club.\nAime le jazz.")
    );

    let json = serde_json::to_string(&marie).unwrap();
    for leak in [
        "lea", "Léa", "Cousine", "sœur", "vacances", "masqu", "hidden",
    ] {
        assert!(!json.contains(leak), "{leak} in {json}");
    }

    assert!(access.search("cousine", 10).hits.is_empty());
    assert!(access.search("vacances", 10).hits.is_empty());
    assert_eq!(names(&access.search("voisine", 10).hits), ["Marie Dupont"]);
    assert_eq!(
        access.search("jazz", 10).hits[0].excerpt,
        "Rencontrée au club.\nAime le jazz."
    );
}

#[test]
fn unknown_categories_are_treated_as_private() {
    let f = fixture();
    let gateway = Gateway::open(&f.root).unwrap();
    let access = gateway.access(Some(&f.token)).unwrap();
    assert!(access.search("martin", 10).hits.is_empty());

    // Opening `private` to this AI opens unknown categories too.
    let clients = f.root.join(".ipse/clients.toml");
    let text = std::fs::read_to_string(&clients).unwrap();
    std::fs::write(
        &clients,
        text.replace("\"summaries\"]", "\"summaries\", \"private\"]"),
    )
    .unwrap();
    let gateway = Gateway::open(&f.root).unwrap();
    let access = gateway.access(Some(&f.token)).unwrap();
    assert_eq!(names(&access.search("martin", 10).hits), ["Prof Martin"]);
}

#[test]
fn a_removed_ai_is_refused() {
    let f = fixture();
    let mut gateway = Gateway::open(&f.root).unwrap();
    let mut clients = Clients::load(&f.root).unwrap();
    let id = clients.list()[0].id.clone();
    assert!(clients.remove(&id).unwrap());
    gateway.refresh().unwrap();
    assert!(gateway.access(Some(&f.token)).is_err());
}

#[test]
fn about_me_needs_identity() {
    let f = fixture();
    let gateway = Gateway::open(&f.root).unwrap();
    let me = gateway.access(Some(&f.token)).unwrap().about_me().unwrap();
    assert_eq!(me.name.as_deref(), Some("Camille Durand"));

    let clients = f.root.join(".ipse/clients.toml");
    let text = std::fs::read_to_string(&clients).unwrap();
    std::fs::write(&clients, text.replace("\"identity\", ", "")).unwrap();
    let gateway = Gateway::open(&f.root).unwrap();
    assert!(gateway.access(Some(&f.token)).unwrap().about_me().is_none());
}

#[test]
fn the_category_is_checked_on_the_file_as_it_is_now() {
    let f = fixture();
    let gateway = Gateway::open(&f.root).unwrap();
    let text = std::fs::read_to_string(&f.marie).unwrap();
    std::fs::write(
        &f.marie,
        text.replace("category: people", "category: health"),
    )
    .unwrap();
    let access = gateway.access(Some(&f.token)).unwrap();
    assert!(
        access.get_by_name("Marie Dupont").is_empty(),
        "index not refreshed, file re-checked"
    );
}

#[test]
fn fresh_access_sees_new_rights_and_new_files() {
    let f = fixture();
    let mut gateway = Gateway::open(&f.root).unwrap();
    assert!(
        gateway
            .fresh_access(Some(&f.token))
            .unwrap()
            .search("zoe", 10)
            .hits
            .is_empty()
    );

    write(
        &f.root,
        &Person {
            file_type: FileType::Person,
            category: "people",
            name: "Zoé Bernard",
            facts: &[],
            relations: &[],
        },
    );
    let access = gateway.fresh_access(Some(&f.token)).unwrap();
    assert_eq!(names(&access.search("zoe", 10).hits), ["Zoé Bernard"]);

    let clients = f.root.join(".ipse/clients.toml");
    let text = std::fs::read_to_string(&clients).unwrap();
    std::fs::write(&clients, text.replace("\"people\", ", "")).unwrap();
    let access = gateway.fresh_access(Some(&f.token)).unwrap();
    assert!(
        access.search("zoe", 10).hits.is_empty(),
        "rights apply at once"
    );

    let mut clients = Clients::load(&f.root).unwrap();
    let id = clients.list()[0].id.clone();
    clients.remove(&id).unwrap();
    assert!(matches!(
        gateway.fresh_access(Some(&f.token)),
        Err(Error::Refused(_))
    ));
}
