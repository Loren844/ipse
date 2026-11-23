//! Repeating events recognised by the index (#175). All data is invented.

// Test helpers may panic: a failed expectation is a failed test.
#![allow(clippy::expect_used, clippy::unwrap_used)]

use std::collections::HashMap;
use std::path::Path;

use ipse_core::search::{INDEX_FILE, SearchIndex, Series};
use ipse_core::time::TimeZone;
use ipse_core::vault::{Category, Common, EventFields, EventTime, FileType, MemoryFile, naming};

fn vault() -> tempfile::TempDir {
    let dir = tempfile::tempdir().unwrap();
    std::fs::create_dir_all(dir.path().join(".ipse")).unwrap();
    std::fs::write(
        dir.path().join(".ipse/vault.toml"),
        "format = 1\nlanguage = \"fr\"\ntime_zone = \"Europe/Paris\"\n",
    )
    .unwrap();
    dir
}

/// An event file; its id.
fn event(root: &Path, category: &str, title: &str, start: &str, source: Option<&str>) -> String {
    let mut common = Common::new(FileType::Event, &TimeZone::UTC).unwrap();
    common.category = Category::new(category).unwrap();
    common.sources = source.map(ToOwned::to_owned).into_iter().collect();
    let id = common.id.to_string();
    let start = EventTime::parse(start).unwrap();
    let path = naming::event_path(root, title, &start, &common.id).unwrap();
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    MemoryFile::new_event(&path, common, EventFields::new(title, start), "")
        .write()
        .unwrap();
    id
}

fn series(root: &Path, allowed: &[&str]) -> HashMap<String, Series> {
    SearchIndex::build(root)
        .unwrap()
        .series(|c| allowed.contains(&c.as_str()))
        .into_iter()
        .map(|(id, s)| (id.to_string(), s))
        .collect()
}

#[test]
fn the_same_title_weekday_and_time_three_weeks_in_a_row() {
    let dir = vault();
    let root = dir.path();
    // Four Tuesdays, a pause, three more: one series.
    let yoga: Vec<String> = [
        "2026-09-01",
        "2026-09-08",
        "2026-09-15",
        "2026-11-03",
        "2026-11-10",
        "2026-11-17",
        "2026-11-24",
    ]
    .iter()
    .map(|d| event(root, "social", "Yoga", &format!("{d}T19:00:00+01:00"), None))
    .collect();
    // Twice only, then another time: no series.
    let lunch: Vec<String> = [
        "2026-11-05T12:30:00+01:00",
        "2026-11-12T12:30:00+01:00",
        "2026-11-19T13:00:00+01:00",
    ]
    .iter()
    .map(|d| event(root, "social", "Déjeuner", d, None))
    .collect();
    // All-day events never make a series by their title.
    for d in ["2026-11-02", "2026-11-09", "2026-11-16"] {
        event(root, "social", "Télétravail", d, None);
    }

    let found = series(root, &["social"]);
    assert_eq!(found.len(), 7, "{found:#?}");
    let first = &found[&yoga[0]];
    assert_eq!(first.weekday, Some(2), "Tuesday");
    assert_eq!(first.time.as_deref(), Some("19:00"));
    assert_eq!(first.occurrences, 7);
    assert!(yoga.iter().all(|id| found[id].key == first.key));
    assert!(lunch.iter().all(|id| !found.contains_key(id)));
    assert!(!first.key.contains("yoga"), "the key says nothing");
}

#[test]
fn the_occurrences_of_one_calendar_event() {
    let dir = vault();
    let root = dir.path();
    // A rule every two weeks, one occurrence moved: still one series, by its UID.
    let ids: Vec<String> = [
        "2026-11-02T09:30:00+01:00",
        "2026-11-16T09:30:00+01:00",
        "2026-12-01T10:00:00+01:00",
    ]
    .iter()
    .map(|d| {
        let source = format!("calendar:7f3a-equipe@example.org#{d}");
        event(root, "work", "Point d’équipe", d, Some(&source))
    })
    .collect();
    // Another calendar event, once.
    let once = event(
        root,
        "work",
        "Point d’équipe",
        "2026-11-09T09:30:00+01:00",
        Some("calendar-2:other@example.org"),
    );

    let found = series(root, &["work"]);
    let first = &found[&ids[0]];
    assert_eq!(first.occurrences, 3);
    assert!(ids.iter().all(|id| found[id].key == first.key));
    assert_eq!(first.weekday, None, "a Tuesday among Mondays");
    assert_eq!(first.time, None);
    assert!(!found.contains_key(&once));
    assert!(
        !first.key.contains("example"),
        "no identifier from elsewhere"
    );

    // Saved and read again, the index keeps it.
    SearchIndex::build(root).unwrap().save(root).unwrap();
    assert!(root.join(INDEX_FILE).is_file());
    let again = SearchIndex::load(root).unwrap().series(|_| true);
    assert_eq!(again.len(), 3);

    // Occurrences that may not be read are not counted.
    assert!(series(root, &["social"]).is_empty());
}
