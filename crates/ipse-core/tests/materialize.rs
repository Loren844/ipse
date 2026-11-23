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
    import_from(vault, "contacts", records)
}

fn import_from(vault: &Vault, instance: &str, records: &[Record]) -> PeopleSummary {
    let mut journal = Journal::new(vault.root());
    journal.append(instance, records, None).unwrap();
    let history = vault.history().unwrap();
    let mut change = history.change(Source::Connector(instance.into()));
    let summary = contacts_to_people(vault, &journal, instance, &mut change).unwrap();
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
            proposals: 0,
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
    assert!(
        text.contains("- Organisation : Boulangerie du Coin. ^f-"),
        "{text}"
    );
    let with_fact: String = text
        .split_inclusive('\n')
        .flat_map(|line| {
            let fact = line
                .starts_with("- Organisation")
                .then_some("- Allergique aux noix.\n");
            std::iter::once(line).chain(fact)
        })
        .collect();
    std::fs::write(&path, &with_fact).unwrap();

    let mut changed = elodie();
    changed["emails"] = json!(["elodie.martin@example.org"]);
    import(&vault, &[card("c-1", changed)]);

    let after = read(&path);
    assert!(after.contains("favourite_colour: green\n"), "{after}");
    assert!(
        after.contains("- Allergique aux noix. ^f-"),
        "the person's fact is kept, with an id added: {after}"
    );
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

#[test]
fn a_conflict_becomes_a_proposal_from_the_source() {
    use ipse_core::proposals::{Accepted, Author, Change, Proposals, Status};

    let (dir, vault) = vault();
    import(&vault, &[card("c-1", elodie())]);
    let path = dir.path().join("people/elodie-martin.md");
    // The person changes her phone; the address book changes it too.
    std::fs::write(&path, read(&path).replace("0612345678", "0611111111")).unwrap();
    let mut changed = elodie();
    changed["phones"] = json!(["0622222222"]);
    let summary = import(&vault, &[card("c-1", changed.clone())]);
    assert_eq!((summary.conflicts, summary.proposals), (1, 1));
    assert!(
        read(&path).contains("0611111111"),
        "the person's value stays"
    );

    let proposals = Proposals::new(&vault);
    let all = proposals.list().unwrap();
    assert_eq!(all.len(), 1);
    let first = &all[0];
    assert_eq!(first.by, Author::Connector("contacts".into()));
    assert!(first.reason.contains("« contacts »"), "{}", first.reason);
    assert!(
        !first.reason.contains("06"),
        "no personal data in the reason"
    );
    assert_eq!(
        first.changes,
        [Change::SetKey {
            file: "people/elodie-martin.md".into(),
            key: "phones".into(),
            from: json!(["0611111111"]),
            to: json!(["0622222222"]),
        }]
    );

    // A later import changing the birthday too updates the same proposal.
    let (pid, created) = (first.id.clone(), first.created.clone());
    std::fs::write(
        &path,
        read(&path).replace("birthday: 1990-04-02", "birthday: 1990-04-03"),
    )
    .unwrap();
    changed["birthday"] = json!("1990-05-02");
    let summary = import(&vault, &[card("c-1", changed.clone())]);
    assert_eq!(summary.proposals, 1);
    let all = proposals.list().unwrap();
    assert_eq!(all.len(), 1, "one proposal per file");
    assert_eq!(
        (all[0].id.as_str(), all[0].created.as_str()),
        (pid.as_str(), created.as_str())
    );
    let keys: Vec<&str> = all[0]
        .changes
        .iter()
        .map(|c| match c {
            Change::SetKey { key, .. } => key.as_str(),
            _ => "",
        })
        .collect();
    assert_eq!(keys, ["birthday", "phones"]);

    // Refused: the person keeps theirs. The same value is not proposed again.
    proposals.refuse(&pid, None).unwrap();
    let summary = import(&vault, &[card("c-1", changed.clone())]);
    assert_eq!(summary.proposals, 0);
    assert_eq!(proposals.list().unwrap().len(), 1);

    // A newer value of the source is.
    changed["phones"] = json!(["0633333333"]);
    let summary = import(&vault, &[card("c-1", changed)]);
    assert_eq!(summary.proposals, 1);
    let newest = proposals.list().unwrap().remove(0);
    assert_ne!(newest.id, pid);
    assert_eq!(newest.status, Status::Waiting);
    assert!(matches!(
        proposals.accept(&newest.id).unwrap(),
        Accepted::Applied(Some(_))
    ));
    let after = read(&path);
    assert!(
        after.contains("0633333333") && !after.contains("0611111111"),
        "{after}"
    );
    assert!(
        after.contains("birthday: 1990-04-03"),
        "refused key untouched: {after}"
    );
}

#[test]
fn the_same_card_from_a_server_is_found_again_not_doubled() {
    let (dir, vault) = vault();
    import(&vault, &[card("c-1", elodie())]);
    // The same address book, later followed on a CalDAV/CardDAV server: same UID.
    let mut moved = elodie();
    moved["phones"] = json!(["0612345678", "0711111111"]);
    let summary = import_from(
        &vault,
        "dav-2",
        &[
            card("c-1", moved),
            card("c-9", json!({"full_name": "Marc Leroy"})),
        ],
    );
    assert_eq!((summary.added, summary.updated), (1, 1), "{summary:?}");
    let people: Vec<_> = std::fs::read_dir(dir.path().join("people"))
        .unwrap()
        .map(|e| e.unwrap().path())
        .filter(|p| read(p).contains("Élodie"))
        .collect();
    assert_eq!(people.len(), 1, "one file for Élodie");
    let text = read(&people[0]);
    assert!(
        text.contains("contacts:c-1") && text.contains("dav-2:c-1"),
        "{text}"
    );
    assert!(text.contains("0711111111"), "{text}");

    // Again: nothing more.
    let again = import_from(&vault, "dav-2", &[card("c-1", elodie())]);
    assert_eq!(again.added, 0);
}
