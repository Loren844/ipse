//! Calendar events in the journal become event files, linked to people; the person
//! always wins.

// Test helpers may panic: a failed expectation is a failed test.
#![allow(clippy::expect_used, clippy::unwrap_used)]

use std::path::{Path, PathBuf};

use ipse_core::journal::Journal;
use ipse_core::materialize::{
    EventsSummary, PeopleSummary, calendar_to_events, contacts_to_people,
};
use ipse_core::vault::{
    EventStatus, History, Language, LinkOrText, MemoryFile, Settings, Source, Vault,
};
use ipse_protocol::{Op, Record};
use jiff::Timestamp;
use serde_json::{Value, json};

const NOW: &str = "2026-11-13T12:00:00Z";

fn vault() -> (tempfile::TempDir, Vault) {
    let dir = tempfile::tempdir().unwrap();
    let vault = Vault::init(
        dir.path(),
        "Camille Durand",
        Settings::new(Language::Fr, "Europe/Paris").unwrap(),
    )
    .unwrap();
    (dir, vault)
}

fn record(kind: &str, id: &str, data: Option<Value>) -> Record {
    Record {
        kind: kind.into(),
        op: if data.is_some() {
            Op::Upsert
        } else {
            Op::Delete
        },
        external_id: Some(id.into()),
        observed_at: "2026-11-12T18:30:00Z".parse().unwrap(),
        data,
    }
}

fn event(id: &str, data: Value) -> Record {
    record("calendar.event", id, Some(data))
}

fn contacts(vault: &Vault, records: &[Record]) -> PeopleSummary {
    let mut journal = Journal::new(vault.root());
    journal.append("contacts", records, None).unwrap();
    let history = vault.history().unwrap();
    let mut change = history.change(Source::Connector("contacts".into()));
    let summary = contacts_to_people(vault, &journal, "contacts", &mut change).unwrap();
    change.commit("Import contacts").unwrap();
    summary
}

/// Imports `records` from `instance`; returns the summary and whether it committed.
fn calendar(vault: &Vault, instance: &str, records: &[Record]) -> (EventsSummary, bool) {
    let mut journal = Journal::new(vault.root());
    journal.append(instance, records, None).unwrap();
    let history = vault.history().unwrap();
    let mut change = history.change(Source::Connector(instance.into()));
    let now: Timestamp = NOW.parse().unwrap();
    let summary = calendar_to_events(vault, &journal, instance, &mut change, now).unwrap();
    let committed = change.commit("Import calendar").unwrap().is_some();
    (summary, committed)
}

fn dinner() -> Value {
    json!({
        "uid": "diner@example.org",
        "title": "Dîner chez Julie",
        "start": "2026-11-14T20:00:00+01:00",
        "end": "2026-11-14T23:00:00+01:00",
        "location": "12 rue des Lilas, Lyon",
        "description": "Apporter le dessert.",
        "organizer": {"name": "Julie Dupont", "email": "julie@example.org"},
        "attendees": [
            {"name": "Élodie Martin", "email": "Elodie@Example.org", "response": "accepted"},
            {"email": "camille@example.org"},
            {"name": "Julie Dupont", "email": "julie@example.org"},
        ],
    })
}

const DINNER: &str = "events/2026/2026-11-14-diner-chez-julie.md";

fn read_event(root: &Path, rel: &str) -> ipse_core::vault::EventFields {
    MemoryFile::read(root.join(rel))
        .unwrap()
        .event()
        .unwrap()
        .clone()
}

fn person(name: &str, email: &str) -> Value {
    json!({"full_name": name, "emails": [email]})
}

#[test]
fn attendees_link_to_the_people_already_imported() {
    let (dir, vault) = vault();
    let owner = vault.owner_file().to_owned();
    let text = std::fs::read_to_string(&owner).unwrap();
    std::fs::write(
        &owner,
        text.replace(
            "owner: true\n",
            "owner: true\nemails:\n  - camille@example.org\n",
        ),
    )
    .unwrap();
    contacts(
        &vault,
        &[record(
            "contact.card",
            "c-1",
            Some(person("Élodie Martin", "elodie@example.org")),
        )],
    );

    let (summary, committed) =
        calendar(&vault, "calendar", &[event("diner@example.org", dinner())]);
    assert_eq!((summary.added, summary.skipped), (1, 0));
    assert!(committed);

    let file = MemoryFile::read(dir.path().join(DINNER)).unwrap();
    assert_eq!(file.common().sources, ["calendar:diner@example.org"]);
    let event = file.event().unwrap();
    assert_eq!(event.title, "Dîner chez Julie");
    assert_eq!(event.start.to_string(), "2026-11-14T20:00:00+01:00");
    assert_eq!(event.end.unwrap().to_string(), "2026-11-14T23:00:00+01:00");
    assert_eq!(
        event.place,
        Some(LinkOrText::Text("12 rue des Lilas, Lyon".into()))
    );
    assert_eq!(
        event.participants,
        [
            LinkOrText::Text("Julie Dupont".into()),
            LinkOrText::parse("[[elodie-martin]]"),
        ],
        "the organizer once, Élodie linked by email whatever its case, the owner left out"
    );
    assert!(file.body().contains("Apporter le dessert."));

    let history = History::open(dir.path()).unwrap();
    let last = &history.entries(1).unwrap()[0];
    assert_eq!(last.source, Some(Source::Connector("calendar".into())));
    assert_eq!(last.files, [DINNER]);

    let elodie = std::fs::read_to_string(dir.path().join("people/elodie-martin.md")).unwrap();
    assert!(
        !elodie.contains("diner"),
        "linking never changes a person file"
    );
}

#[test]
fn an_email_shared_by_two_people_links_to_neither() {
    let (dir, vault) = vault();
    contacts(
        &vault,
        &[
            record(
                "contact.card",
                "c-1",
                Some(person("Élodie Martin", "famille@example.org")),
            ),
            record(
                "contact.card",
                "c-2",
                Some(person("Paul Martin", "famille@example.org")),
            ),
        ],
    );
    let mut data = dinner();
    data["attendees"] = json!([{"name": "Les Martin", "email": "famille@example.org"}]);
    calendar(&vault, "calendar", &[event("diner@example.org", data)]);
    let event = read_event(dir.path(), DINNER);
    assert!(
        event
            .participants
            .contains(&LinkOrText::Text("Les Martin".into()))
    );
    assert!(event.participants.iter().all(|p| p.link().is_none()));
}

#[test]
fn reimporting_the_same_calendar_changes_nothing() {
    let (_dir, vault) = vault();
    calendar(&vault, "calendar", &[event("diner@example.org", dinner())]);
    let (summary, committed) =
        calendar(&vault, "calendar", &[event("diner@example.org", dinner())]);
    assert_eq!(
        (summary.added, summary.updated, summary.unchanged),
        (0, 0, 1)
    );
    assert!(!committed, "no change, no commit");
}

#[test]
fn the_person_s_edits_win() {
    let (dir, vault) = vault();
    calendar(&vault, "calendar", &[event("diner@example.org", dinner())]);
    let path = dir.path().join(DINNER);

    // A note added by the person never stops an update.
    let text = std::fs::read_to_string(&path).unwrap();
    std::fs::write(&path, format!("{text}\nJulie fête ses 40 ans.\n")).unwrap();
    let mut later = dinner();
    later["start"] = json!("2026-11-14T20:30:00+01:00");
    let (summary, _) = calendar(
        &vault,
        "calendar",
        &[event("diner@example.org", later.clone())],
    );
    assert_eq!(summary.updated, 1);
    let text = std::fs::read_to_string(&path).unwrap();
    assert!(text.contains("start: 2026-11-14T20:30:00+01:00"));
    assert!(text.contains("Julie fête ses 40 ans."));

    // A title the person changed is theirs; the calendar's other keys still update.
    std::fs::write(
        &path,
        text.replace("title: Dîner chez Julie", "title: Les 40 ans de Julie"),
    )
    .unwrap();
    let mut even_later = later;
    even_later["start"] = json!("2026-11-14T21:00:00+01:00");
    let (summary, committed) = calendar(
        &vault,
        "calendar",
        &[event("diner@example.org", even_later.clone())],
    );
    assert_eq!((summary.updated, summary.conflicts), (1, 0));
    assert!(committed);
    let now = read_event(dir.path(), DINNER);
    assert_eq!(now.title, "Les 40 ans de Julie");
    assert_eq!(now.start.to_string(), "2026-11-14T21:00:00+01:00");

    // The calendar renames it too: the person's title stays, and it is reported.
    even_later["title"] = json!("Anniversaire de Julie");
    let (summary, _) = calendar(
        &vault,
        "calendar",
        &[event("diner@example.org", even_later)],
    );
    assert_eq!(summary.conflicts, 1);
    assert_eq!(read_event(dir.path(), DINNER).title, "Les 40 ans de Julie");
    let history = History::open(dir.path()).unwrap();
    assert_eq!(history.entries(2).unwrap()[1].source, Some(Source::Person));
}

#[test]
fn a_guest_linked_later_is_not_an_edit() {
    let (dir, vault) = vault();
    calendar(&vault, "calendar", &[event("diner@example.org", dinner())]);
    assert!(
        read_event(dir.path(), DINNER)
            .participants
            .contains(&LinkOrText::Text("Julie Dupont".into()))
    );
    contacts(
        &vault,
        &[record(
            "contact.card",
            "c-9",
            Some(person("Julie Dupont", "julie@example.org")),
        )],
    );
    let mut later = dinner();
    later["start"] = json!("2026-11-14T20:30:00+01:00");
    let (summary, _) = calendar(&vault, "calendar", &[event("diner@example.org", later)]);
    assert_eq!((summary.updated, summary.conflicts), (1, 0));
    assert!(
        read_event(dir.path(), DINNER)
            .participants
            .contains(&LinkOrText::parse("[[julie-dupont]]"))
    );
}

#[test]
fn a_contact_the_person_renamed_keeps_its_name_and_gets_updates() {
    let (dir, vault) = vault();
    let maman = |phone: &str| json!({"full_name": "Maman", "phones": [phone]});
    contacts(
        &vault,
        &[record("contact.card", "c-1", Some(maman("0611111111")))],
    );
    let path = dir.path().join("people/maman.md");
    let text = std::fs::read_to_string(&path).unwrap();
    std::fs::write(&path, text.replace("name: Maman", "name: Marie Dupont")).unwrap();

    let summary = contacts(
        &vault,
        &[record("contact.card", "c-1", Some(maman("0622222222")))],
    );
    assert_eq!((summary.updated, summary.conflicts), (1, 0));
    let file = MemoryFile::read(&path).unwrap();
    let entity = file.entity().unwrap();
    assert_eq!(entity.name.as_deref(), Some("Marie Dupont"));
    assert_eq!(
        entity.aliases,
        ["Maman"],
        "the address book's name is still understood"
    );
    assert_eq!(entity.phones, ["0622222222"]);

    let again = contacts(
        &vault,
        &[record("contact.card", "c-1", Some(maman("0622222222")))],
    );
    assert_eq!((again.updated, again.unchanged), (0, 1));
}

#[test]
fn a_file_the_person_deleted_is_not_created_again() {
    let (dir, vault) = vault();
    calendar(&vault, "calendar", &[event("diner@example.org", dinner())]);
    std::fs::remove_file(dir.path().join(DINNER)).unwrap();
    let mut later = dinner();
    later["title"] = json!("Dîner chez Julie et Marc");
    let (summary, _) = calendar(&vault, "calendar", &[event("diner@example.org", later)]);
    assert_eq!((summary.added, summary.deleted), (0, 1));
    assert!(
        !dir.path()
            .join("events/2026")
            .read_dir()
            .unwrap()
            .any(|_| true)
    );
}

#[test]
fn removed_events_are_cancelled_only_if_they_have_not_happened() {
    let (dir, vault) = vault();
    let mut past = dinner();
    past["start"] = json!("2026-11-01T20:00:00+01:00");
    past["end"] = json!("2026-11-01T23:00:00+01:00");
    past["title"] = json!("Dîner passé");
    calendar(
        &vault,
        "calendar",
        &[
            event("diner@example.org", dinner()),
            event("past@example.org", past),
        ],
    );
    let (summary, _) = calendar(
        &vault,
        "calendar",
        &[
            record("calendar.event", "diner@example.org", None),
            record("calendar.event", "past@example.org", None),
        ],
    );
    assert_eq!(summary.cancelled, 1);
    assert_eq!(
        read_event(dir.path(), DINNER).status,
        Some(EventStatus::Cancelled)
    );
    let past = read_event(dir.path(), "events/2026/2026-11-01-diner-passe.md");
    assert_eq!(past.status, None, "what happened stays");
}

#[test]
fn one_invitation_in_two_calendars_is_one_file() {
    let (dir, vault) = vault();
    calendar(&vault, "calendar", &[event("diner@example.org", dinner())]);
    let (summary, _) = calendar(
        &vault,
        "calendar-2",
        &[event("diner@example.org", dinner())],
    );
    assert_eq!((summary.added, summary.updated), (0, 1));
    let file = MemoryFile::read(dir.path().join(DINNER)).unwrap();
    assert_eq!(
        file.common().sources,
        ["calendar:diner@example.org", "calendar-2:diner@example.org"]
    );
    let events: Vec<PathBuf> = std::fs::read_dir(dir.path().join("events/2026"))
        .unwrap()
        .map(|e| e.unwrap().path())
        .collect();
    assert_eq!(events.len(), 1);
}

#[test]
fn a_series_is_not_a_file_but_its_occurrences_are() {
    let (dir, vault) = vault();
    let series = json!({
        "uid": "yoga", "title": "Yoga", "start": "2026-11-03T18:30:00+01:00",
        "rule": "FREQ=WEEKLY",
    });
    let occurrence = json!({
        "uid": "yoga", "recurrence_id": "2026-11-10T17:30:00Z", "title": "Yoga",
        "start": "2026-11-10T18:30:00+01:00", "end": "2026-11-10T19:30:00+01:00",
    });
    let (summary, _) = calendar(
        &vault,
        "calendar",
        &[
            event("yoga", series),
            event("yoga#2026-11-10T17:30:00Z", occurrence),
        ],
    );
    assert_eq!(summary.added, 1);
    assert!(dir.path().join("events/2026/2026-11-10-yoga.md").is_file());
}

#[test]
fn a_contact_the_person_corrected_keeps_their_correction() {
    let (dir, vault) = vault();
    contacts(
        &vault,
        &[record(
            "contact.card",
            "c-1",
            Some(person("Élodie Martin", "elodie@example.org")),
        )],
    );
    let path = dir.path().join("people/elodie-martin.md");
    let text = std::fs::read_to_string(&path).unwrap();
    std::fs::write(
        &path,
        text.replace("elodie@example.org", "elodie@perso.example"),
    )
    .unwrap();
    let summary = contacts(
        &vault,
        &[record(
            "contact.card",
            "c-1",
            Some(person("Élodie Martin", "elodie@travail.example")),
        )],
    );
    assert_eq!(summary.conflicts, 1);
    assert!(
        std::fs::read_to_string(&path)
            .unwrap()
            .contains("elodie@perso.example")
    );
}
