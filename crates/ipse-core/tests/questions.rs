//! Questions about people known only by a first name or a nickname
//! (`docs/spec/vault.md`, section 13): who is asked about, how often, and what an
//! answer changes. All data is invented.

// Test helpers may panic: a failed expectation is a failed test.
#![allow(clippy::expect_used, clippy::unwrap_used)]

use std::path::Path;

use ipse_core::proposals::{Accepted, Author, Change, Proposals};
use ipse_core::rights::{Categories, Clients, Gateway};
use ipse_core::search::SearchIndex;
use ipse_core::tasks::{Task, TaskKind, TaskStatus, Tasks, question};
use ipse_core::time::TimeZone;
use ipse_core::vault::{
    Common, DateTime, EntityFields, FileType, Language, MemoryFile, Settings, Source, Vault, naming,
};
use jiff::civil::{Date, date};

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

/// Saves a person; returns the file, relative to the vault.
fn person(root: &Path, name: &str, aliases: &[&str]) -> String {
    let mut common = Common::new(FileType::Person, &TimeZone::UTC).unwrap();
    let path = naming::entity_path(root, FileType::Person, Some(name), &common.id).unwrap();
    common.sources = vec!["person".into()];
    let entity = EntityFields {
        name: Some(name.into()),
        aliases: aliases.iter().map(|a| (*a).to_owned()).collect(),
        phones: vec!["+33 6 00 00 00 01".into()],
        ..EntityFields::default()
    };
    MemoryFile::new(&path, common, Some(entity), "\n## Faits\n\n- Inventé.\n")
        .write()
        .unwrap();
    path.strip_prefix(root)
        .unwrap()
        .to_string_lossy()
        .replace('\\', "/")
}

fn plan(vault: &Vault, today: Date) -> usize {
    let index = SearchIndex::load(vault.root()).unwrap();
    Tasks::new(vault).plan_questions(&index, today).unwrap()
}

fn questions(vault: &Vault) -> Vec<Task> {
    Tasks::new(vault)
        .list()
        .unwrap()
        .into_iter()
        .filter(|t| t.kind == TaskKind::Question)
        .collect()
}

fn open_files(vault: &Vault) -> Vec<String> {
    let mut files: Vec<String> = questions(vault)
        .into_iter()
        .filter(|t| t.status == TaskStatus::Open)
        .map(|t| t.items[0].file.clone())
        .collect();
    files.sort();
    files
}

#[test]
fn first_names_nicknames_and_family_words_are_asked_about() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    let vault = vault(root);
    let jo = person(root, "Jo plombier", &[]);
    let maman = person(root, "Maman", &[]);
    let lea = person(root, "Léa", &[]);
    person(root, "Marie Dupont", &[]);
    person(root, "Tonton", &["Marc Roux"]);

    assert_eq!(plan(&vault, TODAY), 3);
    let mut expected = vec![jo, maman, lea];
    expected.sort();
    assert_eq!(
        open_files(&vault),
        expected,
        "not a full name, not someone with a full other name, not the owner"
    );
    let task = &questions(&vault)[0];
    assert_eq!(task.categories, ["people"]);

    let detail = Tasks::new(&vault).detail(&task.id).unwrap();
    assert!(detail.expected.contains("in French"), "{}", detail.expected);
    let item = &detail.items[0];
    assert_eq!(item.text, None, "fields, never mixed with Ipse's words");
    let details = item.details.as_ref().unwrap();
    assert_eq!(details["phones"], serde_json::json!(["+33 6 00 00 00 01"]));
    assert_eq!(details["name"], item.name.as_str());
    assert!(detail.example.contains("\"add_relation\""));
    assert!(detail.example.contains("\"plombier\""));
}

#[test]
fn a_few_at_a_time_and_a_few_a_day() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    let vault = vault(root);
    for name in ["Jo", "Lou", "Max", "Noé", "Sam", "Tom", "Zoé"] {
        person(root, name, &[]);
    }
    assert_eq!(plan(&vault, TODAY), question::OPEN);
    assert_eq!(plan(&vault, TODAY), 0, "three are waiting");

    let tasks = Tasks::new(&vault);
    for task in questions(&vault) {
        question::skip(&vault, &task.id).unwrap();
    }
    assert_eq!(plan(&vault, TODAY), 0, "three were made today already");
    assert_eq!(plan(&vault, date(2026, 11, 18)), 3, "a new day");
    assert_eq!(questions(&vault).len(), 6);
    for task in questions(&vault)
        .into_iter()
        .filter(|t| t.status == TaskStatus::Open)
    {
        tasks.dismiss(&task.id).unwrap();
    }
    assert_eq!(plan(&vault, date(2026, 11, 19)), 1, "the last one");
    assert_eq!(
        plan(&vault, date(2026, 11, 20)),
        0,
        "everyone was asked once, whatever the answer"
    );
}

#[test]
fn an_answer_names_the_person_and_says_how_they_are_known() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    let vault = vault(root);
    let jo = person(root, "Jo plombier", &[]);
    plan(&vault, TODAY);
    let task = questions(&vault).remove(0);
    let stem = jo.trim_start_matches("people/").trim_end_matches(".md");

    question::answer(&vault, &task.id, "  Jo Lambert ", Some("plombier")).unwrap();

    let file = MemoryFile::read(root.join(&jo)).unwrap();
    let entity = file.entity().unwrap();
    assert_eq!(entity.name.as_deref(), Some("Jo Lambert"));
    assert_eq!(entity.aliases, ["Jo plombier"], "the old name stays");
    let owner = std::fs::read_to_string(vault.owner_file()).unwrap();
    assert!(
        owner.contains(&format!("- plombier : [[{stem}]]")),
        "{owner}"
    );

    let history = vault.history().unwrap();
    let last = &history.entries(1).unwrap()[0];
    assert_eq!(last.source, Some(Source::Person));
    assert_eq!(last.files.len(), 2, "one change for both files");
    assert_ne!(
        Tasks::new(&vault).get(&task.id).unwrap().status,
        TaskStatus::Open
    );

    assert!(
        question::answer(&vault, &task.id, "Jo Lambert", None).is_err(),
        "answered once"
    );
    assert_eq!(plan(&vault, date(2026, 11, 18)), 0, "named now");
}

#[test]
fn an_answer_needs_a_name() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    let vault = vault(root);
    person(root, "Léa", &[]);
    plan(&vault, TODAY);
    let task = questions(&vault).remove(0);
    assert!(question::answer(&vault, &task.id, "   ", None).is_err());
    assert_eq!(
        Tasks::new(&vault).get(&task.id).unwrap().status,
        TaskStatus::Open
    );
}

#[test]
fn an_ai_s_answer_keeps_the_old_name_too() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    let vault = vault(root);
    let lea = person(root, "Léa", &[]);
    plan(&vault, TODAY);
    let task = questions(&vault).remove(0);

    let proposals = Proposals::new(&vault);
    let proposal = proposals
        .submit(
            Author::Client("claude".into()),
            "Elle me l'a dit.",
            Some(task.id),
            vec![Change::SetKey {
                file: lea.clone(),
                key: "name".into(),
                from: "Léa".into(),
                to: "Léa Martin".into(),
            }],
        )
        .unwrap();
    assert!(matches!(
        proposals.accept(&proposal.id).unwrap(),
        Accepted::Applied(_)
    ));
    let file = MemoryFile::read(root.join(&lea)).unwrap();
    let entity = file.entity().unwrap();
    assert_eq!(entity.name.as_deref(), Some("Léa Martin"));
    assert_eq!(entity.aliases, ["Léa"]);
}

#[test]
fn someone_added_later_in_the_day_is_asked_about() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    let vault = vault(root);
    person(root, "Marie Dupont", &[]);
    assert_eq!(plan(&vault, TODAY), 0, "nobody to ask");
    assert_eq!(plan(&vault, TODAY), 0);
    let zoe = person(root, "Zoé", &[]);
    assert_eq!(plan(&vault, TODAY), 1, "a new file: looked again");
    assert_eq!(open_files(&vault), [zoe]);
}

/// An AI that may propose in `propose`; its code.
fn client(vault: &Vault, name: &str, propose: &[&str]) -> String {
    let categories = Categories::of(vault.settings());
    let mut clients = Clients::load(vault.root()).unwrap();
    let now = DateTime::now(vault.settings().time_zone());
    let (client, token) = clients.add(name, now, &categories, false).unwrap();
    clients
        .set_rights(
            &client.id,
            None,
            Some(propose.iter().map(|s| (*s).to_owned()).collect()),
            &categories,
        )
        .unwrap();
    token.expose().to_owned()
}

#[test]
fn an_ai_links_the_person_from_the_owner_s_file_only_when_allowed_to() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    let vault = vault(root);
    let jo = person(root, "Jo plombier", &[]);
    let people_only = client(&vault, "Claude", &["people"]);
    let trusted = client(&vault, "Local", &["people", "identity"]);
    let gateway = Gateway::open(root).unwrap();

    let ai = gateway.access(Some(&people_only)).unwrap();
    let (open, _) = ai.tasks(Some("question"), 10).unwrap();
    assert_eq!(open.len(), 1, "planned on the AI's call");
    let task = ai.task(&open[0].id).unwrap();
    let kinds: Vec<&str> = task.example["changes"]
        .as_array()
        .unwrap()
        .iter()
        .map(|c| c["kind"].as_str().unwrap())
        .collect();
    assert_eq!(kinds, ["set_key"], "no relation it could not propose");
    assert_eq!(task.example["changes"][0]["file"], jo.as_str());
    let refused = ai.propose(
        Some(&task.id),
        vec![Change::AddRelation {
            file: "people/ines-roux.md".into(),
            word: "plombier".into(),
            target: "jo-plombier".into(),
            display: None,
        }],
        "Il me l'a dit.",
    );
    assert!(
        matches!(&refused, Err(ipse_core::Error::Refused(m)) if m == "There is no file people/ines-roux.md."),
        "the owner's file is in identity: {refused:?}"
    );

    let local = gateway.access(Some(&trusted)).unwrap();
    let task = local.task(&task.id).unwrap();
    let relation = &task.example["changes"][1];
    assert_eq!(relation["kind"], "add_relation");
    assert_eq!(relation["file"], "people/ines-roux.md");
    let stem = jo.trim_start_matches("people/").trim_end_matches(".md");
    assert_eq!(relation["target"], stem);

    let proposed = local
        .propose(
            Some(&task.id),
            vec![
                Change::SetKey {
                    file: jo.clone(),
                    key: "name".into(),
                    from: "Jo plombier".into(),
                    to: "Jo Lambert".into(),
                },
                Change::AddRelation {
                    file: "people/ines-roux.md".into(),
                    word: "plombier".into(),
                    target: stem.into(),
                    display: None,
                },
            ],
            "Il m'a dit que c'est son plombier.",
        )
        .unwrap();
    let id = proposed.proposal.unwrap();
    assert!(matches!(
        Proposals::new(&vault).accept(&id).unwrap(),
        Accepted::Applied(_)
    ));
    let entity = MemoryFile::read(root.join(&jo)).unwrap();
    assert_eq!(entity.entity().unwrap().aliases, ["Jo plombier"]);
    let owner = std::fs::read_to_string(vault.owner_file()).unwrap();
    assert!(
        owner.contains(&format!("- plombier : [[{stem}]]")),
        "{owner}"
    );
}
