//! `ipse init --demo` (issue #103): a complete invented memory, the same every time
//! apart from dates, made through the normal paths, with nothing that could be real.

// Test helpers may panic: a failed expectation is a failed test.
#![allow(clippy::expect_used, clippy::unwrap_used, clippy::print_stdout)]

use std::path::Path;
use std::process::Command;
use std::time::Instant;

use ipse_core::proposals::{Change, Proposals, Status};
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
    let year = jiff::Zoned::now().date().year();
    let music = std::fs::read_to_string(root.join(format!("states/music/{year}.md"))).unwrap();
    assert!(music.contains("| Les Ondes Basses |"), "{music}");
    assert!(
        events.iter().any(|p| std::fs::read_to_string(p)
            .unwrap()
            .contains("place: \"[[studio-lotus]]\"")),
        "yoga classes linked to their studio"
    );

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
