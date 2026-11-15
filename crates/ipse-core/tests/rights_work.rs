//! The processing tools of the query layer: tasks and proposals within the `propose`
//! right (`docs/spec/rights.md`, sections 2, 4.2 and 6).

// Test helpers may panic: a failed expectation is a failed test.
#![allow(clippy::expect_used, clippy::unwrap_used)]

use std::path::{Path, PathBuf};

use ipse_core::Error;
use ipse_core::proposals::{Change, Proposals};
use ipse_core::rights::{Categories, Clients, Gateway, MAX_CHANGES};
use ipse_core::tasks::{TaskStatus, Tasks};
use ipse_core::vault::{DateTime, History, Language, Settings, Vault};
use serde_json::json;

fn person(root: &Path, stem: &str, category: &str, i: usize) -> PathBuf {
    let path = root.join(format!("people/{stem}.md"));
    std::fs::write(
        &path,
        format!(
            "---\nid: 01jcx8m2p4q6r8t0v2w4y6z8{i:02}\ntype: person\ncategory: {category}\ncreated: 2026-01-10T09:00:00+01:00\nupdated: 2026-01-10T09:00:00+01:00\nname: {stem}\n---\n\n## Notes\n\nNotes de {stem}.\n"
        ),
    )
    .unwrap();
    path
}

struct Fixture {
    _dir: tempfile::TempDir,
    root: PathBuf,
    /// Reads and proposes in `people`.
    worker: String,
    /// Reads `people`, proposes nowhere.
    reader: String,
    /// Reads and proposes in `people` and `private`.
    trusted: String,
}

fn fixture() -> Fixture {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path().to_owned();
    Vault::init(
        &root,
        "Jules Moreau",
        Settings::new(Language::Fr, "Europe/Paris").unwrap(),
    )
    .unwrap();
    let marie = person(&root, "marie", "people", 1);
    let lea = person(&root, "lea", "health", 2);
    let theo = person(&root, "theo", "studies", 3);
    History::open(&root)
        .unwrap()
        .record_outside_edits()
        .unwrap();
    let vault = Vault::open(&root).unwrap();
    // One task per file, so that each has one category.
    let tasks = Tasks::new(&vault);
    for file in [&marie, &lea, &theo] {
        tasks.extract_from(std::slice::from_ref(file)).unwrap();
    }

    let categories = Categories::of(vault.settings());
    let mut clients = Clients::load(&root).unwrap();
    let now = || DateTime::now(vault.settings().time_zone());
    let mut add = |name: &str, read: Option<&[&str]>, propose: &[&str]| {
        let (client, token) = clients.add(name, now(), &categories, false).unwrap();
        clients
            .set_rights(
                &client.id,
                read.map(|r| r.iter().map(|s| (*s).to_owned()).collect()),
                Some(propose.iter().map(|s| (*s).to_owned()).collect()),
                &categories,
            )
            .unwrap();
        token.expose().to_owned()
    };
    let worker = add("Claude", None, &["people"]);
    let reader = add("Lecteur", None, &[]);
    let trusted = add(
        "Local",
        Some(&["identity", "people", "private"]),
        &["people", "private"],
    );
    Fixture {
        _dir: dir,
        root,
        worker,
        reader,
        trusted,
    }
}

fn add_fact(file: &str) -> Vec<Change> {
    vec![Change::AddFact {
        file: file.into(),
        text: "Infirmière.".into(),
    }]
}

#[test]
fn an_ai_sees_only_the_tasks_of_the_categories_it_proposes_in() {
    let f = fixture();
    let gateway = Gateway::open(&f.root).unwrap();
    let ids: Vec<String> = Tasks::new(&Vault::open(&f.root).unwrap())
        .list()
        .unwrap()
        .into_iter()
        .map(|t| t.id)
        .collect();
    let [marie, lea, theo] = [&ids[0], &ids[1], &ids[2]];

    let worker = gateway.access(Some(&f.worker)).unwrap();
    let (tasks, more) = worker.tasks(None, 10).unwrap();
    assert_eq!(tasks.iter().map(|t| &t.id).collect::<Vec<_>>(), [marie]);
    assert!(!more);
    assert_eq!(tasks[0].items, 1);
    let task = worker.task(marie).unwrap();
    assert_eq!(task.items[0].file, "people/marie.md");
    assert_eq!(task.items[0].text.as_deref(), Some("Notes de marie."));
    // A health task and a task of an undeclared category (private) are hidden, in the
    // words used for a task that does not exist.
    let none = |id: &str| format!("There is no task {id}.");
    for id in [lea, theo] {
        assert!(matches!(worker.task(id), Err(Error::Refused(m)) if m == none(id)));
    }
    assert!(
        matches!(worker.task("01zzzzzzzzzzzzzzzzzzzzzzzz"), Err(Error::Refused(m)) if m == none("01zzzzzzzzzzzzzzzzzzzzzzzz"))
    );
    assert!(matches!(
        worker.tasks(Some("chores"), 10),
        Err(Error::Refused(_))
    ));

    // Reading is not proposing.
    let reader = gateway.access(Some(&f.reader)).unwrap();
    assert!(reader.tasks(None, 10).unwrap().0.is_empty());
    assert!(matches!(reader.task(marie), Err(Error::Refused(m)) if m == none(marie)));

    // `private` opened: the undeclared category is seen as private.
    let trusted = gateway.access(Some(&f.trusted)).unwrap();
    let seen: Vec<String> = trusted
        .tasks(None, 10)
        .unwrap()
        .0
        .into_iter()
        .map(|t| t.id)
        .collect();
    assert_eq!(seen, [marie.clone(), theo.clone()]);
}

#[test]
fn a_change_it_may_not_make_reads_as_a_file_that_does_not_exist() {
    let f = fixture();
    let gateway = Gateway::open(&f.root).unwrap();
    let refusal = |token: &str, changes: Vec<Change>| {
        let access = gateway.access(Some(token)).unwrap();
        match access.propose(None, changes, "Raison.") {
            Err(Error::Refused(message)) => message,
            other => panic!("{other:?}"),
        }
    };
    // The health file exists; the other does not: same answer.
    assert_eq!(
        refusal(&f.worker, add_fact("people/lea.md")),
        "There is no file people/lea.md."
    );
    assert_eq!(
        refusal(&f.worker, add_fact("people/lia.md")),
        "There is no file people/lia.md."
    );
    assert_eq!(
        refusal(&f.reader, add_fact("people/marie.md")),
        "There is no file people/marie.md."
    );
    assert_eq!(
        refusal(&f.worker, add_fact("people/theo.md")),
        "There is no file people/theo.md."
    );
    assert_eq!(
        refusal(
            &f.worker,
            vec![Change::Create {
                file: None,
                file_type: "person".into(),
                category: Some("health".into()),
                keys: [("name".to_owned(), json!("Nouveau"))].into(),
                facts: vec![],
                relations: vec![],
                notes: None,
            }]
        ),
        "You cannot propose new files in this category."
    );
    let many = (0..=MAX_CHANGES)
        .flat_map(|_| add_fact("people/marie.md"))
        .collect();
    assert!(refusal(&f.worker, many).contains("at most 100 changes"));
    assert!(refusal(&f.worker, Vec::new()).contains("at least one change"));
    assert!(
        Proposals::new(&Vault::open(&f.root).unwrap())
            .list()
            .unwrap()
            .is_empty()
    );

    // The trusted AI may: an undeclared category counts as private.
    let access = gateway.access(Some(&f.trusted)).unwrap();
    assert!(
        access
            .propose(None, add_fact("people/theo.md"), "Raison.")
            .is_ok()
    );
}

#[test]
fn propose_then_follow_the_proposal() {
    let f = fixture();
    let gateway = Gateway::open(&f.root).unwrap();
    let vault = Vault::open(&f.root).unwrap();
    let task = Tasks::new(&vault).list().unwrap()[0].id.clone();
    let worker = gateway.access(Some(&f.worker)).unwrap();

    let proposed = worker
        .propose(
            Some(&task),
            add_fact("people/marie.md"),
            "Lu dans ses notes.",
        )
        .unwrap();
    assert_eq!(proposed.status, "waiting");
    let id = proposed.proposal.unwrap();
    let status = worker.proposal(&id).unwrap();
    assert_eq!(
        (status.status, status.task.as_deref()),
        ("waiting", Some(task.as_str()))
    );
    assert!(
        worker.tasks(None, 10).unwrap().0.is_empty(),
        "the task waits for the person"
    );

    // Another AI cannot see it.
    let trusted = gateway.access(Some(&f.trusted)).unwrap();
    assert!(
        matches!(trusted.proposal(&id), Err(Error::Refused(m)) if m == format!("There is no proposal {id}."))
    );

    Proposals::new(&vault)
        .refuse(&id, Some("Pas infirmière : médecin."))
        .unwrap();
    let status = worker.proposal(&id).unwrap();
    assert_eq!(status.status, "refused");
    assert_eq!(status.comment.as_deref(), Some("Pas infirmière : médecin."));
    let (open, _) = worker.tasks(None, 10).unwrap();
    assert_eq!(
        open[0].comment.as_deref(),
        Some("Pas infirmière : médecin.")
    );

    // Nothing worth keeping: the task closes without a proposal.
    let closed = worker
        .propose(Some(&task), Vec::new(), "Rien à garder.")
        .unwrap();
    assert_eq!((closed.proposal, closed.status), (None, "done"));
    assert_eq!(
        Tasks::new(&vault).get(&task).unwrap().status,
        TaskStatus::Done { proposal: None }
    );
}

#[test]
fn the_access_log_has_categories_and_counts_only() {
    let f = fixture();
    let mut gateway = Gateway::open(&f.root).unwrap();
    let access = gateway.fresh_access(Some(&f.worker)).unwrap();
    access
        .propose(
            None,
            add_fact("people/marie.md"),
            "Infirmière, d'après la conversation.",
        )
        .unwrap();
    access.log("propose", 1).unwrap();
    let dir = f.root.join(".ipse/access");
    let file = std::fs::read_dir(&dir)
        .unwrap()
        .next()
        .unwrap()
        .unwrap()
        .path();
    let text = std::fs::read_to_string(file).unwrap();
    let line: serde_json::Value = serde_json::from_str(text.lines().last().unwrap()).unwrap();
    assert_eq!(line["tool"], "propose");
    assert_eq!(line["categories"], json!(["people"]));
    assert_eq!(line["results"], 1);
    assert!(
        !text.contains("Infirmière") && !text.contains("marie"),
        "{text}"
    );
}

#[test]
fn rights_are_set_within_known_categories() {
    let f = fixture();
    let vault = Vault::open(&f.root).unwrap();
    let categories = Categories::of(vault.settings());
    let mut clients = Clients::load(&f.root).unwrap();
    let set = |clients: &mut Clients, read: Option<Vec<&str>>, propose: Vec<&str>| {
        clients.set_rights(
            "claude",
            read.map(|r| r.into_iter().map(str::to_owned).collect()),
            Some(propose.into_iter().map(str::to_owned).collect()),
            &categories,
        )
    };
    assert!(
        matches!(set(&mut clients, None, vec!["health"]), Err(Error::Refused(m)) if m.contains("give `health` to read too"))
    );
    assert!(
        matches!(set(&mut clients, None, vec!["studies"]), Err(Error::Refused(m)) if m.contains("not a category"))
    );
    let client = set(
        &mut clients,
        Some(vec!["people", "health"]),
        vec!["health", "people"],
    )
    .unwrap();
    assert_eq!(client.propose, ["health", "people"]);
    // Saved, and read back.
    let again = Clients::load(&f.root).unwrap();
    assert_eq!(
        again.list().iter().find(|c| c.id == "claude").unwrap().read,
        ["people", "health"]
    );
}

#[test]
fn a_file_moved_to_a_hidden_category_leaves_the_task() {
    let f = fixture();
    let gateway = Gateway::open(&f.root).unwrap();
    let vault = Vault::open(&f.root).unwrap();
    let marie = f.root.join("people/marie.md");
    let paul = person(&f.root, "paul", "people", 4);
    let text = std::fs::read_to_string(&marie).unwrap();
    std::fs::write(
        &marie,
        text.replace("Notes de marie.", "A changé de travail."),
    )
    .unwrap();
    let tasks = Tasks::new(&vault);
    tasks.extract_from(&[marie.clone(), paul]).unwrap();
    let last = tasks.list().unwrap().pop().unwrap();
    assert_eq!(last.items.len(), 2);
    let id = last.id;
    let text = std::fs::read_to_string(&marie).unwrap();
    std::fs::write(&marie, text.replace("category: people", "category: health")).unwrap();

    let task = gateway.access(Some(&f.worker)).unwrap().task(&id).unwrap();
    let files: Vec<&str> = task.items.iter().map(|i| i.file.as_str()).collect();
    assert_eq!(files, ["people/paul.md"]);
    let example = task.example.to_string();
    assert!(!example.contains("marie"), "{example}");
}

#[test]
fn the_owner_reads_every_category_and_is_not_logged() {
    let f = fixture();
    let mut gateway = Gateway::open(&f.root).unwrap();
    let owner = gateway.fresh_owner().unwrap();
    assert_eq!(
        owner.about_me().unwrap().name.as_deref(),
        Some("Jules Moreau")
    );
    let names = |query: &str| -> Vec<String> {
        owner
            .search(query, 10)
            .hits
            .into_iter()
            .map(|h| h.name)
            .collect()
    };
    assert_eq!(names("lea"), ["lea"], "health");
    assert_eq!(names("theo"), ["theo"], "an undeclared category");
    assert!(
        !f.root.join(".ipse/access").exists(),
        "the owner's reads are not logged"
    );
}
