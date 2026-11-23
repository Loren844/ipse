//! One kind of event at a glance (issue #134): the next and last ones, who and where
//! most, and the birthdays to come, within each reader's rights. All data is invented.

// Test helpers may panic: a failed expectation is a failed test.
#![allow(clippy::expect_used, clippy::unwrap_used)]

use std::path::Path;

use ipse_core::rights::Gateway;
use ipse_core::vault::{EventKind, Language, Settings, Vault};
use jiff::civil::date;

fn write(root: &Path, rel: &str, i: usize, front: &str) {
    let path = root.join(rel);
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(
        &path,
        format!(
            "---\nid: 01jcx8m2p4q6r8t0v2w4y6z8{i:02}\n{front}\ncreated: 2026-01-10T09:00:00+01:00\nupdated: 2026-01-10T09:00:00+01:00\n---\n"
        ),
    )
    .unwrap();
}

fn event(root: &Path, i: usize, title: &str, start: &str, extra: &str) {
    write(
        root,
        &format!("events/{}/e-{i}.md", &start[..4]),
        i,
        &format!("type: event\ncategory: agenda\ntitle: {title}\nstart: {start}\n{extra}"),
    );
}

fn memory() -> tempfile::TempDir {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
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
        "type: person\ncategory: people\nname: Marie Durand\nbirthday: 1990-12-02",
    );
    write(
        root,
        "people/paul-roux.md",
        2,
        "type: person\ncategory: people\nname: Paul Roux\nbirthday: --03-12",
    );
    write(
        root,
        "people/lea-blanc.md",
        4,
        "type: person\ncategory: people\nname: Léa Blanc\nbirthday: 1988-06-30",
    );
    write(
        root,
        "places/chez-marie.md",
        3,
        "type: place\ncategory: places\nname: Chez Marie",
    );
    let marie = "participants:\n  - \"[[marie-durand]]\"\n";
    let both = "participants:\n  - \"[[marie-durand]]\"\n  - \"[[paul-roux]]\"\n";
    event(
        root,
        10,
        "Déjeuner",
        "2026-10-05T12:30:00+02:00",
        &format!("kind: meal\n{marie}place: \"[[chez-marie]]\"\n"),
    );
    event(
        root,
        11,
        "Dîner",
        "2026-11-20T20:00:00+01:00",
        &format!("kind: meal\n{both}place: \"[[chez-marie]]\"\n"),
    );
    event(
        root,
        12,
        "Brunch",
        "2026-11-29T11:00:00+01:00",
        &format!("kind: meal\n{marie}"),
    );
    event(
        root,
        13,
        "Week-end à Lyon",
        "2026-12-05",
        &format!("kind: trip\nend: 2026-12-07\n{both}"),
    );
    // Lunch with the doctor's: a sensitive category.
    std::fs::write(
        root.join("events/2026/e-14.md"),
        "---\nid: 01jcx8m2p4q6r8t0v2w4y6z814\ntype: event\ncategory: health\ntitle: Repas du service\nstart: 2026-11-10T12:00:00+01:00\nkind: meal\ncreated: 2026-01-10T09:00:00+01:00\nupdated: 2026-01-10T09:00:00+01:00\n---\n",
    )
    .unwrap();
    dir
}

#[test]
fn a_kind_gives_its_next_and_last_events_who_and_where() {
    let dir = memory();
    let mut gateway = Gateway::open(dir.path()).unwrap();
    let owner = gateway.fresh_owner().unwrap();
    let today = date(2026, 11, 23);

    let meals = owner.kind(EventKind::Meal, today, 5);
    assert_eq!(meals.events, 4, "the owner reads every category");
    let titles = |list: &[ipse_core::rights::Details]| -> Vec<String> {
        list.iter()
            .map(|d| d.entry.name.clone().unwrap_or_default())
            .collect()
    };
    assert_eq!(titles(&meals.next), ["Brunch"]);
    assert_eq!(
        titles(&meals.last),
        ["Dîner", "Repas du service", "Déjeuner"],
        "the most recent first"
    );
    let people: Vec<(&str, u32)> = meals
        .people
        .iter()
        .map(|(p, n)| (p.name.as_str(), *n))
        .collect();
    assert_eq!(people, [("Marie Durand", 3), ("Paul Roux", 1)]);
    let places: Vec<(&str, u32)> = meals
        .places
        .iter()
        .map(|(p, n)| (p.name.as_str(), *n))
        .collect();
    assert_eq!(places, [("Chez Marie", 2)]);

    let trips = owner.kind(EventKind::Trip, today, 5);
    assert_eq!(trips.events, 1);
    assert_eq!(trips.next[0].entry.end.as_deref(), Some("2026-12-07"));
    assert_eq!(owner.kind(EventKind::Health, today, 5).events, 0);

    // At most `shown` of each.
    let one = owner.kind(EventKind::Meal, today, 1);
    assert_eq!((one.last.len(), one.people.len()), (1, 1));
}

#[test]
fn birthdays_to_come_the_soonest_first() {
    let dir = memory();
    let mut gateway = Gateway::open(dir.path()).unwrap();
    let owner = gateway.fresh_owner().unwrap();
    let today = date(2026, 11, 23);

    let soon: Vec<(String, String, Option<i16>)> = owner
        .birthdays(today, 365)
        .into_iter()
        .map(|b| (b.person.name, b.day.to_string(), b.age))
        .collect();
    assert_eq!(
        soon,
        [
            ("Marie Durand".to_owned(), "2026-12-02".to_owned(), Some(36)),
            ("Paul Roux".to_owned(), "2027-03-12".to_owned(), None),
            ("Léa Blanc".to_owned(), "2027-06-30".to_owned(), Some(39)),
        ]
    );
    assert_eq!(owner.birthdays(today, 30).len(), 1, "within 30 days");
}
