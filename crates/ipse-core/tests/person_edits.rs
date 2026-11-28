//! The person's own edits from the interface: written at once, by the same code as a
//! proposal's changes, as one change of the history by the person (decision 0016).

// Test helpers may panic: a failed expectation is a failed test.
#![allow(clippy::expect_used, clippy::unwrap_used)]

use std::path::Path;

use ipse_core::Error;
use ipse_core::journal::Journal;
use ipse_core::materialize::contacts_to_people;
use ipse_core::proposals::{
    Accepted, Author, Change, Draft, DraftFact, NewRelation, Proposals, version,
};
use ipse_core::vault::{History, Language, MemoryFile, Settings, Source, Vault};
use ipse_protocol::{Op, Record};
use serde_json::json;

const FILE: &str = "people/marie-durand.md";

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
- Joue du piano ^f-ef56

## Relations

- amie : [[paul-roux]]

## Notes

Rencontrée au club.
";

fn vault(dir: &Path) -> Vault {
    Vault::init(
        dir,
        "Jules Moreau",
        Settings::new(Language::Fr, "Europe/Paris").unwrap(),
    )
    .unwrap();
    std::fs::write(dir.join(FILE), MARIE).unwrap();
    History::open(dir).unwrap().record_outside_edits().unwrap();
    Vault::open(dir).unwrap()
}

fn read(dir: &Path, file: &str) -> String {
    std::fs::read_to_string(dir.join(file)).unwrap()
}

fn now(dir: &Path, file: &str) -> String {
    version(&dir.join(file)).unwrap()
}

fn kept(was: usize, text: &str) -> DraftFact {
    DraftFact {
        was: Some(was),
        text: text.to_owned(),
    }
}

fn draft() -> Draft {
    Draft {
        keys: [
            ("phones".to_owned(), json!(["+33698765432", " "])),
            ("birthday".to_owned(), json!("1990-04-02")),
            ("address".to_owned(), json!("  ")),
        ]
        .into(),
        facts: Some(vec![
            kept(0, "Aime le jazz et la soul"),
            kept(2, "Joue du piano"),
            DraftFact {
                was: None,
                text: "Travaille à Nantes".to_owned(),
            },
        ]),
        relations: Some(vec![NewRelation {
            word: "sœur".to_owned(),
            target: "lea-durand".to_owned(),
            display: None,
        }]),
        notes: Some("Rencontrée au club.\n\nAppelle le dimanche.".to_owned()),
    }
}

#[test]
fn an_edit_is_one_change_by_the_person_and_can_be_undone() {
    let dir = tempfile::tempdir().unwrap();
    let vault = vault(dir.path());
    let proposals = Proposals::new(&vault);
    let change = proposals
        .edit(FILE, &now(dir.path(), FILE), &draft())
        .unwrap()
        .expect("the file changed");

    let after = read(dir.path(), FILE);
    assert!(after.contains("+33698765432") && !after.contains("+33612345678"));
    assert!(after.contains("birthday: 1990-04-02"));
    assert!(!after.contains("address"), "an empty value is none");
    assert!(
        after.contains("- Aime le jazz et la soul ^f-ab12"),
        "{after}"
    );
    assert!(!after.contains("Née à Lyon"));
    assert!(after.contains("- Joue du piano ^f-ef56"));
    assert!(
        after.contains("- Travaille à Nantes ^f-"),
        "a new fact gets an id"
    );
    assert!(!after.contains("[[paul-roux]]"));
    assert!(after.contains("- sœur : [[lea-durand]]"));
    assert!(after.contains("## Notes\n\nRencontrée au club.\n\nAppelle le dimanche.\n"));
    assert!(
        !after.contains("sources:"),
        "the person's own change adds no source"
    );

    let history = vault.history().unwrap();
    let entries = history.entries(10).unwrap();
    assert_eq!(entries[0].id, change);
    assert_eq!(entries[0].source, Some(Source::Person));
    assert_eq!(entries[0].summary, "Edit a file");

    history
        .undo(&change, Source::Person, "Undo a change")
        .unwrap();
    assert_eq!(read(dir.path(), FILE), MARIE);
}

#[test]
fn saving_the_same_values_changes_nothing() {
    let dir = tempfile::tempdir().unwrap();
    let vault = vault(dir.path());
    let same = Draft {
        keys: [("name".to_owned(), json!("Marie Durand"))].into(),
        facts: Some(vec![
            kept(0, "Aime le jazz"),
            kept(1, " Née  à Lyon "),
            kept(2, "Joue du piano"),
        ]),
        relations: Some(vec![NewRelation {
            word: "amie".to_owned(),
            target: "paul-roux".to_owned(),
            display: None,
        }]),
        notes: Some("Rencontrée au club.\n".to_owned()),
    };
    let saved = Proposals::new(&vault)
        .edit(FILE, &now(dir.path(), FILE), &same)
        .unwrap();
    assert_eq!(saved, None);
    assert_eq!(read(dir.path(), FILE), MARIE);
}

#[test]
fn a_file_changed_since_it_was_read_is_not_written_over() {
    let dir = tempfile::tempdir().unwrap();
    let vault = vault(dir.path());
    let read_at = now(dir.path(), FILE);
    std::fs::write(dir.path().join(FILE), MARIE.replace("Rencontrée", "Vue")).unwrap();
    let Err(Error::Refused(message)) = Proposals::new(&vault).edit(FILE, &read_at, &draft()) else {
        panic!("written over")
    };
    assert!(message.contains("changed since you opened it"), "{message}");
    assert!(read(dir.path(), FILE).contains("Vue au club"));
}

#[test]
fn a_value_that_is_not_valid_is_refused_in_plain_words() {
    let dir = tempfile::tempdir().unwrap();
    let vault = vault(dir.path());
    let bad = Draft {
        keys: [("birthday".to_owned(), json!("the second of April"))].into(),
        ..Draft::default()
    };
    let Err(Error::Refused(message)) =
        Proposals::new(&vault).edit(FILE, &now(dir.path(), FILE), &bad)
    else {
        panic!("accepted")
    };
    assert!(message.contains("birthday"), "{message}");
    assert!(!message.contains("April"), "the value is not repeated");
    assert_eq!(read(dir.path(), FILE), MARIE);
}

#[test]
fn facts_written_without_ids_can_be_edited() {
    let dir = tempfile::tempdir().unwrap();
    let vault = vault(dir.path());
    std::fs::write(
        dir.path().join(FILE),
        MARIE.replace(" ^f-cd34", "").replace(" ^f-ab12", ""),
    )
    .unwrap();
    let edit = Draft {
        facts: Some(vec![
            kept(1, "Née à Lyon en 1990"),
            kept(2, "Joue du piano"),
        ]),
        ..Draft::default()
    };
    Proposals::new(&vault)
        .edit(FILE, &now(dir.path(), FILE), &edit)
        .unwrap();
    let after = read(dir.path(), FILE);
    assert!(!after.contains("Aime le jazz"), "{after}");
    assert!(after.contains("- Née à Lyon en 1990 ^f-"), "{after}");
    assert!(after.contains("Rencontrée au club."), "notes are kept");
}

#[test]
fn a_proposal_made_from_the_old_text_becomes_stale() {
    let dir = tempfile::tempdir().unwrap();
    let vault = vault(dir.path());
    let proposals = Proposals::new(&vault);
    let proposal = proposals
        .submit(
            Author::Client("claude".to_owned()),
            "Précisé dans une conversation.",
            None,
            vec![Change::EditFact {
                file: FILE.to_owned(),
                fact: "f-ab12".to_owned(),
                from: "Aime le jazz".to_owned(),
                to: "Aime le jazz manouche".to_owned(),
            }],
        )
        .unwrap();
    proposals
        .edit(FILE, &now(dir.path(), FILE), &draft())
        .unwrap();
    assert!(matches!(
        proposals.accept(&proposal.id).unwrap(),
        Accepted::Stale(_)
    ));
    assert!(read(dir.path(), FILE).contains("Aime le jazz et la soul"));
}

#[test]
fn an_ai_cannot_propose_to_rewrite_the_notes() {
    let change = json!({"kind": "set_notes", "file": FILE, "from": null, "to": "Autre chose."});
    assert!(serde_json::from_value::<Change>(change).is_err());
}

#[test]
fn a_new_file_is_the_persons_and_can_take_the_name_a_link_gives() {
    let dir = tempfile::tempdir().unwrap();
    let vault = vault(dir.path());
    let proposals = Proposals::new(&vault);
    // Marie's file links to [[paul-roux]], which does not exist yet.
    let created = proposals
        .create(
            Change::Create {
                file: None,
                file_type: "person".to_owned(),
                category: None,
                keys: [("name".to_owned(), json!("Paul Roux"))].into(),
                facts: vec!["Joue au club.".to_owned()],
                relations: Vec::new(),
                notes: None,
            },
            Some("paul-roux"),
        )
        .unwrap();
    assert_eq!(created.file, "people/paul-roux.md");
    let file = MemoryFile::read(dir.path().join(&created.file)).unwrap();
    assert_eq!(file.common().id.to_string(), created.id);
    assert!(file.common().sources.is_empty(), "the person's own file");
    let entries = vault.history().unwrap().entries(1).unwrap();
    assert_eq!(entries[0].source, Some(Source::Person));
    assert_eq!(entries[0].summary, "Create a file");

    // A name a link gives that does not fit its type of file is ignored.
    let event = proposals
        .create(
            Change::Create {
                file: None,
                file_type: "event".to_owned(),
                category: None,
                keys: [
                    ("title".to_owned(), json!("Concert")),
                    ("start".to_owned(), json!("2026-12-04T20:00:00+01:00")),
                ]
                .into(),
                facts: Vec::new(),
                relations: Vec::new(),
                notes: Some("Avec Marie.".to_owned()),
            },
            Some("paul-roux"),
        )
        .unwrap();
    assert_eq!(event.file, "events/2026/2026-12-04-concert.md");

    // Only new files.
    assert!(matches!(
        proposals.create(
            Change::AddFact {
                file: FILE.to_owned(),
                text: "x".to_owned()
            },
            None
        ),
        Err(Error::Refused(_))
    ));
}

#[test]
fn a_deleted_file_comes_back_when_undone_and_the_owner_file_stays() {
    let dir = tempfile::tempdir().unwrap();
    let vault = vault(dir.path());
    let proposals = Proposals::new(&vault);
    let change = proposals
        .delete(FILE, &now(dir.path(), FILE))
        .unwrap()
        .expect("removed");
    assert!(!dir.path().join(FILE).exists());
    let history = vault.history().unwrap();
    assert_eq!(history.entries(1).unwrap()[0].summary, "Delete a file");
    history
        .undo(&change, Source::Person, "Undo a change")
        .unwrap();
    assert_eq!(read(dir.path(), FILE), MARIE);

    let owner = vault
        .owner_file()
        .strip_prefix(dir.path())
        .unwrap()
        .to_string_lossy()
        .replace('\\', "/");
    let Err(Error::Refused(message)) = proposals.delete(&owner, &now(dir.path(), &owner)) else {
        panic!("deleted")
    };
    assert!(message.contains("own file"), "{message}");
    assert!(vault.owner_file().exists());

    // Only memory files.
    assert!(matches!(
        proposals.delete("vault.toml", "x"),
        Err(Error::Refused(_))
    ));
}

#[test]
fn a_source_never_writes_over_what_the_person_edited() {
    let dir = tempfile::tempdir().unwrap();
    let vault = Vault::init(
        dir.path(),
        "Jules Moreau",
        Settings::new(Language::Fr, "Europe/Paris").unwrap(),
    )
    .unwrap();
    let card = |phone: &str| Record {
        kind: "contact.card".into(),
        op: Op::Upsert,
        external_id: Some("c-1".into()),
        observed_at: "2026-11-12T18:30:00Z".parse().unwrap(),
        data: Some(json!({"full_name": "Élodie Martin", "phones": [phone]})),
    };
    let import = |record: Record| {
        let mut journal = Journal::new(vault.root());
        journal.append("contacts", &[record], None).unwrap();
        let history = vault.history().unwrap();
        let mut change = history
            .change(Source::Connector("contacts".into()))
            .unwrap();
        let summary = contacts_to_people(&vault, &journal, "contacts", &mut change).unwrap();
        change.commit("Import contacts").unwrap();
        summary
    };
    import(card("0612345678"));
    let file = "people/elodie-martin.md";
    let edit = Draft {
        keys: [("phones".to_owned(), json!(["0611111111"]))].into(),
        ..Draft::default()
    };
    Proposals::new(&vault)
        .edit(file, &now(dir.path(), file), &edit)
        .unwrap();
    let summary = import(card("0622222222"));
    assert_eq!(summary.proposals, 1, "the source proposes instead");
    let after = read(dir.path(), file);
    assert!(after.contains("0611111111") && !after.contains("0622222222"));
}

/// The refusal's message, for a change that must be refused.
fn refusal(result: ipse_core::Result<Option<String>>) -> String {
    match result {
        Err(Error::Refused(message)) => message,
        other => panic!("expected a refusal, got {other:?}"),
    }
}

#[test]
fn the_file_as_markdown_is_one_change_by_the_person() {
    let dir = tempfile::tempdir().unwrap();
    let vault = vault(dir.path());
    let proposals = Proposals::new(&vault);
    let typed = MARIE
        .replace(
            "- Joue du piano ^f-ef56\n",
            "- Joue du piano ^f-ef56\n- Court le dimanche\n",
        )
        .replace(
            "## Notes\n",
            "## Voyages\n\nLisbonne en 2024.\n\n## Notes\n",
        );
    let change = proposals
        .edit_text(FILE, &now(dir.path(), FILE), &typed)
        .unwrap()
        .expect("the file changed");

    let after = read(dir.path(), FILE);
    assert!(
        after.contains("## Voyages\n\nLisbonne en 2024.\n"),
        "{after}"
    );
    assert!(after.contains("- Aime le jazz ^f-ab12"), "ids kept");
    assert!(
        after.contains("- Court le dimanche ^f-"),
        "a new fact gets an id"
    );
    assert!(!after.contains("updated: 2026-01-10"), "updated is today");
    let entries = vault.history().unwrap().entries(10).unwrap();
    assert_eq!(entries[0].id, change);
    assert_eq!(entries[0].source, Some(Source::Person));
    assert_eq!(entries[0].summary, "Edit a file as Markdown");

    // The same text again changes nothing.
    let same = read(dir.path(), FILE);
    assert!(
        proposals
            .edit_text(FILE, &now(dir.path(), FILE), &same)
            .unwrap()
            .is_none()
    );

    vault
        .history()
        .unwrap()
        .undo(&change, Source::Person, "Undo a change")
        .unwrap();
    assert_eq!(read(dir.path(), FILE), MARIE);
}

#[test]
fn markdown_that_does_not_read_as_a_file_is_refused_in_words() {
    let dir = tempfile::tempdir().unwrap();
    let vault = vault(dir.path());
    let proposals = Proposals::new(&vault);
    let at = now(dir.path(), FILE);
    let try_text = |text: &str| refusal(proposals.edit_text(FILE, &at, text));

    assert!(
        try_text("Marie Durand\n").contains("front matter"),
        "no front matter"
    );
    let message = try_text(&MARIE.replace("name: Marie Durand\n", "name: [Marie\n"));
    assert!(message.starts_with("Line "), "{message}");
    assert_eq!(
        try_text(&MARIE.replace("01jcx8m2p4q6r8t0v2w4y6z8a0", "01jcx8m2p4q6r8t0v2w4y6z8a1")),
        "The `id` cannot be changed."
    );
    assert!(try_text(&MARIE.replace("type: person", "type: place")).contains("`type`"));
    assert!(try_text(&MARIE.replace("category: people", "category: amis")).contains("`category`"));
    assert!(
        try_text(&MARIE.replace("name: Marie", "owner: true\nname: Marie")).contains("`owner`")
    );
    // A known category is fine.
    proposals
        .edit_text(
            FILE,
            &at,
            &MARIE.replace("category: people", "category: health"),
        )
        .unwrap();
    assert!(read(dir.path(), FILE).contains("category: health"));

    // Read at an older version: refused.
    assert!(refusal(proposals.edit_text(FILE, &at, MARIE)).contains("changed since you opened it"));
}

#[test]
fn a_copy_made_by_hand_gets_an_id_of_its_own() {
    // Obsidian's "Make a copy" (issue #230).
    let dir = tempfile::tempdir().unwrap();
    let vault = vault(dir.path());
    let copy = "people/marie-durand 1.md";
    std::fs::write(dir.path().join(copy), MARIE).unwrap();

    let found = ipse_core::vault::copies(dir.path()).unwrap();
    assert_eq!(found.len(), 1);
    assert_eq!(
        found[0].path,
        Path::new(copy),
        "the longer name is the copy"
    );
    assert_eq!(found[0].original, Path::new(FILE));
    let report = ipse_core::vault::check(dir.path()).unwrap();
    assert!(
        report
            .problems
            .iter()
            .any(|p| p.path == Path::new(copy) && p.message.contains(FILE)),
        "{report:?}"
    );

    let proposals = Proposals::new(&vault);
    let refused = proposals.own_id(FILE).unwrap_err();
    assert!(
        matches!(refused, Error::Refused(_)),
        "the original keeps its id"
    );
    let id = proposals.own_id(copy).unwrap();
    assert!(read(dir.path(), copy).contains(&format!("\nid: {id}\n")));
    assert!(read(dir.path(), FILE).contains("\nid: 01jcx8m2p4q6r8t0v2w4y6z8a0\n"));
    assert!(ipse_core::vault::copies(dir.path()).unwrap().is_empty());
    let last = &History::open(dir.path()).unwrap().entries(1).unwrap()[0];
    assert_eq!(last.source, Some(Source::Person));
    assert!(matches!(proposals.own_id(copy), Err(Error::Refused(_))));
}

/// Today in Paris, like `2026-11-28`.
fn today() -> String {
    jiff::Timestamp::now()
        .to_zoned(jiff::tz::TimeZone::get("Europe/Paris").unwrap())
        .date()
        .to_string()
}

#[test]
fn a_quick_note_is_a_dated_fact_by_the_person() {
    // Issue #277.
    let dir = tempfile::tempdir().unwrap();
    let vault = vault(dir.path());
    let proposals = Proposals::new(&vault);
    let change = proposals
        .note(FILE, "  Part vivre   à Lyon en mars. ")
        .unwrap()
        .expect("a line was added");
    let after = read(dir.path(), FILE);
    let line = format!("- {} : Part vivre à Lyon en mars. ^f-", today());
    assert!(after.contains(&line), "{after}");
    assert!(after.contains("- Joue du piano ^f-ef56"), "the rest stays");
    let history = vault.history().unwrap();
    let entry = &history.entries(1).unwrap()[0];
    assert_eq!(entry.id, change);
    assert_eq!(entry.source, Some(Source::Person));
    assert_eq!(entry.summary, "Add a note");

    let refused = |text: &str| match proposals.note(FILE, text) {
        Err(Error::Refused(message)) => message,
        other => panic!("{other:?}"),
    };
    assert!(refused("   ").contains("Write something"));
    assert!(refused(&"a".repeat(501)).contains("500"));
}

#[test]
fn a_quick_note_on_an_event_goes_to_its_notes() {
    let dir = tempfile::tempdir().unwrap();
    let vault = vault(dir.path());
    let event = "events/2026/2026-11-14-diner.md";
    std::fs::create_dir_all(dir.path().join("events/2026")).unwrap();
    std::fs::write(
        dir.path().join(event),
        "---\nid: 01jcx8m2p4q6r8t0v2w4y6z8b1\ntype: event\ncategory: agenda\ncreated: 2026-11-12T19:30:00+01:00\nupdated: 2026-11-12T19:30:00+01:00\ntitle: Dîner\nstart: 2026-11-14T20:00:00+01:00\n---\n\n## Notes\n\nApporter le dessert.\n",
    )
    .unwrap();
    History::open(dir.path())
        .unwrap()
        .record_outside_edits()
        .unwrap();
    let vault = Vault::open(vault.root()).unwrap();
    Proposals::new(&vault)
        .note(event, "Marc a adoré le gâteau.")
        .unwrap();
    let after = read(dir.path(), event);
    let expected = format!(
        "## Notes\n\nApporter le dessert.\n\n{} : Marc a adoré le gâteau.\n",
        today()
    );
    assert!(after.contains(&expected), "{after}");
}
