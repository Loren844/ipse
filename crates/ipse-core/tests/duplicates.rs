//! Possible duplicates and merges (decision 0011). All data is invented.

// Test helpers may panic: a failed expectation is a failed test.
#![allow(clippy::expect_used, clippy::unwrap_used)]

use std::path::Path;

use ipse_core::duplicates::{self, NOT_DUPLICATES_FILE};
use ipse_core::proposals::{Accepted, Author, Change, Proposal, Proposals, Status};
use ipse_core::vault::{History, Language, Settings, Source, Vault};

fn vault(root: &Path) -> Vault {
    Vault::init(
        root,
        "Inès Roux",
        Settings::new(Language::Fr, "Europe/Paris").unwrap(),
    )
    .unwrap();
    Vault::open(root).unwrap()
}

/// A person file, created on `day` of November 2026 (the older is kept).
fn person(root: &Path, stem: &str, name: &str, day: u8, keys: &str, body: &str) {
    let text = format!(
        "---\nid: 01jcx8m2p4q6r8t0v2w4y6z8{day:02}\ntype: person\ncategory: people\ncreated: 2026-11-{day:02}T09:00:00+01:00\nupdated: 2026-11-{day:02}T09:00:00+01:00\nsources:\n  - contacts:{stem}\nname: {name}\n{keys}---\n{body}"
    );
    std::fs::write(root.join(format!("people/{stem}.md")), text).unwrap();
}

fn waiting(vault: &Vault) -> Vec<Proposal> {
    Proposals::new(vault)
        .list()
        .unwrap()
        .into_iter()
        .filter(|p| p.status == Status::Waiting)
        .collect()
}

fn merges(proposal: &Proposal) -> Vec<(&str, &str, Vec<&str>)> {
    proposal
        .changes
        .iter()
        .filter_map(|c| match c {
            Change::Merge {
                keep,
                remove,
                clues,
            } => Some((
                keep.as_str(),
                remove.as_str(),
                clues.iter().map(String::as_str).collect(),
            )),
            _ => None,
        })
        .collect()
}

#[test]
fn clues_make_proposals_a_name_alone_does_not() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    let vault = vault(root);
    let email = |e: &str| format!("emails:\n  - {e}\n");
    let phone = |p: &str| format!("phones:\n  - \"{p}\"\n");
    person(
        root,
        "marie-dupont",
        "Marie Dupont",
        1,
        &email("marie@example.org"),
        "",
    );
    person(
        root,
        "marie-d",
        "Marie D.",
        2,
        &email("Marie@Example.org"),
        "",
    );
    person(
        root,
        "jo-plombier",
        "Jo plombier",
        3,
        &phone("+33 6 12 34 56 78"),
        "",
    );
    person(root, "jo", "Jo", 4, &phone("06.12.34.56.78"), "");
    person(root, "leo-martin", "Léo Martin", 5, "", "");
    person(root, "leo-martin-2", "Léo Martin", 6, "", "");
    person(
        root,
        "paul-roux",
        "Paul Roux",
        7,
        &phone("+33700000001"),
        "",
    );
    person(
        root,
        "paul-roux-2",
        "Paul Roux",
        8,
        &phone("0700000001"),
        "",
    );

    assert_eq!(duplicates::propose(&vault).unwrap(), 3);
    let found = waiting(&vault);
    let mut all: Vec<_> = found.iter().flat_map(merges).collect();
    all.sort();
    assert_eq!(
        all,
        [
            ("people/jo-plombier.md", "people/jo.md", vec!["phone"]),
            ("people/marie-dupont.md", "people/marie-d.md", vec!["email"]),
            (
                "people/paul-roux.md",
                "people/paul-roux-2.md",
                vec!["phone", "name"]
            ),
        ],
        "same name alone (Léo Martin) is not proposed"
    );
    for proposal in &found {
        assert_eq!(proposal.by, Author::Ipse);
    }
    let find = |keep: &str| {
        found
            .iter()
            .find(|p| merges(p)[0].0 == keep)
            .unwrap()
            .reason
            .clone()
    };
    assert_eq!(
        find("people/marie-dupont.md"),
        "Même adresse email : très probablement la même personne."
    );
    assert!(
        find("people/paul-roux.md")
            .starts_with("Même numéro de téléphone, et même nom : probablement")
    );

    assert_eq!(duplicates::propose(&vault).unwrap(), 0, "already waiting");
}

#[test]
fn two_people_is_remembered() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    let vault = vault(root);
    person(
        root,
        "lea",
        "Léa",
        1,
        "phones:\n  - \"+33 4 78 00 00 00\"\n",
        "",
    );
    person(
        root,
        "luc",
        "Luc",
        2,
        "phones:\n  - \"04 78 00 00 00\"\n",
        "",
    );
    assert_eq!(duplicates::propose(&vault).unwrap(), 1);
    let proposal = waiting(&vault).remove(0);
    Proposals::new(&vault)
        .refuse(&proposal.id, Some("La ligne fixe de la famille."))
        .unwrap();
    let remembered = std::fs::read_to_string(root.join(NOT_DUPLICATES_FILE)).unwrap();
    assert!(
        remembered.contains("[\"01jcx8m2p4q6r8t0v2w4y6z801\", \"01jcx8m2p4q6r8t0v2w4y6z802\"]"),
        "{remembered}"
    );
    assert_eq!(duplicates::propose(&vault).unwrap(), 0);
    assert_eq!(duplicates::propose(&vault).unwrap(), 0, "across runs");
    assert!(root.join("people/lea.md").is_file() && root.join("people/luc.md").is_file());
}

/// Marie twice (her own file and "Maman"), and Paul linking to "Maman".
fn marie_maman_paul(root: &Path) {
    person(
        root,
        "marie-dupont",
        "Marie Dupont",
        1,
        "emails:\n  - marie@example.org\nphones:\n  - \"+33612345678\"\n",
        "\n## Faits\n\n- Infirmière à Lyon. ^f-ab12\n\n## Notes\n\nRencontrée au club.\n",
    );
    person(
        root,
        "maman",
        "Maman",
        2,
        "emails:\n  - MARIE@example.org\n  - marie.d@example.net\nphones:\n  - \"06 12 34 56 78\"\nbirthday: 1960-04-02\n",
        "\n## Faits\n\n- Infirmière à Lyon. ^f-cd34\n- Aime le jazz. ^f-ef56\n\n## Relations\n\n- fils : [[paul-roux]]\n\n## Notes\n\nAppelle le dimanche.\n",
    );
    person(
        root,
        "paul-roux",
        "Paul Roux",
        3,
        "",
        "\n## Relations\n\n- mère : [[maman]]\n- amie : [[Maman|Mamounette]]\n",
    );
}

#[test]
fn a_merge_keeps_one_file_redirects_links_and_can_be_undone() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    let vault = vault(root);
    marie_maman_paul(root);
    History::open(root).unwrap().record_outside_edits().unwrap();
    let read =
        |stem: &str| std::fs::read_to_string(root.join(format!("people/{stem}.md"))).unwrap();
    let (marie, maman, paul) = (read("marie-dupont"), read("maman"), read("paul-roux"));

    duplicates::propose(&vault).unwrap();
    let proposal = waiting(&vault).remove(0);
    let proposals = Proposals::new(&vault);
    let preview = proposals.preview(&proposal).unwrap();
    assert_eq!(preview.files.len(), 1, "the kept file, which says the rest");
    let shown: Vec<String> = preview.files[0]
        .items
        .iter()
        .map(|i| format!("{:?} {:?} {:?}", i.what, i.before, i.after))
        .collect();
    assert!(
        shown.contains(&"Merged Some(\"Maman\") Some(\"Marie Dupont\")".to_owned()),
        "{shown:?}"
    );
    assert!(
        shown.contains(&"Links None Some(\"1\")".to_owned()),
        "{shown:?}"
    );

    let Accepted::Applied(Some(change)) = proposals.accept(&proposal.id).unwrap() else {
        panic!("applied")
    };
    let merged = read("marie-dupont");
    assert!(
        !root.join("people/maman.md").exists(),
        "the other is removed"
    );
    assert!(merged.contains("aliases:\n  - Maman\n"), "{merged}");
    assert!(merged.contains("  - marie.d@example.net\n"), "{merged}");
    assert!(
        !merged.contains("MARIE@example.org"),
        "an email is not repeated"
    );
    assert!(
        !merged.contains("06 12 34 56 78"),
        "a phone is not repeated"
    );
    assert!(merged.contains("birthday: 1960-04-02"), "{merged}");
    assert!(merged.contains("  - contacts:maman\n"), "sources");
    assert_eq!(
        merged.matches("Infirmière à Lyon.").count(),
        1,
        "a fact is not repeated"
    );
    assert!(merged.contains("- Aime le jazz. ^f-"));
    assert!(merged.contains("- fils : [[paul-roux]]"));
    assert!(
        merged.contains("Rencontrée au club.\n\nAppelle le dimanche.\n"),
        "{merged}"
    );
    let linked = read("paul-roux");
    assert!(
        linked.contains("- mère : [[marie-dupont]]\n- amie : [[marie-dupont|Mamounette]]\n"),
        "{linked}"
    );

    let history = History::open(root).unwrap();
    let last = &history.entries(1).unwrap()[0];
    assert_eq!(last.id, change);
    let mut files = last.files.clone();
    files.sort();
    assert_eq!(
        files,
        [
            "people/maman.md",
            "people/marie-dupont.md",
            "people/paul-roux.md"
        ],
        "one change"
    );

    proposals.undo(&proposal.id).unwrap();
    assert_eq!(read("marie-dupont"), marie);
    assert_eq!(read("maman"), maman, "both files back, byte for byte");
    assert_eq!(read("paul-roux"), paul, "links back");
    assert_eq!(history.entries(1).unwrap()[0].source, Some(Source::Person));
}

#[test]
fn the_owner_s_file_is_never_merged_into_another() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    let vault = vault(root);
    // The owner's file is younger than the other: it is kept anyway.
    let owner = std::fs::read_to_string(root.join("people/ines-roux.md")).unwrap();
    let owner = owner.replace(
        "owner: true\n",
        "owner: true\nemails:\n  - ines@example.org\n",
    );
    std::fs::write(root.join("people/ines-roux.md"), owner).unwrap();
    person(root, "moi", "Moi", 1, "emails:\n  - ines@example.org\n", "");
    duplicates::propose(&vault).unwrap();
    let proposal = waiting(&vault).remove(0);
    assert_eq!(merges(&proposal)[0].0, "people/ines-roux.md");

    let refused = Proposals::new(&vault).submit(
        Author::Client("claude".into()),
        "Fusion.",
        None,
        vec![Change::Merge {
            keep: "people/moi.md".into(),
            remove: "people/ines-roux.md".into(),
            clues: Vec::new(),
        }],
    );
    assert!(refused.is_err());
}

#[test]
fn events_copied_between_two_calendars_are_proposed_together() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    let vault = vault(root);
    std::fs::create_dir_all(root.join("events/2026")).unwrap();
    let event = |stem: &str, title: &str, start: &str, source: &str, day: u8| {
        let text = format!(
            "---\nid: 01jcx8m2p4q6r8t0v2w5y6z8{day:02}\ntype: event\ncategory: agenda\ncreated: 2026-11-{day:02}T09:00:00+01:00\nupdated: 2026-11-{day:02}T09:00:00+01:00\nsources:\n  - {source}:{stem}\ntitle: {title}\nstart: {start}\n---\n"
        );
        std::fs::write(root.join(format!("events/2026/{stem}.md")), text).unwrap();
    };
    event(
        "dentiste",
        "Dentiste",
        "2026-12-01T09:00:00+01:00",
        "calendar",
        1,
    );
    event(
        "dentiste-2",
        "Dentiste",
        "2026-12-01T09:00:00+01:00",
        "calendar-2",
        2,
    );
    event(
        "reunion",
        "Réunion",
        "2026-12-02T14:00:00+01:00",
        "calendar",
        3,
    );
    event(
        "reunion-2",
        "Réunion",
        "2026-12-02T14:00:00+01:00",
        "calendar-2",
        4,
    );
    event("yoga", "Yoga", "2026-12-03T18:30:00+01:00", "calendar", 5);
    event(
        "yoga-2",
        "Yoga",
        "2026-12-10T18:30:00+01:00",
        "calendar-2",
        6,
    );
    event("cours", "Cours", "2026-12-04T10:00:00+01:00", "calendar", 7);
    event(
        "cours-2",
        "Cours",
        "2026-12-04T10:00:00+01:00",
        "calendar",
        8,
    );

    assert_eq!(
        duplicates::propose(&vault).unwrap(),
        1,
        "one per pair of calendars"
    );
    let proposal = waiting(&vault).remove(0);
    let found: Vec<(&str, &str)> = merges(&proposal)
        .into_iter()
        .map(|(k, r, _)| (k, r))
        .collect();
    assert_eq!(
        found,
        [
            ("events/2026/dentiste.md", "events/2026/dentiste-2.md"),
            ("events/2026/reunion.md", "events/2026/reunion-2.md"),
        ],
        "not a weekly class, not two alike in one calendar"
    );
    assert!(
        proposal
            .reason
            .starts_with("2 événements semblent copiés entre « calendar » et « calendar-2 »")
    );
    Proposals::new(&vault).accept(&proposal.id).unwrap();
    assert!(!root.join("events/2026/dentiste-2.md").exists());
    let kept = std::fs::read_to_string(root.join("events/2026/dentiste.md")).unwrap();
    assert!(
        kept.contains("  - calendar:dentiste\n  - calendar-2:dentiste-2\n"),
        "{kept}"
    );
}
