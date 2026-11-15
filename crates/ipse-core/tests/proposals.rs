//! Proposals: submit, preview, accept, refuse, undo (`docs/spec/vault.md`, section 12).

// Test helpers may panic: a failed expectation is a failed test.
#![allow(clippy::expect_used, clippy::unwrap_used)]

use std::path::Path;

use ipse_core::Error;
use ipse_core::proposals::{
    Accepted, Author, Change, DiffItem, NewRelation, Proposal, Proposals, Status, What,
};
use ipse_core::vault::{History, Language, Settings, Source, Vault};
use serde_json::{Value, json};

const MARIE: &str = "---
id: 01jcx8m2p4q6r8t0v2w4y6z8a0
type: person
category: people
created: 2026-01-10T09:00:00+01:00
updated: 2026-01-10T09:00:00+01:00
name: Marie Durand
phones:
  - \"+33612345678\"
---

## Faits

- Aime le jazz ^f-ab12
- Née à Lyon ^f-cd34

## Relations

- amie : [[paul-roux]]

## Notes

Rencontrée au club.
";

/// A vault with Marie's file, recorded as the person's.
fn vault(dir: &Path) -> Vault {
    Vault::init(
        dir,
        "Jules Moreau",
        Settings::new(Language::Fr, "Europe/Paris").unwrap(),
    )
    .unwrap();
    std::fs::write(dir.join("people/marie-durand.md"), MARIE).unwrap();
    History::open(dir).unwrap().record_outside_edits().unwrap();
    Vault::open(dir).unwrap()
}

fn marie(dir: &Path) -> String {
    std::fs::read_to_string(dir.join("people/marie-durand.md")).unwrap()
}

fn by() -> Author {
    Author::Client("claude".to_owned())
}

fn set_phone(from: Value, to: Value) -> Change {
    Change::SetKey {
        file: "people/marie-durand.md".into(),
        key: "phones".into(),
        from,
        to,
    }
}

/// Every kind of change but `create`, on Marie's file.
fn edits() -> Vec<Change> {
    let file = || "people/marie-durand.md".to_owned();
    vec![
        set_phone(json!(["+33612345678"]), json!(["+33698765432"])),
        Change::SetKey {
            file: file(),
            key: "birthday".into(),
            from: Value::Null,
            to: json!("1990-04-02"),
        },
        Change::AddFact {
            file: file(),
            text: "Allergique aux noix.".into(),
        },
        Change::EditFact {
            file: file(),
            fact: "f-cd34".into(),
            from: "Née à Lyon".into(),
            to: "Née à Lyon en 1990".into(),
        },
        Change::RemoveFact {
            file: file(),
            fact: "f-ab12".into(),
            text: "Aime le jazz".into(),
        },
        Change::AddRelation {
            file: file(),
            word: "sœur".into(),
            target: "lea-durand".into(),
            display: None,
        },
        Change::RemoveRelation {
            file: file(),
            word: "amie".into(),
            target: "paul-roux".into(),
        },
    ]
}

#[test]
fn every_kind_of_change_previews_applies_and_undoes() {
    let dir = tempfile::tempdir().unwrap();
    let vault = vault(dir.path());
    let proposals = Proposals::new(&vault);
    let proposal = proposals
        .submit(by(), "Mise à jour depuis la conversation.", None, edits())
        .unwrap();
    assert_eq!(proposal.status, Status::Waiting);
    assert_eq!(proposal.categories, ["people"]);

    let preview = proposals.preview(&proposal).unwrap();
    assert_eq!(preview.stale, None);
    assert_eq!(preview.files.len(), 1);
    let diff = &preview.files[0];
    assert_eq!(
        (diff.file.as_str(), diff.name.as_str(), diff.created),
        ("people/marie-durand.md", "Marie Durand", false)
    );
    let item = |what: What, before: Option<&str>, after: Option<&str>| DiffItem {
        what,
        before: before.map(str::to_owned),
        after: after.map(str::to_owned),
    };
    assert_eq!(
        diff.items,
        [
            item(
                What::Key("phones".into()),
                Some("+33612345678"),
                Some("+33698765432")
            ),
            item(What::Key("birthday".into()), None, Some("1990-04-02")),
            item(What::Fact, None, Some("Allergique aux noix.")),
            item(What::Fact, Some("Née à Lyon"), Some("Née à Lyon en 1990")),
            item(What::Fact, Some("Aime le jazz"), None),
            item(What::Relation, None, Some("sœur: lea-durand")),
            item(What::Relation, Some("amie: paul-roux"), None),
        ]
    );
    for i in &diff.items {
        for text in [&i.before, &i.after].into_iter().flatten() {
            assert!(!text.contains("^f-"), "no fact id on screen: {text}");
        }
    }

    let before = marie(dir.path());
    let Accepted::Applied(Some(change)) = proposals.accept(&proposal.id).unwrap() else {
        panic!("applied")
    };
    let after = marie(dir.path());
    assert!(after.contains("  - \"+33698765432\"") && !after.contains("+33612345678"));
    assert!(after.contains("birthday: 1990-04-02"));
    assert!(after.contains("- Née à Lyon en 1990 ^f-cd34\n"));
    assert!(!after.contains("Aime le jazz"));
    assert!(
        after.contains("- Allergique aux noix. ^f-"),
        "new fact gets an id"
    );
    assert!(after.contains("- sœur : [[lea-durand]]\n") && !after.contains("paul-roux"));
    assert!(
        after.contains("## Notes\n\nRencontrée au club.\n"),
        "notes kept"
    );

    let history = History::open(dir.path()).unwrap();
    let last = &history.entries(1).unwrap()[0];
    assert_eq!(last.id, change);
    assert_eq!(last.source, Some(Source::Proposal(proposal.id.clone())));
    assert_eq!(last.summary, "Accept a proposal");
    assert_eq!(last.files, ["people/marie-durand.md"]);
    assert!(matches!(
        proposals.get(&proposal.id).unwrap().status,
        Status::Accepted { change: Some(c), .. } if c == change
    ));
    assert!(matches!(
        proposals.accept(&proposal.id),
        Err(Error::Refused(_))
    ));

    let undone = proposals.undo(&proposal.id).unwrap();
    assert!(matches!(
        undone.status,
        Status::Undone {
            change: Some(_),
            ..
        }
    ));
    assert_eq!(marie(dir.path()), before, "byte for byte");
    let last = &history.entries(1).unwrap()[0];
    assert_eq!(last.source, Some(Source::Person));
    assert_eq!(last.summary, "Undo a proposal");
}

#[test]
fn a_new_person_and_a_new_event() {
    let dir = tempfile::tempdir().unwrap();
    let vault = vault(dir.path());
    let proposals = Proposals::new(&vault);
    let proposal = proposals
        .submit(
            by(),
            "Léa est mentionnée dans les notes de Marie.",
            None,
            vec![
                Change::Create {
                    file: None,
                    file_type: "person".into(),
                    category: None,
                    keys: [("name".to_owned(), json!("Léa Durand"))].into(),
                    facts: vec!["Sœur de Marie.".into()],
                    relations: vec![NewRelation {
                        word: "sœur".into(),
                        target: "marie-durand".into(),
                        display: None,
                    }],
                    notes: None,
                },
                Change::Create {
                    file: None,
                    file_type: "event".into(),
                    category: None,
                    keys: [
                        ("title".to_owned(), json!("Anniversaire de Léa")),
                        ("start".to_owned(), json!("2026-12-05")),
                        ("participants".to_owned(), json!(["[[lea-durand]]"])),
                    ]
                    .into(),
                    facts: vec![],
                    relations: vec![],
                    notes: None,
                },
            ],
        )
        .unwrap();
    assert_eq!(proposal.categories, ["agenda", "people"]);
    let files: Vec<&str> = proposal.changes.iter().filter_map(Change::file).collect();
    assert_eq!(
        files,
        [
            "people/lea-durand.md",
            "events/2026/2026-12-05-anniversaire-de-lea.md"
        ]
    );
    let preview = proposals.preview(&proposal).unwrap();
    assert!(preview.files.iter().all(|f| f.created));
    assert_eq!(preview.files[1].name, "Anniversaire de Léa");

    assert!(matches!(
        proposals.accept(&proposal.id[..8]).unwrap(),
        Accepted::Applied(Some(_))
    ));
    let lea = std::fs::read_to_string(dir.path().join("people/lea-durand.md")).unwrap();
    assert!(lea.contains("name: Léa Durand"));
    assert!(
        lea.contains(&format!("  - proposal:{}", proposal.id)),
        "{lea}"
    );
    assert!(lea.contains("- Sœur de Marie. ^f-"));
    let event = dir
        .path()
        .join("events/2026/2026-12-05-anniversaire-de-lea.md");
    assert!(
        std::fs::read_to_string(&event)
            .unwrap()
            .contains("category: agenda")
    );

    // Undo removes the files it created.
    proposals.undo(&proposal.id).unwrap();
    assert!(!dir.path().join("people/lea-durand.md").exists());
    assert!(!event.exists());
}

#[test]
fn a_file_the_person_changed_since_makes_the_proposal_stale() {
    let dir = tempfile::tempdir().unwrap();
    let vault = vault(dir.path());
    let proposals = Proposals::new(&vault);
    let proposal = proposals
        .submit(
            by(),
            "Nouveau numéro.",
            None,
            vec![set_phone(json!(["+33612345678"]), json!(["+33698765432"]))],
        )
        .unwrap();
    let edited = MARIE.replace("+33612345678", "+33611111111");
    std::fs::write(dir.path().join("people/marie-durand.md"), &edited).unwrap();

    let preview = proposals.preview(&proposal).unwrap();
    assert!(
        preview
            .stale
            .as_deref()
            .unwrap()
            .contains("`phones` of people/marie-durand.md changed")
    );
    let Accepted::Stale(reason) = proposals.accept(&proposal.id).unwrap() else {
        panic!("stale")
    };
    assert!(!reason.contains("+336"), "{reason}");
    assert_eq!(marie(dir.path()), edited, "nothing written");
    assert!(matches!(
        proposals.get(&proposal.id).unwrap().status,
        Status::Stale { .. }
    ));
    // A stale proposal can only be refused.
    assert!(matches!(
        proposals.accept(&proposal.id),
        Err(Error::Refused(_))
    ));
    let refused = proposals
        .refuse(&proposal.id, Some("  Déjà fait  "))
        .unwrap();
    assert!(matches!(
        refused.status,
        Status::Refused { comment: Some(c), .. } if c == "Déjà fait"
    ));
}

#[test]
fn a_removed_fact_or_file_makes_it_stale_too() {
    let dir = tempfile::tempdir().unwrap();
    let vault = vault(dir.path());
    let proposals = Proposals::new(&vault);
    let proposal = proposals
        .submit(by(), "Précision.", None, edits()[3..4].to_vec())
        .unwrap();
    std::fs::write(
        dir.path().join("people/marie-durand.md"),
        MARIE.replace("- Née à Lyon ^f-cd34\n", ""),
    )
    .unwrap();
    assert!(
        proposals
            .preview(&proposal)
            .unwrap()
            .stale
            .unwrap()
            .contains("was removed")
    );
    std::fs::remove_file(dir.path().join("people/marie-durand.md")).unwrap();
    assert!(
        proposals
            .preview(&proposal)
            .unwrap()
            .stale
            .unwrap()
            .contains("no longer exists")
    );
}

#[test]
fn undo_is_refused_when_the_file_changed_since() {
    let dir = tempfile::tempdir().unwrap();
    let vault = vault(dir.path());
    let proposals = Proposals::new(&vault);
    let proposal = proposals
        .submit(by(), "Fait.", None, edits()[2..3].to_vec())
        .unwrap();
    proposals.accept(&proposal.id).unwrap();
    let path = dir.path().join("people/marie-durand.md");
    let mut text = std::fs::read_to_string(&path).unwrap();
    text.push_str("Une ligne de plus.\n");
    std::fs::write(&path, &text).unwrap();
    let Err(Error::Refused(message)) = proposals.undo(&proposal.id) else {
        panic!("refused")
    };
    assert!(
        message.contains("people/marie-durand.md changed since"),
        "{message}"
    );
    assert_eq!(std::fs::read_to_string(&path).unwrap(), text);
    assert!(matches!(
        proposals.get(&proposal.id).unwrap().status,
        Status::Accepted { .. }
    ));
}

#[test]
fn submit_refuses_what_cannot_apply_in_plain_words_without_the_values() {
    let dir = tempfile::tempdir().unwrap();
    let vault = vault(dir.path());
    let proposals = Proposals::new(&vault);
    let refused = |changes: Vec<Change>| match proposals.submit(by(), "Raison.", None, changes) {
        Err(Error::Refused(message)) => message,
        other => panic!("{other:?}"),
    };
    let file = || "people/marie-durand.md".to_owned();

    let message = refused(vec![set_phone(
        json!(["+33600000001"]),
        json!(["+33600000002"]),
    )]);
    assert!(message.contains("changed since"), "{message}");
    assert!(!message.contains("+336"), "{message}");
    let message = refused(vec![Change::SetKey {
        file: file(),
        key: "birthday".into(),
        from: Value::Null,
        to: json!("le 2 avril SECRET"),
    }]);
    assert!(message.contains("`birthday` must be a date"), "{message}");
    assert!(!message.contains("SECRET"), "{message}");
    for (change, expected) in [
        (
            Change::SetKey {
                file: file(),
                key: "owner".into(),
                from: Value::Null,
                to: json!(true),
            },
            "`owner` cannot be set",
        ),
        (
            Change::AddFact {
                file: "people/nobody.md".into(),
                text: "x".into(),
            },
            "There is no file people/nobody.md",
        ),
        (
            Change::AddFact {
                file: "../outside.md".into(),
                text: "x".into(),
            },
            "is not the path of a memory file",
        ),
        (
            Change::AddFact {
                file: "journal/contacts/x.md".into(),
                text: "x".into(),
            },
            "is not the path of a memory file",
        ),
        (
            Change::RemoveFact {
                file: file(),
                fact: "nope".into(),
                text: "x".into(),
            },
            "is not a fact identifier",
        ),
        (
            Change::AddFact {
                file: file(),
                text: "Aime le jazz".into(),
            },
            "changes nothing",
        ),
        (
            Change::AddRelation {
                file: file(),
                word: "ami".into(),
                target: "people/x".into(),
                display: None,
            },
            "is not a file name",
        ),
    ] {
        let message = refused(vec![change]);
        assert!(message.contains(expected), "{message}");
    }
    assert!(matches!(
        proposals.submit(by(), "  ", None, edits()),
        Err(Error::Refused(_))
    ));
    assert!(proposals.list().unwrap().is_empty(), "nothing saved");
}

#[test]
fn proposals_are_listed_newest_first_and_read_from_their_format() {
    let dir = tempfile::tempdir().unwrap();
    let vault = vault(dir.path());
    let proposals = Proposals::new(&vault);
    let first = proposals
        .submit(by(), "Un.", None, edits()[2..3].to_vec())
        .unwrap();
    std::thread::sleep(std::time::Duration::from_millis(2));
    let second = proposals
        .submit(by(), "Deux.", None, edits()[1..2].to_vec())
        .unwrap();
    let ids: Vec<String> = proposals
        .list()
        .unwrap()
        .into_iter()
        .map(|p| p.id)
        .collect();
    assert_eq!(ids, [second.id.clone(), first.id.clone()]);

    // The format as saved, read back.
    let saved = dir
        .path()
        .join(format!(".ipse/proposals/{}.json", first.id));
    let value: Value = serde_json::from_str(&std::fs::read_to_string(&saved).unwrap()).unwrap();
    assert_eq!(value["format"], 1);
    assert_eq!(value["by"], "client:claude");
    assert_eq!(value["status"], json!({"state": "waiting"}));
    assert_eq!(
        value["changes"][0],
        json!({"kind": "add_fact", "file": "people/marie-durand.md", "text": "Allergique aux noix."})
    );
    let back: Proposal = serde_json::from_value(value).unwrap();
    assert_eq!(back, first);

    // A newer format is refused in plain words; a broken file is skipped in the list.
    std::fs::write(
        dir.path()
            .join(".ipse/proposals/01zzzzzzzzzzzzzzzzzzzzzzzz.json"),
        r#"{"format": 9}"#,
    )
    .unwrap();
    assert!(matches!(
        proposals.get("01zzzzzzzzzzzzzzzzzzzzzzzz"),
        Err(Error::UnsupportedVersion { .. })
    ));
    assert_eq!(proposals.list().unwrap().len(), 2);
    assert!(matches!(proposals.get("zz"), Err(Error::Refused(_))));
}

#[test]
fn a_decided_proposal_is_described_as_it_was_written() {
    let dir = tempfile::tempdir().unwrap();
    let vault = vault(dir.path());
    let proposals = Proposals::new(&vault);
    let proposal = proposals
        .submit(by(), "Mise à jour depuis la conversation.", None, edits())
        .unwrap();
    let preview = proposals.preview(&proposal).unwrap();
    proposals.accept(&proposal.id).unwrap();

    // Once accepted, the files hold the changes: the preview finds nothing left to do,
    // the description still says what the proposal held, in the same words.
    let described = proposals.described(&proposal);
    assert_eq!(described.stale, None);
    assert_eq!(described.files.len(), 1);
    let file = &described.files[0];
    assert_eq!(
        (
            file.name.as_str(),
            file.file_type.as_str(),
            file.category.as_str()
        ),
        ("Marie Durand", "person", "people")
    );
    assert_eq!(file.items, preview.files[0].items);

    // A new file is named from its keys, even once it is gone.
    let created = Proposal {
        changes: vec![Change::Create {
            file: Some("people/lea-durand.md".into()),
            file_type: "person".into(),
            category: None,
            keys: [("name".to_owned(), json!("Léa Durand"))].into(),
            facts: vec!["Sœur de Marie.".into()],
            relations: vec![NewRelation {
                word: "sœur".into(),
                target: "marie-durand".into(),
                display: None,
            }],
            notes: None,
        }],
        ..proposal
    };
    let described = proposals.described(&created);
    let file = &described.files[0];
    assert!(file.created);
    assert_eq!(file.name, "Léa Durand");
    assert_eq!(
        file.items,
        [
            DiffItem {
                what: What::Key("name".into()),
                before: None,
                after: Some("Léa Durand".into())
            },
            DiffItem {
                what: What::Fact,
                before: None,
                after: Some("Sœur de Marie.".into())
            },
            DiffItem {
                what: What::Relation,
                before: None,
                after: Some("sœur: marie-durand".into())
            },
        ]
    );
}
