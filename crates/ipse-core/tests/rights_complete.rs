//! Custom categories, unknown categories with every tool, and the access log
//! (`docs/spec/rights.md`, sections 1.1, 5 and 6). All data is invented.

// Test helpers may panic: a failed expectation is a failed test.
#![allow(clippy::expect_used, clippy::unwrap_used)]

use std::path::{Path, PathBuf};

use ipse_core::rights::{Categories, Clients, Gateway};
use ipse_core::time::TimeZone;
use ipse_core::vault::{
    Category, Common, DateTime, EntityFields, EventFields, EventTime, FileType, Language,
    MemoryFile, Settings, StateLogFields, Topic, Vault, naming,
};
use jiff::ToSpan as _;

fn person(root: &Path, category: &str, name: &str) -> String {
    let mut common = Common::new(FileType::Person, &TimeZone::UTC).unwrap();
    common.category = Category::new(category).unwrap();
    let id = common.id.to_string();
    let path = naming::entity_path(root, FileType::Person, Some(name), &common.id).unwrap();
    let entity = EntityFields {
        name: Some(name.to_owned()),
        ..EntityFields::default()
    };
    MemoryFile::new(
        &path,
        common,
        Some(entity),
        "\n## Faits\n\n- Joue au tennis.\n",
    )
    .write()
    .unwrap();
    id
}

fn event(root: &Path, category: &str, title: &str, day: &str) {
    let mut common = Common::new(FileType::Event, &TimeZone::UTC).unwrap();
    common.category = Category::new(category).unwrap();
    let start = EventTime::parse(day).unwrap();
    let path = naming::event_path(root, title, &start, &common.id).unwrap();
    MemoryFile::new_event(&path, common, EventFields::new(title, start), "")
        .write()
        .unwrap();
}

fn states(root: &Path, topic: &str, category: &str, today: jiff::civil::Date) {
    let mut common = Common::new(FileType::Event, &TimeZone::UTC).unwrap();
    common.file_type = FileType::StateLog;
    common.category = Category::new(category).unwrap();
    let topic = Topic::new(topic).unwrap();
    let year = today.year();
    let path = naming::state_log_path(root, &topic, year);
    let body = format!(
        "\n| Subject | First seen | Last seen | Count | Source |\n| --- | --- | --- | --- | --- |\n| Marathon de Lyon | {today} | {today} | 4 | device |\n"
    );
    MemoryFile::new_state_log(&path, common, StateLogFields { topic, year }, body)
        .write()
        .unwrap();
}

struct Fixture {
    _dir: tempfile::TempDir,
    root: PathBuf,
    today: jiff::civil::Date,
}

/// A vault with a person, an event and states in the undeclared category `sport`, and
/// a person in each of `studies` and `therapy`, declared by `declared`.
fn fixture(declared: &str) -> Fixture {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path().to_owned();
    Vault::init(
        &root,
        "Camille Durand",
        Settings::new(Language::Fr, "Europe/Paris").unwrap(),
    )
    .unwrap();
    let settings = root.join(".ipse/vault.toml");
    let text = std::fs::read_to_string(&settings).unwrap();
    std::fs::write(&settings, format!("{text}{declared}")).unwrap();

    let today = jiff::Timestamp::now()
        .to_zoned(jiff::tz::TimeZone::get("Europe/Paris").unwrap())
        .date();
    person(&root, "sport", "Coach Bernard");
    event(
        &root,
        "sport",
        "Match du dimanche",
        &today.tomorrow().unwrap().to_string(),
    );
    states(&root, "running", "sport", today);
    person(&root, "studies", "Prof Martin");
    person(&root, "therapy", "Docteur Moreau");
    person(&root, "people", "Marie Dupont");
    Fixture {
        root,
        today,
        _dir: dir,
    }
}

/// Connects an AI with the vault's categories as `ipse client add` does.
fn connect(root: &Path) -> String {
    let vault = Vault::open(root).unwrap();
    let (_, token) = Clients::load(root)
        .unwrap()
        .add(
            "Claude",
            DateTime::now(&TimeZone::UTC),
            &Categories::of(vault.settings()),
            false,
        )
        .unwrap();
    token.expose().to_owned()
}

fn day(f: &Fixture, n: i64) -> EventTime {
    EventTime::Day(f.today.checked_add(n.days()).unwrap())
}

#[test]
fn an_undeclared_category_is_private_for_every_tool() {
    let f = fixture("");
    let token = connect(&f.root);
    let gateway = Gateway::open(&f.root).unwrap();
    let access = gateway.access(Some(&token)).unwrap();
    assert!(access.search("bernard", 10).hits.is_empty(), "search");
    assert!(access.get_by_name("Coach Bernard").is_empty(), "get");
    let week = access.events(&day(&f, 0), &day(&f, 7), None, 200);
    assert!(week.events.is_empty(), "events");
    assert!(access.now(&[]).states.is_empty(), "now");
    let history = access.states_history("running", &day(&f, -7), &day(&f, 0), 200);
    assert!(history.states.is_empty(), "states_history");
    assert!(access.categories_read().is_empty());
    assert_eq!(access.search("marie", 10).hits.len(), 1, "the rest is read");
}

#[test]
fn a_declared_category_opens_to_new_ais_and_a_sensitive_one_stays_closed() {
    let f = fixture(
        "\n[categories.studies]\nname = \"Études\"\nsensitive = false\n\n[categories.therapy]\nsensitive = true\n\n[categories.sport]\n",
    );
    let token = connect(&f.root);
    let clients = std::fs::read_to_string(f.root.join(".ipse/clients.toml")).unwrap();
    assert!(clients.contains("\"studies\""), "{clients}");
    assert!(!clients.contains("\"therapy\""), "{clients}");
    assert!(
        !clients.contains("\"sport\""),
        "sensitive when left out: {clients}"
    );

    let gateway = Gateway::open(&f.root).unwrap();
    let access = gateway.access(Some(&token)).unwrap();
    assert_eq!(access.get_by_name("Prof Martin").len(), 1);
    assert!(access.get_by_name("Docteur Moreau").is_empty());
    assert!(access.get_by_name("Coach Bernard").is_empty());
    // A declared category is not `private`: opening `private` does not open it.
    let path = f.root.join(".ipse/clients.toml");
    std::fs::write(
        &path,
        clients.replace("\"summaries\"", "\"summaries\", \"private\""),
    )
    .unwrap();
    let gateway = Gateway::open(&f.root).unwrap();
    let access = gateway.access(Some(&token)).unwrap();
    assert!(access.get_by_name("Docteur Moreau").is_empty());
}

#[test]
fn a_category_declared_later_is_not_given_to_ais_already_connected() {
    let f = fixture("");
    let token = connect(&f.root);
    let settings = f.root.join(".ipse/vault.toml");
    let text = std::fs::read_to_string(&settings).unwrap();
    std::fs::write(
        &settings,
        format!("{text}\n[categories.studies]\nsensitive = false\n"),
    )
    .unwrap();
    let gateway = Gateway::open(&f.root).unwrap();
    let access = gateway.access(Some(&token)).unwrap();
    assert!(access.get_by_name("Prof Martin").is_empty());
}

#[test]
fn the_access_log_holds_categories_and_counts_only() {
    let f = fixture("\n[categories.sport]\nsensitive = false\n");
    let token = connect(&f.root);
    let gateway = Gateway::open(&f.root).unwrap();

    let access = gateway.access(Some(&token)).unwrap();
    let found = access.search("bernard", 10);
    access.log("search", found.hits.len()).unwrap();
    let access = gateway.access(Some(&token)).unwrap();
    let week = access.events(&day(&f, 0), &day(&f, 7), None, 200);
    access.log("events", week.events.len()).unwrap();
    let access = gateway.access(Some(&token)).unwrap();
    let now = access.now(&[]);
    access.log("now", now.states.len()).unwrap();
    let access = gateway.access(Some(&token)).unwrap();
    assert!(access.get_by_name("Nobody At All").is_empty());
    access.log("get", 0).unwrap();

    let dir = f.root.join(ipse_core::rights::ACCESS_DIR);
    let files: Vec<_> = std::fs::read_dir(&dir).unwrap().collect();
    assert_eq!(files.len(), 1, "one file this month");
    let month = jiff::Timestamp::now()
        .to_zoned(jiff::tz::TimeZone::get("Europe/Paris").unwrap())
        .strftime("%Y-%m")
        .to_string();
    let text = std::fs::read_to_string(dir.join(format!("{month}.jsonl"))).unwrap();
    let lines: Vec<serde_json::Value> = text
        .lines()
        .map(|l| serde_json::from_str(l).unwrap())
        .collect();
    let summary: Vec<(String, Vec<String>, u64)> = lines
        .iter()
        .map(|l| {
            let categories = l["categories"]
                .as_array()
                .unwrap()
                .iter()
                .map(|c| c.as_str().unwrap().to_owned())
                .collect();
            (
                l["tool"].as_str().unwrap().to_owned(),
                categories,
                l["results"].as_u64().unwrap(),
            )
        })
        .collect();
    assert_eq!(
        summary,
        [
            ("search".to_owned(), vec!["sport".to_owned()], 1),
            ("events".to_owned(), vec!["sport".to_owned()], 1),
            ("now".to_owned(), vec!["sport".to_owned()], 1),
            ("get".to_owned(), vec![], 0),
        ]
    );
    for line in &lines {
        assert_eq!(line["client"], "claude");
        assert!(line["at"].as_str().unwrap().starts_with(&month));
        let keys: Vec<&String> = line.as_object().unwrap().keys().collect();
        assert_eq!(keys, ["at", "categories", "client", "results", "tool"]);
    }
    for leak in [
        "bernard", "Bernard", "Match", "Marathon", "Nobody", "tennis",
    ] {
        assert!(!text.contains(leak), "{leak} in the log");
    }
}
