//! Summaries written by the person's AI (`docs/spec/vault.md`, section 7): day tasks
//! from events, longer periods once the shorter summaries exist, the file accepted,
//! the person's edits protected. All data is invented.

// Test helpers may panic: a failed expectation is a failed test.
#![allow(clippy::expect_used, clippy::unwrap_used)]

use std::path::Path;

use ipse_core::proposals::{Accepted, Author, Change, Proposals};
use ipse_core::search::SearchIndex;
use ipse_core::tasks::{Task, TaskKind, TaskStatus, Tasks};
use ipse_core::time::TimeZone;
use ipse_core::vault::{
    Common, EntityFields, EventFields, EventTime, FileType, Language, LinkOrText, MemoryFile,
    Settings, Vault, naming,
};
use jiff::civil::{Date, date};

/// A Tuesday: the week of Monday 9 to Sunday 15 November has ended.
const TODAY: Date = date(2026, 11, 17);

fn vault(root: &Path) -> Vault {
    Vault::init(
        root,
        "Inès Roux",
        Settings::new(Language::Fr, "Europe/Paris").unwrap(),
    )
    .unwrap();
    let mut common = Common::new(FileType::Person, &TimeZone::UTC).unwrap();
    let path =
        naming::entity_path(root, FileType::Person, Some("Julie Dupont"), &common.id).unwrap();
    common.sources = vec!["person".into()];
    let entity = EntityFields {
        name: Some("Julie Dupont".into()),
        ..EntityFields::default()
    };
    MemoryFile::new(
        &path,
        common,
        Some(entity),
        "\n## Faits\n\n- Amie d'enfance.\n",
    )
    .write()
    .unwrap();
    Vault::open(root).unwrap()
}

fn event(root: &Path, title: &str, start: &str) -> String {
    let common = Common::new(FileType::Event, &TimeZone::UTC).unwrap();
    let start = EventTime::parse(start).unwrap();
    let mut fields = EventFields::new(title, start);
    fields.participants = vec![LinkOrText::parse("[[julie-dupont]]")];
    let path = naming::event_path(root, title, &start, &common.id).unwrap();
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    MemoryFile::new_event(&path, common, fields, "\n## Notes\n\nApporter du pain.\n")
        .write()
        .unwrap();
    path.strip_prefix(root)
        .unwrap()
        .to_string_lossy()
        .replace('\\', "/")
}

fn plan(vault: &Vault, today: Date) -> usize {
    let index = SearchIndex::load(vault.root()).unwrap();
    Tasks::new(vault).plan_summaries(&index, today).unwrap()
}

fn summary_tasks(vault: &Vault) -> Vec<Task> {
    Tasks::new(vault)
        .list()
        .unwrap()
        .into_iter()
        .filter(|t| t.kind == TaskKind::Summary)
        .collect()
}

fn write(period: &str, start: &str, text: &str) -> Change {
    Change::WriteSummary {
        file: None,
        period: period.into(),
        start: start.into(),
        from: None,
        text: text.into(),
    }
}

/// Answers the task of `period` with `text`, and accepts it.
fn answer(vault: &Vault, key: &str, text: &str) {
    let task = summary_tasks(vault)
        .into_iter()
        .find(|t| t.period.as_deref() == Some(key))
        .unwrap_or_else(|| panic!("a task for {key}"));
    let (period, start) = key.split_once(':').unwrap();
    let proposals = Proposals::new(vault);
    let proposal = proposals
        .submit(
            Author::Client("claude".into()),
            "Résumé.",
            Some(task.id),
            vec![write(period, start, text)],
        )
        .unwrap();
    assert!(matches!(
        proposals.accept(&proposal.id).unwrap(),
        Accepted::Applied(_)
    ));
}

#[test]
fn a_task_for_each_past_day_with_events() {
    let dir = tempfile::tempdir().unwrap();
    let vault = vault(dir.path());
    let dinner = event(dir.path(), "Dîner chez Julie", "2026-11-14T19:30:00+01:00");
    event(dir.path(), "Yoga", "2026-11-14T08:00:00+01:00");
    event(dir.path(), "Cinéma", "2026-11-10T20:00:00+01:00");
    event(dir.path(), "Trop vieux", "2026-09-01T20:00:00+02:00");
    event(dir.path(), "Aujourd'hui", "2026-11-17T09:00:00+01:00");

    assert_eq!(plan(&vault, TODAY), 2, "the 10th and the 14th, not today");
    let tasks = summary_tasks(&vault);
    let periods: Vec<&str> = tasks.iter().filter_map(|t| t.period.as_deref()).collect();
    assert_eq!(
        periods,
        ["day:2026-11-10", "day:2026-11-14"],
        "oldest first"
    );
    let fourteenth = &tasks[1];
    assert_eq!(
        fourteenth.description,
        "Summarize the day 2026-11-14 from its 2 events."
    );
    assert_eq!(fourteenth.categories, ["agenda", "people", "summaries"]);
    let files: Vec<&str> = fourteenth.items.iter().map(|i| i.file.as_str()).collect();
    assert!(files.contains(&dinner.as_str()));
    assert!(
        files.contains(&"people/julie-dupont.md"),
        "the people it links to"
    );

    let detail = Tasks::new(&vault).detail(&fourteenth.id).unwrap();
    assert!(detail.expected.contains("in French"), "{}", detail.expected);
    assert!(detail.expected.contains("2 à 4 phrases"));
    let event_text = detail.items.iter().find(|i| i.file == dinner).unwrap();
    let text = event_text.text.as_deref().unwrap();
    assert!(
        text.contains("Title: Dîner chez Julie") && text.contains("With: julie-dupont"),
        "{text}"
    );
    assert!(text.contains("Apporter du pain."));
    let julie = detail
        .items
        .iter()
        .find(|i| i.file == "people/julie-dupont.md")
        .unwrap();
    assert_eq!(julie.text.as_deref(), Some("Amie d'enfance."));
    assert!(
        detail.example.contains("\"kind\": \"write_summary\""),
        "{}",
        detail.example
    );
    assert!(detail.example.contains("\"start\": \"2026-11-14\""));

    assert_eq!(
        plan(&vault, TODAY),
        0,
        "nothing changed: nothing asked again"
    );
    assert_eq!(
        plan(&vault, date(2026, 11, 18)),
        1,
        "the 17th has ended: one more a day"
    );
    assert_eq!(summary_tasks(&vault).len(), 3);
}

#[test]
fn a_week_once_its_days_are_summarized() {
    let dir = tempfile::tempdir().unwrap();
    let vault = vault(dir.path());
    for day in [9, 11, 14] {
        event(
            dir.path(),
            "Yoga",
            &format!("2026-11-{day:02}T08:00:00+01:00"),
        );
    }
    assert_eq!(plan(&vault, TODAY), 3);
    answer(&vault, "day:2026-11-09", "Yoga le matin.");
    answer(&vault, "day:2026-11-11", "Yoga, puis le marché.");
    assert_eq!(plan(&vault, TODAY), 0, "the 14th is not summarized yet");
    answer(&vault, "day:2026-11-14", "Yoga avec Julie.");
    assert_eq!(plan(&vault, TODAY), 1, "the week, from its days");
    let week = summary_tasks(&vault)
        .into_iter()
        .find(|t| t.period.as_deref() == Some("week:2026-11-09"))
        .unwrap();
    assert_eq!(
        week.description,
        "Summarize the week 2026-W46 from its 3 day summaries."
    );
    let files: Vec<&str> = week.items.iter().map(|i| i.file.as_str()).collect();
    assert_eq!(
        files,
        [
            "summaries/day/2026/2026-11-09.md",
            "summaries/day/2026/2026-11-11.md",
            "summaries/day/2026/2026-11-14.md"
        ],
        "built from the day summaries only"
    );
    let detail = Tasks::new(&vault).detail(&week.id).unwrap();
    assert_eq!(detail.items[2].text.as_deref(), Some("Yoga avec Julie."));
    assert!(detail.expected.contains("summaries of its days only"));
    assert_eq!(plan(&vault, TODAY), 0, "the month has not ended");
}

#[test]
fn an_accepted_summary_is_a_file_with_its_events() {
    let dir = tempfile::tempdir().unwrap();
    let vault = vault(dir.path());
    let dinner = event(dir.path(), "Dîner chez Julie", "2026-11-14T19:30:00+01:00");
    plan(&vault, TODAY);
    answer(
        &vault,
        "day:2026-11-14",
        "Dîner chez Julie, qui part au Japon en mars.",
    );

    let path = dir.path().join("summaries/day/2026/2026-11-14.md");
    let text = std::fs::read_to_string(&path).unwrap();
    let stem = dinner.rsplit('/').next().unwrap().trim_end_matches(".md");
    assert!(
        text.contains("\ntype: summary\ncategory: summaries\n"),
        "{text}"
    );
    assert!(text.contains("sources:\n  - proposal:"), "{text}");
    assert!(
        text.contains("\nperiod: day\nstart: 2026-11-14\nend: 2026-11-15\n---\n"),
        "{text}"
    );
    assert!(
        text.ends_with(&format!(
            "\n## Résumé\n\nDîner chez Julie, qui part au Japon en mars.\n\n## Événements\n\n- [[{stem}]]\n"
        )),
        "{text}"
    );
    let file = MemoryFile::read(&path).unwrap();
    assert_eq!(file.common().file_type, FileType::Summary);
    assert_eq!(file.common().category.as_str(), "summaries");

    // Searchable, in its category.
    let index = SearchIndex::load(dir.path()).unwrap();
    let hits = index.search("japon", 10, |c| c.as_str() == "summaries");
    assert_eq!(hits.len(), 1);
    assert!(
        index
            .search("japon", 10, |c| c.as_str() == "agenda")
            .is_empty()
    );

    let task = summary_tasks(&vault).remove(0);
    assert!(matches!(
        task.status,
        TaskStatus::Done { proposal: Some(_) }
    ));
}

#[test]
fn the_person_s_edit_is_never_written_over() {
    let dir = tempfile::tempdir().unwrap();
    let vault = vault(dir.path());
    event(dir.path(), "Dîner chez Julie", "2026-11-14T19:30:00+01:00");
    plan(&vault, TODAY);
    answer(&vault, "day:2026-11-14", "Dîner chez Julie.");
    let path = dir.path().join("summaries/day/2026/2026-11-14.md");

    // A proposal made, then the person rewrites the summary: it no longer applies.
    let proposals = Proposals::new(&vault);
    let late = proposals
        .submit(
            Author::Client("claude".into()),
            "Plus court.",
            None,
            vec![write("day", "2026-11-14", "Dîner.")],
        )
        .unwrap();
    let text = std::fs::read_to_string(&path).unwrap();
    std::fs::write(
        &path,
        text.replace("Dîner chez Julie.", "Dîner chez Julie, très drôle."),
    )
    .unwrap();
    assert!(matches!(
        proposals.accept(&late.id).unwrap(),
        Accepted::Stale(_)
    ));
    assert!(
        std::fs::read_to_string(&path)
            .unwrap()
            .contains("très drôle")
    );

    // A new event that day: asked again, with the person's text to keep.
    event(dir.path(), "Yoga", "2026-11-14T08:00:00+01:00");
    plan(&vault, TODAY);
    let update = summary_tasks(&vault)
        .into_iter()
        .find(|t| t.status == TaskStatus::Open && t.period.as_deref() == Some("day:2026-11-14"))
        .unwrap();
    let detail = Tasks::new(&vault).detail(&update.id).unwrap();
    let current = detail
        .items
        .iter()
        .find(|i| i.file == "summaries/day/2026/2026-11-14.md")
        .unwrap();
    assert_eq!(
        current.text.as_deref(),
        Some("Dîner chez Julie, très drôle.")
    );
    assert!(detail.expected.contains("keep their words"));

    // The update waits for the person, showing their text and the new one.
    let proposal = proposals
        .submit(
            Author::Client("claude".into()),
            "Ajout du yoga.",
            Some(update.id),
            vec![write(
                "day",
                "2026-11-14",
                "Yoga le matin. Dîner chez Julie, très drôle.",
            )],
        )
        .unwrap();
    let preview = proposals.preview(&proposal).unwrap();
    let item = &preview.files[0].items[0];
    assert_eq!(
        item.before.as_deref(),
        Some("Dîner chez Julie, très drôle.")
    );
    assert_eq!(
        item.after.as_deref(),
        Some("Yoga le matin. Dîner chez Julie, très drôle.")
    );
    assert!(
        std::fs::read_to_string(&path)
            .unwrap()
            .contains("Dîner chez Julie, très drôle.\n")
    );
    proposals.accept(&proposal.id).unwrap();
    let text = std::fs::read_to_string(&path).unwrap();
    assert!(text.contains("Yoga le matin."));
    assert_eq!(text.matches("- [[").count(), 2, "both events listed");
}

#[test]
fn a_summary_must_name_a_period_and_say_something() {
    let dir = tempfile::tempdir().unwrap();
    let vault = vault(dir.path());
    let proposals = Proposals::new(&vault);
    let by = || Author::Client("claude".into());
    for change in [
        write("week", "2026-11-10", "Pas un lundi."),
        write("day", "2026-11-14", "  "),
        write("decade", "2026-01-01", "Non."),
    ] {
        assert!(
            proposals
                .submit(by(), "Résumé.", None, vec![change])
                .is_err()
        );
    }
}

#[test]
fn summary_tasks_are_for_an_ai_that_may_propose_summaries() {
    use ipse_core::rights::{Categories, Clients, Gateway};
    let dir = tempfile::tempdir().unwrap();
    let vault = vault(dir.path());
    // Yesterday, whatever the day the test runs: the AI's call plans with the real today.
    let yesterday = jiff::Zoned::now()
        .with_time_zone(jiff::tz::TimeZone::get("Europe/Paris").unwrap())
        .date()
        .yesterday()
        .unwrap();
    event(dir.path(), "Dîner chez Julie", &yesterday.to_string());

    let categories = Categories::of(vault.settings());
    let mut clients = Clients::load(dir.path()).unwrap();
    let now = ipse_core::vault::DateTime::now(vault.settings().time_zone());
    let mut add = |name: &str, propose: &[&str]| {
        let (client, token) = clients.add(name, now, &categories, false).unwrap();
        let propose = propose.iter().map(|s| (*s).to_owned()).collect();
        clients
            .set_rights(&client.id, None, Some(propose), &categories)
            .unwrap();
        token.expose().to_owned()
    };
    let writer = add("Claude", &["summaries"]);
    let other = add("Autre", &["people", "agenda"]);

    let mut gateway = Gateway::open(dir.path()).unwrap();
    let (tasks, _) = gateway
        .fresh_access(Some(&other))
        .unwrap()
        .tasks(Some("summary"), 10)
        .unwrap();
    assert!(tasks.is_empty(), "no right to propose summaries");
    let refused = gateway.fresh_access(Some(&other)).unwrap().propose(
        None,
        vec![write("day", &yesterday.to_string(), "Dîner.")],
        "Résumé.",
    );
    assert!(refused.is_err());

    let access = gateway.fresh_access(Some(&writer)).unwrap();
    let (tasks, _) = access.tasks(Some("summary"), 10).unwrap();
    assert_eq!(tasks.len(), 1, "planned by the AI's call itself");
    let task = access.task(&tasks[0].id).unwrap();
    assert!(
        task.items
            .iter()
            .any(|i| i.file == "people/julie-dupont.md")
    );
    assert!(
        task.example["changes"][0].get("file").is_none(),
        "the example names a period, not a file"
    );
    let proposed = access
        .propose(
            Some(&tasks[0].id),
            vec![write("day", &yesterday.to_string(), "Dîner chez Julie.")],
            "Résumé de la journée.",
        )
        .unwrap();
    assert_eq!(proposed.status, "waiting");
}
