//! Contacts in the journal become people files in the vault.

// Test helpers may panic: a failed expectation is a failed test.
#![allow(clippy::expect_used, clippy::unwrap_used)]

use std::path::Path;

use ipse_core::journal::Journal;
use ipse_core::materialize::{PeopleSummary, contacts_to_people};
use ipse_core::vault::{Language, MemoryFile, Settings, Source, Vault};
use ipse_protocol::{Op, Record};
use serde_json::{Value, json};

fn card(id: &str, data: Value) -> Record {
    Record {
        kind: "contact.card".into(),
        op: Op::Upsert,
        external_id: Some(id.into()),
        observed_at: "2026-11-12T18:30:00Z".parse().unwrap(),
        data: Some(data),
    }
}

fn elodie() -> Value {
    json!({
        "full_name": "Élodie Martin",
        "nicknames": ["Lolo"],
        "emails": ["elodie@example.org"],
        "phones": ["0612345678"],
        "birthday": "1990-04-02",
        "organization": "Boulangerie du Coin",
        "note": "Rencontrée au club."
    })
}

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

fn import(vault: &Vault, records: &[Record]) -> PeopleSummary {
    let mut journal = Journal::new(vault.root());
    journal.append("contacts", records, None).unwrap();
    let history = vault.history().unwrap();
    let mut change = history.change(Source::Connector("contacts".into()));
    let summary = contacts_to_people(vault, &journal, "contacts", &mut change).unwrap();
    change.commit("Import contacts").unwrap();
    summary
}

fn read(path: &Path) -> String {
    std::fs::read_to_string(path).unwrap()
}

#[test]
fn each_contact_becomes_a_person_file() {
    let (dir, vault) = vault();
    let summary = import(
        &vault,
        &[
            card("c-1", elodie()),
            card("c-2", json!({"full_name": "Marc Leroy"})),
            card("c-3", json!({"note": "no name"})),
        ],
    );
    assert_eq!(
        summary,
        PeopleSummary {
            added: 2,
            updated: 0,
            unchanged: 0,
            skipped: 1,
            conflicts: 0,
            deleted: 0,
            relations: 0,
            owner: false,
        }
    );

    let path = dir.path().join("people/elodie-martin.md");
    let file = MemoryFile::read(&path).unwrap();
    assert_eq!(file.common().category.as_str(), "people");
    assert_eq!(file.common().sources, ["contacts:c-1"]);
    let entity = file.entity().unwrap();
    assert_eq!(entity.name.as_deref(), Some("Élodie Martin"));
    assert_eq!(entity.aliases, ["Lolo"]);
    assert_eq!(entity.emails, ["elodie@example.org"]);
    assert_eq!(entity.phones, ["0612345678"]);
    assert_eq!(entity.birthday.unwrap().to_string(), "1990-04-02");
    assert!(!entity.owner);

    let sections = file.sections();
    assert_eq!(
        sections.facts[0].text,
        "Organisation : Boulangerie du Coin."
    );
    assert_eq!(sections.notes.as_deref(), Some("Rencontrée au club."));
    assert!(dir.path().join("people/marc-leroy.md").is_file());
}

#[test]
fn reimporting_unchanged_contacts_writes_nothing() {
    let (dir, vault) = vault();
    import(&vault, &[card("c-1", elodie())]);
    let path = dir.path().join("people/elodie-martin.md");
    let before = read(&path);
    let modified = std::fs::metadata(&path).unwrap().modified().unwrap();

    let summary = import(&vault, &[card("c-1", elodie())]);
    assert_eq!(
        (summary.added, summary.updated, summary.unchanged),
        (0, 0, 1)
    );
    assert_eq!(read(&path), before);
    assert_eq!(
        std::fs::metadata(&path).unwrap().modified().unwrap(),
        modified
    );
}

#[test]
fn a_changed_phone_changes_only_that_file() {
    let (dir, vault) = vault();
    import(
        &vault,
        &[
            card("c-1", elodie()),
            card("c-2", json!({"full_name": "Marc Leroy"})),
        ],
    );
    let marc = dir.path().join("people/marc-leroy.md");
    let marc_before = read(&marc);

    let mut changed = elodie();
    changed["phones"] = json!(["+33612345678"]);
    let summary = import(
        &vault,
        &[
            card("c-1", changed),
            card("c-2", json!({"full_name": "Marc Leroy"})),
        ],
    );
    assert_eq!(
        (summary.added, summary.updated, summary.unchanged),
        (0, 1, 1)
    );

    let elodie = MemoryFile::read(dir.path().join("people/elodie-martin.md")).unwrap();
    assert_eq!(elodie.entity().unwrap().phones, ["+33612345678"]);
    assert!(elodie.common().updated.timestamp() >= elodie.common().created.timestamp());
    assert_eq!(read(&marc), marc_before);
}

#[test]
fn what_the_person_wrote_survives_a_reimport() {
    let (dir, vault) = vault();
    import(&vault, &[card("c-1", elodie())]);
    let path = dir.path().join("people/elodie-martin.md");
    let text = read(&path).replace(
        "name: Élodie Martin\n",
        "name: Élodie Martin\nfavourite_colour: green\n",
    ) + "\n## Relations\n\n- sœur : [[lea-martin]]\n";
    std::fs::write(&path, &text).unwrap();
    let with_fact = text.replace(
        "- Organisation : Boulangerie du Coin.\n",
        "- Organisation : Boulangerie du Coin.\n- Allergique aux noix.\n",
    );
    std::fs::write(&path, &with_fact).unwrap();

    let mut changed = elodie();
    changed["emails"] = json!(["elodie.martin@example.org"]);
    import(&vault, &[card("c-1", changed)]);

    let after = read(&path);
    assert!(after.contains("favourite_colour: green\n"), "{after}");
    assert!(after.contains("- Allergique aux noix.\n"));
    assert!(after.contains("- sœur : [[lea-martin]]\n"));
    assert!(after.contains("  - elodie.martin@example.org\n"));
    assert!(!after.contains("  - elodie@example.org\n"));
}

#[test]
fn people_who_look_alike_are_never_merged() {
    let (dir, vault) = vault();
    let twin = json!({"full_name": "Élodie Martin", "phones": ["0612345678"]});
    let summary = import(&vault, &[card("c-1", elodie()), card("other-app-7", twin)]);
    assert_eq!(summary.added, 2, "decision 0011: no silent merge");
    assert!(dir.path().join("people/elodie-martin.md").is_file());
    assert!(dir.path().join("people/elodie-martin-2.md").is_file());
}

#[test]
fn the_owner_file_is_left_alone() {
    let (_dir, vault) = vault();
    let before = read(vault.owner_file());
    import(
        &vault,
        &[card("c-1", json!({"full_name": "Camille Durand"}))],
    );
    assert_eq!(read(vault.owner_file()), before);
}
