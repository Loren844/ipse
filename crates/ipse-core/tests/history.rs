//! The vault's invisible history (`docs/spec/vault.md`, section 9).

// Test helpers may panic: a failed expectation is a failed test.
#![allow(clippy::expect_used, clippy::unwrap_used)]

use std::path::{Path, PathBuf};

use ipse_core::vault::history::Entry;
use ipse_core::vault::{History, Language, Settings, Source, Vault};

fn init(root: &Path) -> Vault {
    Vault::init(
        root,
        "Camille Durand",
        Settings::new(Language::Fr, "Europe/Paris").unwrap(),
    )
    .unwrap()
}

fn entries(root: &Path) -> Vec<Entry> {
    History::open(root).unwrap().entries(50).unwrap()
}

fn write(root: &Path, rel: &str, text: &str) -> PathBuf {
    let path = root.join(rel);
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(&path, text).unwrap();
    path
}

#[test]
fn init_in_an_empty_folder_is_one_change_by_ipse() {
    let dir = tempfile::tempdir().unwrap();
    init(dir.path());
    let entries = entries(dir.path());
    assert_eq!(entries.len(), 1);
    assert_eq!(entries[0].source, Some(Source::Ipse));
    assert_eq!(entries[0].summary, "Create this memory");
    assert_eq!(
        entries[0].files,
        [".gitignore", ".ipse/vault.toml", "people/camille-durand.md"]
    );
    let ignore = std::fs::read_to_string(dir.path().join(".gitignore")).unwrap();
    for line in [
        "journal/",
        ".ipse/index/",
        ".ipse/proposals/",
        ".ipse/clients.toml",
    ] {
        assert!(ignore.lines().any(|l| l == line), "{line}");
    }
}

#[test]
fn init_where_notes_already_are_records_them_as_the_person_s() {
    let dir = tempfile::tempdir().unwrap();
    write(dir.path(), "Journal intime.md", "Mes notes.");
    write(dir.path(), ".gitignore", "brouillons/\n");
    write(dir.path(), "brouillons/x.md", "pas dans l'historique");
    init(dir.path());

    let entries = entries(dir.path());
    let summaries: Vec<(&str, Option<&Source>)> = entries
        .iter()
        .map(|e| (e.summary.as_str(), e.source.as_ref()))
        .collect();
    assert_eq!(
        summaries,
        [
            ("Create this memory", Some(&Source::Ipse)),
            ("Notes already in this folder", Some(&Source::Person)),
        ]
    );
    assert_eq!(entries[1].files, ["Journal intime.md"]);
    let ignore = std::fs::read_to_string(dir.path().join(".gitignore")).unwrap();
    assert!(
        ignore.starts_with("brouillons/\njournal/\n"),
        "the person's lines kept first"
    );
}

#[test]
fn one_change_touching_three_files_is_one_commit() {
    let dir = tempfile::tempdir().unwrap();
    let vault = init(dir.path());
    let history = vault.history().unwrap();
    let mut change = history.change(Source::Connector("contacts".into()));
    for name in ["a", "b", "c"] {
        change.touched(&write(dir.path(), &format!("people/{name}.md"), name));
    }
    change.touched(&write(dir.path(), "journal/contacts/x.jsonl", "raw"));
    let entry = change.commit("Import contacts").unwrap().unwrap();
    assert_eq!(
        entry.files,
        ["people/a.md", "people/b.md", "people/c.md"],
        "the journal is not memory"
    );
    assert_eq!(entry.source, Some(Source::Connector("contacts".into())));

    let entries = History::open(dir.path()).unwrap().entries(10).unwrap();
    assert_eq!(entries.len(), 2);
    assert_eq!(entries[0].files.len(), 3);

    let mut nothing = history.change(Source::Connector("contacts".into()));
    nothing.touched(&dir.path().join("people/a.md"));
    assert!(
        nothing.commit("Import contacts").unwrap().is_none(),
        "no change, no commit"
    );
}

#[test]
fn edits_made_outside_ipse_are_recorded_as_the_person_s() {
    let dir = tempfile::tempdir().unwrap();
    let vault = init(dir.path());
    let history = vault.history().unwrap();
    let mut change = history.change(Source::Connector("contacts".into()));
    let marie = write(dir.path(), "people/marie.md", "v1");
    let paul = write(dir.path(), "people/paul.md", "v1");
    change.touched(&marie);
    change.touched(&paul);
    change.commit("Import contacts").unwrap();

    std::fs::write(&marie, "v2, corrigé à la main").unwrap();
    std::fs::remove_file(&paul).unwrap();
    write(dir.path(), "places/lyon.md", "nouveau");
    write(dir.path(), ".obsidian/workspace.json", "{}");
    write(dir.path(), "journal/calendar/x.jsonl", "raw");

    let history = vault.history().unwrap();
    let entry = &history.entries(1).unwrap()[0];
    assert_eq!(entry.source, Some(Source::Person));
    assert_eq!(entry.summary, "Changes made outside Ipse");
    assert_eq!(
        entry.files,
        ["people/marie.md", "people/paul.md", "places/lyon.md"]
    );
    assert!(
        history.record_outside_edits().unwrap().is_none(),
        "recorded once"
    );

    let sources = history
        .last_sources(&[marie.clone(), dir.path().join("people/camille-durand.md")])
        .unwrap();
    assert_eq!(sources[&marie], Source::Person);
    assert_eq!(
        sources[&dir.path().join("people/camille-durand.md")],
        Source::Ipse
    );
}

#[test]
fn a_vault_without_history_starts_one() {
    let dir = tempfile::tempdir().unwrap();
    let vault = init(dir.path());
    std::fs::remove_dir_all(dir.path().join(".git")).unwrap();
    write(dir.path(), "people/ana.md", "made before the history");
    let history = vault.history().unwrap();
    let entries = history.entries(10).unwrap();
    assert_eq!(entries.len(), 1);
    assert_eq!(entries[0].source, Some(Source::Ipse));
    assert_eq!(entries[0].summary, "Start the history of this memory");
    assert!(entries[0].files.contains(&"people/ana.md".to_owned()));
}

#[test]
fn git_sees_a_clean_folder() {
    let Ok(version) = std::process::Command::new("git").arg("--version").output() else {
        return; // git is not needed by Ipse; this only checks compatibility when present.
    };
    if !version.status.success() {
        return;
    }
    let dir = tempfile::tempdir().unwrap();
    let vault = init(dir.path());
    let history = vault.history().unwrap();
    let mut change = history.change(Source::Person);
    change.touched(&write(dir.path(), "people/x.md", "x"));
    change.commit("Add x").unwrap();
    let status = std::process::Command::new("git")
        .args(["status", "--porcelain"])
        .current_dir(dir.path())
        .output()
        .unwrap();
    assert!(status.status.success());
    assert_eq!(String::from_utf8_lossy(&status.stdout), "");
    let log = std::process::Command::new("git")
        .args(["log", "-1", "--format=%an%n%B"])
        .current_dir(dir.path())
        .output()
        .unwrap();
    assert_eq!(
        String::from_utf8_lossy(&log.stdout).trim(),
        "Ipse\nAdd x\n\nIpse-Source: person"
    );
}
