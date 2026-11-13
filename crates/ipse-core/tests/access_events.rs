//! Events and states through the query layer: rights, periods, caps
//! (`docs/spec/rights.md`, section 4).

// Test helpers may panic: a failed expectation is a failed test.
#![allow(clippy::expect_used, clippy::unwrap_used)]

use std::path::Path;

use ipse_core::rights::{Clients, Gateway};
use ipse_core::time::TimeZone;
use ipse_core::vault::{
    Category, Common, DateTime, EntityFields, EventFields, EventTime, FileType, Language,
    LinkOrText, MemoryFile, Settings, StateLogFields, Topic, Vault, naming,
};
use jiff::ToSpan;
use jiff::civil::Date;

struct Fixture {
    _dir: tempfile::TempDir,
    root: std::path::PathBuf,
    token: String,
    marie: String,
    lea: String,
    today: Date,
}

fn tz() -> jiff::tz::TimeZone {
    jiff::tz::TimeZone::get("Europe/Paris").unwrap()
}

fn person(root: &Path, category: &str, name: &str) -> String {
    let mut common = Common::new(FileType::Person, &TimeZone::UTC).unwrap();
    common.category = Category::new(category).unwrap();
    let id = common.id.to_string();
    let path = naming::entity_path(root, FileType::Person, Some(name), &common.id).unwrap();
    let entity = EntityFields {
        name: Some(name.to_owned()),
        ..EntityFields::default()
    };
    MemoryFile::new(&path, common, Some(entity), "")
        .write()
        .unwrap();
    id
}

fn event(root: &Path, category: &str, title: &str, start: &str, who: &[&str]) {
    let mut common = Common::new(FileType::Event, &TimeZone::UTC).unwrap();
    common.category = Category::new(category).unwrap();
    let start = EventTime::parse(start).unwrap();
    let mut fields = EventFields::new(title, start);
    fields.participants = who.iter().map(|w| LinkOrText::parse(w)).collect();
    fields.place = Some(LinkOrText::Text("12 rue des Lilas".into()));
    let path = naming::event_path(root, title, &start, &common.id).unwrap();
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    MemoryFile::new_event(
        &path,
        common,
        fields,
        "\n## Notes\n\nAvec [[lea-secret]] aussi.\nApporter du pain.\n",
    )
    .write()
    .unwrap();
}

fn states(root: &Path, topic: &str, category: &str, year: i16, rows: &str) {
    let mut common = Common::new(FileType::Event, &TimeZone::UTC).unwrap();
    common.file_type = FileType::StateLog;
    common.category = Category::new(category).unwrap();
    let topic = Topic::new(topic).unwrap();
    let path = naming::state_log_path(root, &topic, year);
    let body = format!(
        "\n| Subject | First seen | Last seen | Count | Source |\n| --- | --- | --- | --- | --- |\n{rows}"
    );
    MemoryFile::new_state_log(&path, common, StateLogFields { topic, year }, body)
        .write()
        .unwrap();
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
    let marie = person(&root, "people", "Marie Dupont");
    let lea = person(&root, "health", "Léa Secret");
    let today = jiff::Timestamp::now().to_zoned(tz()).date();
    let day = |n: i64| today.checked_add(n.days()).unwrap();

    event(
        &root,
        "agenda",
        "Dîner chez Marie",
        &format!("{}T20:00:00+01:00", day(1)),
        &["[[marie-dupont]]", "[[lea-secret]]", "Paul"],
    );
    event(&root, "agenda", "Vacances", &day(3).to_string(), &[]);
    event(
        &root,
        "health",
        "Rendez-vous médical",
        &format!("{}T09:00:00+01:00", day(1)),
        &[],
    );
    event(
        &root,
        "agenda",
        "Réunion passée",
        &format!("{}T10:00:00+01:00", day(-40)),
        &["[[marie-dupont]]"],
    );

    let year = today.year();
    let rows = format!(
        "| [[marie-dupont]] | {} | {} | 3 | device |\n| [[lea-secret]] | {} | {} | 5 | device |\n| Nina Simone | {} | {} | 64 | device |\n| Justice | {} | {} | 9 | device |\n",
        day(-30),
        day(-2),
        day(-10),
        day(-1),
        day(-300),
        day(0),
        day(-200),
        day(-100),
    );
    states(&root, "music", "media", year, &rows);

    let mut clients = Clients::load(&root).unwrap();
    let (_, token) = clients
        .add("Claude", DateTime::now(&TimeZone::UTC))
        .unwrap();
    Fixture {
        root,
        token: token.expose().to_owned(),
        marie,
        lea,
        today,
        _dir: dir,
    }
}

fn day(f: &Fixture, n: i64) -> EventTime {
    EventTime::Day(f.today.checked_add(n.days()).unwrap())
}

fn titles(events: &ipse_core::rights::Events) -> Vec<&str> {
    events
        .events
        .iter()
        .map(|e| e.name.as_deref().unwrap())
        .collect()
}

#[test]
fn events_of_a_period_without_what_is_hidden() {
    let f = fixture();
    let gateway = Gateway::open(&f.root).unwrap();
    let access = gateway.access(Some(&f.token)).unwrap();

    let week = access.events(&day(&f, 0), &day(&f, 7), None, 200);
    assert_eq!(
        titles(&week),
        ["Dîner chez Marie", "Vacances"],
        "ordered; health hidden"
    );
    assert!(!week.more);
    let dinner = &week.events[0];
    assert_eq!(dinner.file_type, "event");
    assert_eq!(
        dinner.participants,
        ["[[marie-dupont]]", "Paul"],
        "Léa left out"
    );
    assert_eq!(dinner.place.as_deref(), Some("12 rue des Lilas"));
    assert_eq!(dinner.notes.as_deref(), Some("Apporter du pain."));
    assert!(
        dinner.start.as_deref().unwrap().ends_with(":00+01:00")
            || dinner.start.as_deref().unwrap().ends_with(":00+02:00")
    );
    let holidays = &week.events[1];
    assert_eq!(
        holidays.start.as_deref(),
        Some(f.today.checked_add(3.days()).unwrap().to_string().as_str())
    );

    assert_eq!(
        titles(&access.events(&day(&f, 3), &day(&f, 3), None, 200)),
        ["Vacances"],
        "a whole day counts whole"
    );
    assert!(
        access
            .events(&day(&f, 4), &day(&f, 7), None, 200)
            .events
            .is_empty()
    );

    let with_marie = access.events(&day(&f, -60), &day(&f, 60), Some(&f.marie), 200);
    assert_eq!(titles(&with_marie), ["Réunion passée", "Dîner chez Marie"]);
    assert!(
        access
            .events(&day(&f, -60), &day(&f, 60), Some(&f.lea), 200)
            .events
            .is_empty(),
        "a hidden person: nothing"
    );
    assert!(
        access
            .events(&day(&f, -60), &day(&f, 60), Some("not-an-id"), 200)
            .events
            .is_empty()
    );

    let first = access.events(&day(&f, -60), &day(&f, 60), None, 1);
    assert_eq!((first.events.len(), first.more), (1, true));
}

#[test]
fn an_ai_without_agenda_sees_no_event() {
    let f = fixture();
    let clients = f.root.join(".ipse/clients.toml");
    let text = std::fs::read_to_string(&clients).unwrap();
    std::fs::write(&clients, text.replace("\"agenda\", ", "")).unwrap();
    let gateway = Gateway::open(&f.root).unwrap();
    let access = gateway.access(Some(&f.token)).unwrap();
    assert!(
        access
            .events(&day(&f, -60), &day(&f, 60), None, 200)
            .events
            .is_empty()
    );
    assert!(access.search("diner", 10).hits.is_empty());
    assert!(access.get_by_name("Vacances").is_empty());
}

#[test]
fn events_are_searchable_and_readable() {
    let f = fixture();
    let gateway = Gateway::open(&f.root).unwrap();
    let access = gateway.access(Some(&f.token)).unwrap();
    let found = access.search("diner paul", 10);
    assert_eq!(found.hits.len(), 1);
    assert_eq!(found.hits[0].name, "Dîner chez Marie");
    assert!(
        access.search("lea", 10).hits.is_empty(),
        "a hidden guest does not match"
    );
    let entry = access.get(&found.hits[0].id.to_string()).unwrap();
    assert_eq!(entry.participants, ["[[marie-dupont]]", "Paul"]);
}

#[test]
fn now_gives_current_states_only() {
    let f = fixture();
    let gateway = Gateway::open(&f.root).unwrap();
    let access = gateway.access(Some(&f.token)).unwrap();
    let now = access.now(&[]);
    let subjects: Vec<&str> = now.states.iter().map(|s| s.subject.as_str()).collect();
    assert_eq!(
        subjects,
        ["Nina Simone", "[[marie-dupont]]"],
        "most recent first; Léa hidden; Justice is past"
    );
    assert!(now.states.iter().all(|s| s.status.is_none()));
    assert_eq!(now.states[0].last_seen, f.today.to_string());
    assert!(access.now(&["watching".into()]).states.is_empty());

    let clients = f.root.join(".ipse/clients.toml");
    let text = std::fs::read_to_string(&clients).unwrap();
    std::fs::write(&clients, text.replace("\"media\", ", "")).unwrap();
    let gateway = Gateway::open(&f.root).unwrap();
    assert!(
        gateway
            .access(Some(&f.token))
            .unwrap()
            .now(&[])
            .states
            .is_empty()
    );
}

#[test]
fn states_history_gives_past_states_with_their_status() {
    let f = fixture();
    let gateway = Gateway::open(&f.root).unwrap();
    let access = gateway.access(Some(&f.token)).unwrap();
    let history = access.states_history("music", &day(&f, -365), &day(&f, 0), 200);
    let statuses: Vec<(&str, Option<&str>)> = history
        .states
        .iter()
        .map(|s| (s.subject.as_str(), s.status))
        .collect();
    assert!(
        statuses.contains(&("Justice", Some("past")))
            || statuses.contains(&("Justice", Some("recent"))),
        "{statuses:?}"
    );
    assert!(statuses.contains(&("Nina Simone", Some("current"))));
    assert!(!statuses.iter().any(|(s, _)| s.contains("lea")));
    assert!(
        access
            .states_history("Music", &day(&f, -365), &day(&f, 0), 200)
            .states
            .is_empty()
    );
}
