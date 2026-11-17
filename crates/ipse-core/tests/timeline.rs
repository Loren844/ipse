//! The timeline (issue #133): a life by years and months, with the people seen most
//! and what marked each month, within each reader's rights. All data is invented.

// Test helpers may panic: a failed expectation is a failed test.
#![allow(clippy::expect_used, clippy::unwrap_used)]

use std::path::Path;

use ipse_core::rights::{Categories, Clients, Gateway};
use ipse_core::vault::{DateTime, EventKind, Language, Settings, Vault};

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

#[test]
fn years_and_months_with_who_and_what_marked_them() {
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
        "type: person\ncategory: people\nname: Marie Durand",
    );
    write(
        root,
        "people/paul-roux.md",
        2,
        "type: person\ncategory: people\nname: Paul Roux",
    );
    write(
        root,
        "places/chez-marie.md",
        3,
        "type: place\ncategory: places\nname: Chez Marie",
    );
    let both = "participants:\n  - \"[[marie-durand]]\"\n  - \"[[paul-roux]]\"\n";
    let marie = "participants:\n  - \"[[marie-durand]]\"\n";
    event(root, 10, "Dîner", "2025-12-05T20:00:00+01:00", marie);
    event(
        root,
        11,
        "Week-end à Lyon",
        "2026-03-07",
        &format!("kind: trip\n{both}"),
    );
    event(
        root,
        12,
        "Café",
        "2026-03-02T09:00:00+01:00",
        &format!("{marie}place: \"[[chez-marie]]\"\n"),
    );
    event(
        root,
        13,
        "Anniversaire de Paul",
        "2026-03-20T19:00:00+01:00",
        "kind: celebration\nparticipants:\n  - \"[[paul-roux]]\"\n",
    );
    // A health appointment: a sensitive category, closed to AIs.
    std::fs::write(
        root.join("events/2026/e-14.md"),
        "---\nid: 01jcx8m2p4q6r8t0v2w4y6z814\ntype: event\ncategory: health\ntitle: Médecin\nstart: 2026-04-02T10:00:00+02:00\ncreated: 2026-01-10T09:00:00+01:00\nupdated: 2026-01-10T09:00:00+01:00\n---\n",
    )
    .unwrap();

    let mut gateway = Gateway::open(root).unwrap();
    let owner = gateway.fresh_owner().unwrap();
    let timeline = owner.timeline();
    let years: Vec<(i16, u32)> = timeline.years.iter().map(|y| (y.year, y.events)).collect();
    assert_eq!(years, [(2026, 4), (2025, 1)]);
    let year = &timeline.years[0];
    // Marie and Paul are in two events each; the place is not a person.
    let people: Vec<(&str, u32)> = year
        .people
        .iter()
        .map(|s| (s.stem.as_str(), s.events))
        .collect();
    assert_eq!(people, [("marie-durand", 2), ("paul-roux", 2)]);
    let months: Vec<(i8, u32)> = year.months.iter().map(|m| (m.month, m.events)).collect();
    assert_eq!(months, [(3, 3), (4, 1)]);
    let march = &year.months[0];
    let marks: Vec<(&str, EventKind)> = march.marks.iter().map(|m| (m.title, m.kind)).collect();
    assert_eq!(
        marks,
        [
            ("Week-end à Lyon", EventKind::Trip),
            ("Anniversaire de Paul", EventKind::Celebration)
        ]
    );

    // An AI that cannot read health sees one event less in 2026.
    let vault = Vault::open(root).unwrap();
    let categories = Categories::of(vault.settings());
    let mut clients = Clients::load(root).unwrap();
    let (_, token) = clients
        .add(
            "Claude",
            DateTime::now(vault.settings().time_zone()),
            &categories,
            false,
        )
        .unwrap();
    let gateway = Gateway::open(root).unwrap();
    let access = gateway.access(Some(token.expose())).unwrap();
    let theirs = access.timeline();
    assert_eq!(theirs.years[0].events, 3);
    assert_eq!(theirs.years[0].months.len(), 1);
}
