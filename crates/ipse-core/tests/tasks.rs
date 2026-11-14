//! Processing tasks: made from notes, answered by proposals (`docs/spec/vault.md`,
//! section 13).

// Test helpers may panic: a failed expectation is a failed test.
#![allow(clippy::expect_used, clippy::unwrap_used)]

use std::path::{Path, PathBuf};

use ipse_core::Error;
use ipse_core::proposals::{Accepted, Author, Change, Proposals};
use ipse_core::tasks::{TaskKind, TaskStatus, Tasks};
use ipse_core::vault::{History, Language, Settings, Vault};

fn person(i: usize, notes: &str) -> String {
    format!(
        "---\nid: 01jcx8m2p4q6r8t0v2w4{i:06}\ntype: person\ncategory: people\ncreated: 2026-01-10T09:00:00+01:00\nupdated: 2026-01-10T09:00:00+01:00\nname: Personne {i}\n---\n\n## Notes\n\n{notes}\n"
    )
}

/// A vault with `n` people whose notes say something; their paths.
fn vault(dir: &Path, n: usize) -> (Vault, Vec<PathBuf>) {
    Vault::init(
        dir,
        "Jules Moreau",
        Settings::new(Language::Fr, "Europe/Paris").unwrap(),
    )
    .unwrap();
    let mut paths = Vec::new();
    for i in 0..n {
        let path = dir.join(format!("people/personne-{i}.md"));
        std::fs::write(
            &path,
            person(i, &format!("Rencontrée au club {i}. Infirmière.")),
        )
        .unwrap();
        paths.push(path);
    }
    History::open(dir).unwrap().record_outside_edits().unwrap();
    (Vault::open(dir).unwrap(), paths)
}

#[test]
fn notes_become_extract_tasks_of_twenty_once() {
    let dir = tempfile::tempdir().unwrap();
    let (vault, mut paths) = vault(dir.path(), 25);
    // Files without notes, or that are not memory, are left out.
    paths.push(dir.path().join("people/jules-moreau.md"));
    paths.push(dir.path().join(".ipse/vault.toml"));
    let tasks = Tasks::new(&vault);
    assert_eq!(tasks.extract_from(&paths).unwrap(), 2);
    let all = tasks.list().unwrap();
    assert_eq!(
        all.iter().map(|t| t.items.len()).collect::<Vec<_>>(),
        [20, 5],
        "oldest first"
    );
    assert!(all.iter().all(|t| t.kind == TaskKind::Extract
        && t.status == TaskStatus::Open
        && t.categories == ["people"]));
    assert_eq!(
        all[0].description,
        "Read the notes of 20 files and propose the facts and relations they hold."
    );
    assert!(!all[0].description.contains("Personne"), "no personal data");

    let detail = tasks.detail(&all[1].id).unwrap();
    assert_eq!(detail.items[0].file, "people/personne-20.md");
    assert_eq!(detail.items[0].name, "Personne 20");
    assert_eq!(detail.items[0].file_type, "person");
    assert_eq!(
        detail.items[0].text.as_deref(),
        Some("Rencontrée au club 20. Infirmière.")
    );
    assert!(detail.expected.contains("in French"), "{}", detail.expected);
    let example: serde_json::Value = serde_json::from_str(&detail.example).unwrap();
    assert_eq!(example["task"], all[1].id.as_str());
    assert_eq!(example["changes"][0]["file"], "people/personne-20.md");

    // The same notes again: nothing new. Changed notes: one new task.
    assert_eq!(tasks.extract_from(&paths).unwrap(), 0);
    std::fs::write(&paths[3], person(3, "A déménagé à Nantes.")).unwrap();
    assert_eq!(tasks.extract_from(&paths).unwrap(), 1);
    let last = tasks.list().unwrap().pop().unwrap();
    assert_eq!(last.items.len(), 1);
    assert_eq!(last.items[0].file, "people/personne-3.md");
}

#[test]
fn a_task_follows_its_proposals() {
    let dir = tempfile::tempdir().unwrap();
    let (vault, paths) = vault(dir.path(), 1);
    let tasks = Tasks::new(&vault);
    tasks.extract_from(&paths).unwrap();
    let id = tasks.list().unwrap()[0].id.clone();
    let proposals = Proposals::new(&vault);
    let fact = |text: &str| {
        vec![Change::AddFact {
            file: "people/personne-0.md".into(),
            text: text.into(),
        }]
    };
    let submit = |text: &str| {
        proposals.submit(
            Author::Client("claude".into()),
            "Lu dans les notes.",
            Some(id.clone()),
            fact(text),
        )
    };
    let status = || tasks.get(&id).unwrap();

    let first = submit("Infirmière.").unwrap();
    assert_eq!(first.task.as_deref(), Some(id.as_str()));
    assert_eq!(
        status().status,
        TaskStatus::Proposed {
            proposal: first.id.clone()
        }
    );
    let Err(Error::Refused(message)) = submit("Autre chose.") else {
        panic!("one proposal at a time")
    };
    assert!(message.contains("already proposed"), "{message}");

    proposals.refuse(&first.id, Some("Trop vague")).unwrap();
    assert_eq!(status().status, TaskStatus::Open);
    assert_eq!(status().comment.as_deref(), Some("Trop vague"));

    let second = submit("Infirmière à Lyon.").unwrap();
    assert!(matches!(
        proposals.accept(&second.id).unwrap(),
        Accepted::Applied(Some(_))
    ));
    assert_eq!(
        status().status,
        TaskStatus::Done {
            proposal: second.id.clone()
        }
    );
    assert_eq!(status().comment, None);
    assert!(matches!(submit("Encore."), Err(Error::Refused(_))), "done");

    proposals.undo(&second.id).unwrap();
    assert_eq!(status().status, TaskStatus::Open);
    assert!(status().comment.unwrap().contains("undid"));

    // A proposal that no longer applies opens the task again, saying why.
    let third = submit("Infirmière de nuit.").unwrap();
    std::fs::remove_file(&paths[0]).unwrap();
    assert!(matches!(
        proposals.accept(&third.id).unwrap(),
        Accepted::Stale(_)
    ));
    assert_eq!(status().status, TaskStatus::Open);
    assert!(status().comment.unwrap().contains("no longer exists"));
}

#[test]
fn an_unknown_task_is_refused() {
    let dir = tempfile::tempdir().unwrap();
    let (vault, _) = vault(dir.path(), 1);
    let result = Proposals::new(&vault).submit(
        Author::Client("claude".into()),
        "Raison.",
        Some("01zzzzzzzzzzzzzzzzzzzzzzzz".into()),
        vec![Change::AddFact {
            file: "people/personne-0.md".into(),
            text: "Fait.".into(),
        }],
    );
    assert!(matches!(result, Err(Error::Refused(m)) if m.contains("There is no task")));
    assert!(Proposals::new(&vault).list().unwrap().is_empty());
}
