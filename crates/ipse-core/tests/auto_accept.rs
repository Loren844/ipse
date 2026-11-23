//! Standing consent (decision 0017, issue #173): summaries and event kinds from one AI
//! accepted in advance, by the person, through the same path as an accepted proposal.
//! Nothing else passes. All data is invented.

// Test helpers may panic: a failed expectation is a failed test.
#![allow(clippy::expect_used, clippy::unwrap_used)]

use std::path::Path;

use ipse_core::proposals::{Change, Proposals, Status};
use ipse_core::rights::{Categories, Clients, Gateway, Standing};
use ipse_core::time::TimeZone;
use ipse_core::vault::{
    Common, EntityFields, EventFields, EventTime, FileType, History, Language, LinkOrText,
    MemoryFile, Settings, Source, Vault, naming,
};
use jiff::civil::Date;
use serde_json::{Value, json};

struct Fixture {
    _dir: tempfile::TempDir,
    root: std::path::PathBuf,
    token: String,
    client: String,
    /// Days with an event, oldest first, all past.
    days: Vec<Date>,
    lunch: String,
}

fn today() -> Date {
    jiff::Zoned::now()
        .with_time_zone(jiff::tz::TimeZone::get("Europe/Paris").unwrap())
        .date()
}

fn event(root: &Path, title: &str, start: &str) -> String {
    let common = Common::new(FileType::Event, &TimeZone::UTC).unwrap();
    let start = EventTime::parse(start).unwrap();
    let mut fields = EventFields::new(title, start);
    fields.participants = vec![LinkOrText::parse("[[julie-dupont]]")];
    let path = naming::event_path(root, title, &start, &common.id).unwrap();
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    MemoryFile::new_event(&path, common, fields, "")
        .write()
        .unwrap();
    path.strip_prefix(root)
        .unwrap()
        .to_string_lossy()
        .replace('\\', "/")
}

fn fixture() -> Fixture {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path().to_owned();
    let vault = Vault::init(
        &root,
        "Inès Roux",
        Settings::new(Language::Fr, "Europe/Paris").unwrap(),
    )
    .unwrap();
    let common = Common::new(FileType::Person, &TimeZone::UTC).unwrap();
    let path =
        naming::entity_path(&root, FileType::Person, Some("Julie Dupont"), &common.id).unwrap();
    let entity = EntityFields {
        name: Some("Julie Dupont".into()),
        ..EntityFields::default()
    };
    MemoryFile::new(&path, common, Some(entity), "")
        .write()
        .unwrap();
    // Five past days with a lunch each, the most recent two days ago.
    let days: Vec<Date> = (2..7)
        .rev()
        .map(|n| today().checked_sub(jiff::Span::new().days(n)).unwrap())
        .collect();
    let mut lunch = String::new();
    for day in &days {
        lunch = event(
            &root,
            "Déjeuner avec Julie",
            &format!("{day}T12:30:00+01:00"),
        );
    }

    let categories = Categories::of(vault.settings());
    let mut clients = Clients::load(&root).unwrap();
    let now = ipse_core::vault::DateTime::now(vault.settings().time_zone());
    let (client, token) = clients.add("Claude", now, &categories, false).unwrap();
    clients
        .set_rights(
            &client.id,
            None,
            Some(vec!["summaries".into(), "people".into(), "agenda".into()]),
            &categories,
        )
        .unwrap();
    Fixture {
        _dir: dir,
        root,
        token: token.expose().to_owned(),
        client: client.id,
        days,
        lunch,
    }
}

fn write(day: Date, text: &str) -> Change {
    Change::WriteSummary {
        file: None,
        period: "day".into(),
        start: day.to_string(),
        from: None,
        text: text.into(),
    }
}

/// The open summary task of `day`.
fn day_task(gateway: &mut Gateway, token: &str, day: Date) -> String {
    let access = gateway.fresh_access(Some(token)).unwrap();
    let (tasks, _) = access.tasks(Some("summary"), 50).unwrap();
    tasks
        .into_iter()
        .find(|t| {
            access
                .task(&t.id)
                .unwrap()
                .description
                .contains(&day.to_string())
        })
        .unwrap_or_else(|| panic!("a task for {day}"))
        .id
}

fn propose(
    gateway: &mut Gateway,
    token: &str,
    task: Option<&str>,
    changes: Vec<Change>,
) -> (String, &'static str) {
    let proposed = gateway
        .fresh_access(Some(token))
        .unwrap()
        .propose(task, changes, "Résumé de la journée.")
        .unwrap();
    (proposed.proposal.unwrap(), proposed.status)
}

fn set(root: &Path, client: &str, standing: Standing, on: bool) {
    Clients::load(root)
        .unwrap()
        .set_auto_accept(client, standing, on)
        .unwrap();
}

#[test]
fn off_by_default_then_summaries_pass_and_nothing_else() {
    let f = fixture();
    let mut gateway = Gateway::open(&f.root).unwrap();
    let vault = Vault::open(&f.root).unwrap();
    let proposals = Proposals::new(&vault);

    // Off by default: a summary waits.
    let task = day_task(&mut gateway, &f.token, f.days[0]);
    let (first, status) = propose(
        &mut gateway,
        &f.token,
        Some(&task),
        vec![write(f.days[0], "Déjeuner.")],
    );
    assert_eq!(status, "waiting");
    assert!(
        Clients::load(&f.root).unwrap().list()[0]
            .auto_accept
            .is_empty()
    );
    proposals.refuse(&first, None).unwrap();

    set(&f.root, &f.client, Standing::Summaries, true);
    let saved = std::fs::read_to_string(f.root.join(".ipse/clients.toml")).unwrap();
    assert!(saved.contains("auto_accept = [\"summaries\"]"), "{saved}");

    // On: accepted at once, by the same path, and marked so.
    let task = day_task(&mut gateway, &f.token, f.days[1]);
    let (id, status) = propose(
        &mut gateway,
        &f.token,
        Some(&task),
        vec![write(f.days[1], "Déjeuner avec Julie.")],
    );
    assert_eq!(status, "accepted");
    let proposal = proposals.get(&id).unwrap();
    assert!(proposal.automatic);
    let Status::Accepted {
        change: Some(change),
        ..
    } = &proposal.status
    else {
        panic!("{:?}", proposal.status)
    };
    let written = format!("summaries/day/{}/{}.md", f.days[1].year(), f.days[1]);
    assert!(
        std::fs::read_to_string(f.root.join(&written))
            .unwrap()
            .contains("Déjeuner avec Julie.")
    );
    let history = History::open(&f.root).unwrap();
    let entry = history
        .entries(10)
        .unwrap()
        .into_iter()
        .find(|e| &e.id == change)
        .unwrap();
    assert_eq!(
        entry.source,
        Some(Source::Proposal(id.clone())),
        "who: the proposal, by its AI"
    );
    // Undone in one step, like any accepted proposal.
    proposals.undo(&id).unwrap();
    assert!(!f.root.join(&written).exists());

    // A fact from the same AI still waits, with or without a task.
    let (fact, status) = propose(
        &mut gateway,
        &f.token,
        None,
        vec![Change::AddFact {
            file: "people/julie-dupont.md".into(),
            text: "Aime la cuisine japonaise.".into(),
        }],
    );
    assert_eq!(status, "waiting");
    assert_eq!(proposals.get(&fact).unwrap().status, Status::Waiting);
    // A summary without its task waits too: the AI did not get it from Ipse.
    let (loose, status) = propose(
        &mut gateway,
        &f.token,
        None,
        vec![write(f.days[2], "Déjeuner.")],
    );
    assert_eq!(status, "waiting");
    assert!(!proposals.get(&loose).unwrap().automatic);

    // Turned off: waits again.
    set(&f.root, &f.client, Standing::Summaries, false);
    let task = day_task(&mut gateway, &f.token, f.days[3]);
    let (_, status) = propose(
        &mut gateway,
        &f.token,
        Some(&task),
        vec![write(f.days[3], "Déjeuner.")],
    );
    assert_eq!(status, "waiting");
}

#[test]
fn event_kinds_pass_only_with_their_own_consent() {
    let f = fixture();
    let mut gateway = Gateway::open(&f.root).unwrap();
    set(&f.root, &f.client, Standing::Summaries, true);
    let classify = |gateway: &mut Gateway| {
        let access = gateway.fresh_access(Some(&f.token)).unwrap();
        let (tasks, _) = access.tasks(Some("classify"), 10).unwrap();
        tasks[0].id.clone()
    };
    let kind = || Change::SetKey {
        file: f.lunch.clone(),
        key: "kind".into(),
        from: Value::Null,
        to: json!("meal"),
    };
    let task = classify(&mut gateway);
    let (id, status) = propose(&mut gateway, &f.token, Some(&task), vec![kind()]);
    assert_eq!(
        status, "waiting",
        "the consent for summaries says nothing of kinds"
    );
    Proposals::new(&Vault::open(&f.root).unwrap())
        .refuse(&id, None)
        .unwrap();

    set(&f.root, &f.client, Standing::EventKinds, true);
    let task = classify(&mut gateway);
    let (_, status) = propose(&mut gateway, &f.token, Some(&task), vec![kind()]);
    assert_eq!(status, "accepted");
    assert!(
        std::fs::read_to_string(f.root.join(&f.lunch))
            .unwrap()
            .contains("\nkind: meal")
    );
}

#[test]
fn offered_after_three_summaries_accepted_in_a_row_never_before() {
    let f = fixture();
    let mut gateway = Gateway::open(&f.root).unwrap();
    let vault = Vault::open(&f.root).unwrap();
    let proposals = Proposals::new(&vault);
    let mut accept = |day: Date| {
        let task = day_task(&mut gateway, &f.token, day);
        let (id, _) = propose(
            &mut gateway,
            &f.token,
            Some(&task),
            vec![write(day, "Déjeuner.")],
        );
        proposals.accept(&id).unwrap();
    };
    accept(f.days[0]);
    accept(f.days[1]);
    assert!(
        proposals.offers(&f.client, &[]).unwrap().is_empty(),
        "two are not enough"
    );
    accept(f.days[2]);
    assert_eq!(
        proposals.offers(&f.client, &[]).unwrap(),
        [Standing::Summaries]
    );
    assert!(
        proposals
            .offers(&f.client, &[Standing::Summaries])
            .unwrap()
            .is_empty(),
        "not offered once on, or declined"
    );
    assert!(proposals.offers("other", &[]).unwrap().is_empty(), "per AI");

    // A refusal after them: no longer offered.
    let task = day_task(&mut gateway, &f.token, f.days[3]);
    let (id, _) = propose(
        &mut gateway,
        &f.token,
        Some(&task),
        vec![write(f.days[3], "Rien.")],
    );
    proposals.refuse(&id, None).unwrap();
    assert!(proposals.offers(&f.client, &[]).unwrap().is_empty());
}
