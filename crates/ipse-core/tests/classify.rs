//! Event kinds (issue #131): the `kind` key of an event, the `classify` tasks that ask
//! the person's AI for it, and the proposals that answer them. All data is invented.

// Test helpers may panic: a failed expectation is a failed test.
#![allow(clippy::expect_used, clippy::unwrap_used)]

use std::path::Path;

use ipse_core::Error;
use ipse_core::proposals::{Accepted, Author, Change, Proposals};
use ipse_core::search::SearchIndex;
use ipse_core::tasks::{TaskKind, TaskStatus, Tasks, classify};
use ipse_core::vault::{EventKind, Language, MemoryFile, Settings, Vault};
use jiff::civil::{Date, date};
use serde_json::{Value, json};

const TODAY: Date = date(2026, 11, 17);

fn vault(root: &Path) -> Vault {
    Vault::init(
        root,
        "Inès Roux",
        Settings::new(Language::Fr, "Europe/Paris").unwrap(),
    )
    .unwrap();
    Vault::open(root).unwrap()
}

/// Writes an event `days` days from today; returns its file.
fn event(
    root: &Path,
    i: usize,
    title: &str,
    days: i32,
    category: &str,
    kind: Option<&str>,
) -> String {
    let day = TODAY.checked_add(jiff::Span::new().days(days)).unwrap();
    let rel = format!("events/{}/{day}-e{i}.md", day.year());
    let path = root.join(&rel);
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    let kind = kind.map(|k| format!("kind: {k}\n")).unwrap_or_default();
    std::fs::write(
        &path,
        format!(
            "---\nid: 01jcx8m2p4q6r8t0v2w4{i:06}\ntype: event\ncategory: {category}\ncreated: 2026-01-10T09:00:00+01:00\nupdated: 2026-01-10T09:00:00+01:00\ntitle: {title}\nstart: {day}T12:30:00+01:00\nplace: Le Petit Zinc\nparticipants:\n  - Marie\n{kind}---\n\n## Notes\n\nRéserver une table.\n"
        ),
    )
    .unwrap();
    rel
}

fn plan(vault: &Vault, today: Date) -> usize {
    let index = SearchIndex::load(vault.root()).unwrap();
    Tasks::new(vault).plan_classify(&index, today).unwrap()
}

fn classify_tasks(vault: &Vault) -> Vec<ipse_core::tasks::Task> {
    Tasks::new(vault)
        .list()
        .unwrap()
        .into_iter()
        .filter(|t| t.kind == TaskKind::Classify)
        .collect()
}

#[test]
fn an_event_keeps_its_kind_and_refuses_an_unknown_one() {
    let dir = tempfile::tempdir().unwrap();
    vault(dir.path());
    let rel = event(dir.path(), 1, "Déjeuner", -1, "agenda", Some("meal"));
    let path = dir.path().join(&rel);
    let mut file = MemoryFile::read(&path).unwrap();
    assert_eq!(file.event().unwrap().kind, Some(EventKind::Meal));
    file.event_mut().unwrap().kind = Some(EventKind::Outing);
    file.write().unwrap();
    assert!(
        std::fs::read_to_string(&path)
            .unwrap()
            .contains("kind: outing\n")
    );

    std::fs::write(
        &path,
        std::fs::read_to_string(&path)
            .unwrap()
            .replace("kind: outing", "kind: repas"),
    )
    .unwrap();
    let Err(Error::Invalid { problem, .. }) = MemoryFile::read(&path) else {
        panic!("read")
    };
    assert!(
        problem.contains("`kind` must be one of: meal, celebration"),
        "{problem}"
    );
    assert_eq!(EventKind::ALL.len(), 12);
    for kind in EventKind::ALL {
        assert_eq!(EventKind::parse(kind.as_str()), Some(kind));
    }
}

#[test]
fn events_without_a_kind_are_asked_about_a_few_batches_a_day() {
    let dir = tempfile::tempdir().unwrap();
    let vault = vault(dir.path());
    // 70 agenda events around today, 5 in health, 1 with a kind already.
    for i in 0..70 {
        let n = i32::try_from(i).unwrap();
        let days = if i % 2 == 0 { -n } else { n * 3 };
        event(
            dir.path(),
            i,
            &format!("Déjeuner {i}"),
            days,
            "agenda",
            None,
        );
    }
    for i in 70..75 {
        event(dir.path(), i, "Dentiste", -400, "health", None);
    }
    event(dir.path(), 99, "Dîner", 0, "agenda", Some("meal"));

    assert_eq!(plan(&vault, TODAY), classify::OPEN);
    let tasks = classify_tasks(&vault);
    assert_eq!(tasks.len(), 2);
    for task in &tasks {
        assert_eq!(task.categories.len(), 1, "one category a task");
        assert!(task.items.len() <= classify::PER_TASK);
    }
    assert_eq!(tasks[0].categories, ["agenda"]);
    assert_eq!(tasks[0].items.len(), classify::PER_TASK);
    assert!(
        tasks[0].items[0].file.contains(&TODAY.to_string()),
        "nearest today first: {}",
        tasks[0].items[0].file
    );
    let all: Vec<&str> = tasks
        .iter()
        .flat_map(|t| &t.items)
        .map(|i| i.file.as_str())
        .collect();
    assert!(
        !all.iter().any(|f| f.ends_with("-e99.md")),
        "a kind already"
    );
    assert_eq!(plan(&vault, TODAY), 0, "two waiting at most");

    // One done: one more today, then the day's limit.
    Tasks::new(&vault).dismiss(&tasks[0].id).unwrap();
    assert_eq!(plan(&vault, TODAY), 1);
    Tasks::new(&vault)
        .dismiss(&classify_tasks(&vault)[1].id)
        .unwrap();
    assert_eq!(plan(&vault, TODAY), 0, "three a day");
    let tomorrow = TODAY.tomorrow().unwrap();
    assert_eq!(plan(&vault, tomorrow), 1);

    // Every event is asked about once.
    let mut asked: Vec<String> = classify_tasks(&vault)
        .iter()
        .flat_map(|t| t.items.iter().map(|i| i.file.clone()))
        .collect();
    let count = asked.len();
    asked.sort();
    asked.dedup();
    assert_eq!(asked.len(), count);
}

#[test]
fn a_task_gives_the_fields_and_is_answered_with_a_kind() {
    let dir = tempfile::tempdir().unwrap();
    let vault = vault(dir.path());
    let lunch = event(dir.path(), 1, "Déjeuner avec Marie", -1, "agenda", None);
    let other = event(dir.path(), 2, "Concert", 3, "agenda", None);
    assert_eq!(plan(&vault, TODAY), 1);
    let task = classify_tasks(&vault).remove(0);
    let tasks = Tasks::new(&vault);
    let detail = tasks.detail(&task.id).unwrap();
    assert!(detail.expected.contains("`meal` (lunch, dinner, a coffee)"));
    assert!(detail.example.contains("\"key\": \"kind\""));
    let item = detail.items.iter().find(|i| i.file == lunch).unwrap();
    assert_eq!(
        item.text, None,
        "fields only, never mixed with Ipse's words"
    );
    let details = item.details.as_ref().unwrap();
    assert_eq!(details["title"], "Déjeuner avec Marie");
    assert_eq!(details["place"], "Le Petit Zinc");
    assert_eq!(details["with"], json!(["Marie"]));
    assert_eq!(details["notes"], "Réserver une table.");

    let proposals = Proposals::new(&vault);
    let wrong = proposals.submit(
        Author::Client("claude".into()),
        "D'après les titres.",
        Some(task.id.clone()),
        vec![Change::SetKey {
            file: lunch.clone(),
            key: "kind".into(),
            from: Value::Null,
            to: json!("repas"),
        }],
    );
    let Err(Error::Refused(message)) = wrong else {
        panic!("accepted an unknown kind")
    };
    assert!(message.contains("meal, celebration"), "{message}");

    let proposal = proposals
        .submit(
            Author::Client("claude".into()),
            "D'après les titres.",
            Some(task.id.clone()),
            vec![
                Change::SetKey {
                    file: lunch.clone(),
                    key: "kind".into(),
                    from: Value::Null,
                    to: json!("meal"),
                },
                Change::SetKey {
                    file: other.clone(),
                    key: "kind".into(),
                    from: Value::Null,
                    to: json!("culture"),
                },
            ],
        )
        .unwrap();
    assert!(matches!(
        proposals.accept(&proposal.id).unwrap(),
        Accepted::Applied(Some(_))
    ));
    let read = |rel: &str| MemoryFile::read(dir.path().join(rel)).unwrap();
    assert_eq!(read(&lunch).event().unwrap().kind, Some(EventKind::Meal));
    assert_eq!(read(&other).event().unwrap().kind, Some(EventKind::Culture));
    assert!(matches!(
        tasks.get(&task.id).unwrap().status,
        TaskStatus::Done { .. }
    ));

    // Given a kind, an event is not asked about again, and leaves the task's items.
    assert_eq!(plan(&vault, TODAY.tomorrow().unwrap()), 0);
    assert!(tasks.detail(&task.id).unwrap().items.is_empty());
}
