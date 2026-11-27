//! `ipse init --demo` (issue #103): a complete invented memory, the same every time
//! apart from dates, made through the normal paths, with nothing that could be real.

// Test helpers may panic: a failed expectation is a failed test.
#![allow(clippy::expect_used, clippy::unwrap_used, clippy::print_stdout)]

use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::Instant;

use ipse_core::proposals::{Change, Proposal, Proposals, Status};
use ipse_core::vault::{History, MemoryFile, Source, Vault};

const IPSE: &str = env!("CARGO_BIN_EXE_ipse");

fn demo(folder: &Path, language: &str) -> String {
    let out = Command::new(IPSE)
        .args([
            "init",
            folder.to_str().unwrap(),
            "--demo",
            "--language",
            language,
        ])
        .args(["--time-zone", "Europe/Paris"])
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    String::from_utf8(out.stdout).unwrap()
}

fn files(dir: &Path) -> Vec<std::path::PathBuf> {
    let mut out = Vec::new();
    for entry in std::fs::read_dir(dir).unwrap().flatten() {
        let path = entry.path();
        if path.is_dir() {
            out.extend(files(&path));
        } else if path.extension().is_some_and(|e| e == "md") {
            out.push(path);
        }
    }
    out.sort();
    out
}

fn names(dir: &Path) -> Vec<String> {
    files(dir)
        .iter()
        .map(|p| p.file_name().unwrap().to_string_lossy().into_owned())
        .collect()
}

#[test]
fn a_demo_memory_full_of_invented_people_and_events() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path().join("demo");
    let started = Instant::now();
    let printed = demo(&root, "fr");
    let took = started.elapsed();
    println!("Demo memory: {took:.2?}");
    assert!(printed.contains("Camille Laurent"), "{printed}");
    assert!(printed.contains("ipse ui --vault"), "{printed}");

    let people = files(&root.join("people"));
    let events = files(&root.join("events"));
    assert!(people.len() >= 140, "{} people", people.len());
    assert!(events.len() >= 250, "{} events", events.len());

    // Nothing that could be someone real: every email at example.org, every French
    // number in the range kept for fiction.
    for path in &people {
        let file = MemoryFile::read(path).unwrap();
        let entity = file.entity().unwrap();
        for email in &entity.emails {
            assert!(email.ends_with("example.org"), "{email}");
        }
        for phone in &entity.phones {
            assert!(phone.starts_with("+3363998"), "{phone}");
        }
    }

    // The owner, their family linked from their own card.
    let owner = std::fs::read_to_string(root.join("people/camille-laurent.md")).unwrap();
    assert!(owner.contains("- mère : [[maman]]"), "{owner}");
    // Dinners sent by friends say so.
    let invited = events
        .iter()
        .filter(|p| {
            std::fs::read_to_string(p)
                .unwrap()
                .contains("\norganizer: ")
        })
        .count();
    assert!(invited > 10, "{invited} invitations from others");
    assert!(
        events.iter().any(|p| {
            let start = p.file_name().unwrap().to_string_lossy()[..10].to_owned();
            start > jiff::Zoned::now().date().to_string()
        }),
        "events ahead, for the home screen"
    );

    // What an AI proposed: summaries accepted and waiting, a fact accepted, one
    // refused with a comment, one waiting; and Ipse's duplicates.
    let vault = Vault::open(&root).unwrap();
    let all = Proposals::new(&vault).list().unwrap();
    let count = |f: &dyn Fn(&Status) -> bool| all.iter().filter(|p| f(&p.status)).count();
    assert_eq!(count(&|s| matches!(s, Status::Waiting)), 6, "{all:#?}");
    assert_eq!(count(&|s| matches!(s, Status::Refused { .. })), 1);
    assert!(count(&|s| matches!(s, Status::Accepted { .. })) >= 4);
    assert!(!files(&root.join("summaries")).is_empty());

    spread_over_the_last_days(&all);

    // The history reads like a real one: the imports, then the accepted proposals.
    let history = History::open(&root).unwrap();
    let sources: Vec<Option<Source>> = history
        .entries(50)
        .unwrap()
        .into_iter()
        .map(|e| e.source)
        .collect();
    assert!(sources.contains(&Some(Source::Connector("contacts".into()))));
    assert!(sources.contains(&Some(Source::Connector("calendar".into()))));
    assert!(
        sources
            .iter()
            .any(|s| matches!(s, Some(Source::Proposal(_))))
    );

    // More than people: places, organizations, works, projects, things, topics, and
    // what is going on right now.
    for (folder, at_least) in [
        ("places", 4),
        ("organizations", 2),
        ("works", 3),
        ("projects", 2),
        ("things", 2),
        ("topics", 2),
    ] {
        assert!(files(&root.join(folder)).len() >= at_least, "{folder}");
    }
    music_and_places(&root, &events);

    // Kinds on part of the events: the older ones; kinds proposed for the nearest.
    let kinds = events
        .iter()
        .filter(|p| std::fs::read_to_string(p).unwrap().contains("\nkind: "))
        .count();
    assert!(
        kinds > 100 && kinds < events.len() - 20,
        "{kinds} of {}",
        events.len()
    );
    assert!(all.iter().any(|p| {
        p.status == Status::Waiting
            && p.changes
                .iter()
                .any(|c| matches!(c, Change::SetKey { key, .. } if key == "kind"))
    }));

    // The same memory every time, apart from ids and dates.
    let again = dir.path().join("again");
    demo(&again, "fr");
    assert_eq!(names(&root.join("people")), names(&again.join("people")));
    assert_eq!(names(&root.join("events")), names(&again.join("events")));
}

#[test]
fn a_demo_memory_in_english() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path().join("demo");
    let printed = demo(&root, "en");
    assert!(printed.contains("Alex Morgan"), "{printed}");
    let owner = std::fs::read_to_string(root.join("people/alex-morgan.md")).unwrap();
    assert!(owner.contains("- mother: [[mom]]"), "{owner}");
    for path in files(&root.join("people")) {
        let file = MemoryFile::read(&path).unwrap();
        for phone in &file.entity().unwrap().phones {
            assert!(phone.starts_with("+1202555"), "{phone}");
        }
    }
}

#[test]
fn a_demo_memory_takes_no_name() {
    let dir = tempfile::tempdir().unwrap();
    let out = Command::new(IPSE)
        .args([
            "init",
            dir.path().to_str().unwrap(),
            "--demo",
            "--name",
            "X",
        ])
        .output()
        .unwrap();
    assert!(!out.status.success());
}

/// The music of the moment has its files, and a location history ties places to
/// events (issue #170).
/// Made over the last days, not all at once (issue #269): no two proposals the same
/// time, the waiting ones within two days, the decided ones within two weeks.
fn spread_over_the_last_days(all: &[Proposal]) {
    let now = jiff::Timestamp::now();
    let when = |text: &str| text.parse::<jiff::Timestamp>().unwrap();
    let mut created: Vec<&str> = all.iter().map(|p| p.created.as_str()).collect();
    created.sort_unstable();
    created.dedup();
    assert_eq!(created.len(), all.len(), "two proposals share a time");
    for proposal in all {
        let made = when(&proposal.created);
        assert!(made <= now, "{proposal:#?}");
        let days = if proposal.status == Status::Waiting {
            3
        } else {
            16
        };
        assert!(
            made > now - jiff::SignedDuration::from_hours(24 * days),
            "{proposal:#?}"
        );
        if let Status::Accepted { at, .. } | Status::Refused { at, .. } = &proposal.status {
            assert!(when(at) >= made && when(at) <= now, "{proposal:#?}");
        }
    }
}

fn music_and_places(root: &Path, events: &[PathBuf]) {
    let year = jiff::Zoned::now().date().year();
    let music = std::fs::read_to_string(root.join(format!("states/music/{year}.md"))).unwrap();
    // The music of the moment has its files (issue #170).
    assert!(music.contains("| [[les-ondes-basses]] |"), "{music}");
    assert!(root.join("organizations/les-ondes-basses.md").is_file());
    assert!(root.join("people/mira-sol.md").is_file());
    // A location history: places with positions, the dinners at them, a state log.
    let located = files(&root.join("places"))
        .iter()
        .filter(|p| std::fs::read_to_string(p).unwrap().contains("\ngeo: ["))
        .count();
    assert!(located >= 8, "{located} places with a position");
    assert!(root.join(format!("states/location/{year}.md")).is_file());
    assert!(
        events.iter().any(|p| std::fs::read_to_string(p)
            .unwrap()
            .contains("place: \"[[chez-jade]]\"")),
        "a dinner linked to the place the phone saw"
    );
    assert!(
        events.iter().any(|p| std::fs::read_to_string(p)
            .unwrap()
            .contains("place: \"[[studio-lotus]]\"")),
        "yoga classes linked to their studio"
    );
}

/// Issue #264: the weekly team meeting of the demo, five colleagues every Monday, does
/// not fill "most often with"; counting work again brings them back, and the AI's
/// answer agrees with the timeline.
#[test]
fn most_often_with_is_not_the_weekly_meeting() {
    use ipse_core::rights::Gateway;
    use ipse_core::vault::EventTime;

    let dir = tempfile::tempdir().unwrap();
    let root = dir.path().join("demo");
    demo(&root, "fr");
    // The meeting's people, as its files link them.
    let mut team: Vec<String> = Vec::new();
    for path in files(&root.join("events")) {
        let file = MemoryFile::read(&path).unwrap();
        let event = file.event().unwrap();
        if event.title == "Point d’équipe" {
            team = event
                .participants
                .iter()
                .filter_map(|p| match p {
                    ipse_core::vault::LinkOrText::Link(link) => {
                        Some(link.target.to_ascii_lowercase())
                    }
                    ipse_core::vault::LinkOrText::Text(_) => None,
                })
                .collect();
            break;
        }
    }
    team.sort();
    assert_eq!(team.len(), 5, "{team:?}");

    let mut gateway = Gateway::open(&root).unwrap();
    let owner = gateway.fresh_owner().unwrap();
    let top = |people: &[ipse_core::search::Seen]| -> Vec<String> {
        let mut stems: Vec<String> = people.iter().map(|s| s.stem.clone()).collect();
        stems.sort();
        stems
    };
    let timeline = owner.timeline();
    let year = timeline
        .years
        .iter()
        .max_by_key(|y| y.events)
        .expect("the demo has events");
    assert_eq!(year.people.len(), 5);
    assert_ne!(
        top(&year.people),
        team,
        "the top of {} is not the team",
        year.year
    );
    let colleagues = year
        .people
        .iter()
        .filter(|s| team.contains(&s.stem))
        .count();
    assert!(colleagues < 3, "{:?}", year.people);

    // The switch "include work" counts every event: the team is back on top.
    let with_work = owner.timeline_with(None, true);
    let same = with_work
        .years
        .iter()
        .find(|y| y.year == year.year)
        .unwrap();
    assert_eq!(top(&same.people), team);

    // The AI's answer for that year follows the timeline's rule.
    let from = EventTime::Day(jiff::civil::date(year.year, 1, 1));
    let to = EventTime::Day(jiff::civil::date(year.year, 12, 31));
    let seen: Vec<(String, u32)> = owner
        .seen_most(&from, &to, 5)
        .into_iter()
        .map(|(link, events)| (link.name, events))
        .collect();
    let counts: Vec<u32> = year.people.iter().map(|s| s.events).collect();
    assert_eq!(seen.iter().map(|(_, n)| *n).collect::<Vec<u32>>(), counts);
}
