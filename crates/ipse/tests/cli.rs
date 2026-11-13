//! Tests of the `ipse` program as a person runs it.

// Test helpers may panic: a failed expectation is a failed test.
#![allow(clippy::expect_used, clippy::unwrap_used)]

use std::process::Command;

fn ipse(args: &[&str]) -> std::process::Output {
    Command::new(env!("CARGO_BIN_EXE_ipse"))
        .args(args)
        .output()
        .expect("the ipse binary runs")
}

#[test]
fn version_prints_the_crate_version() {
    let out = ipse(&["--version"]);
    assert!(out.status.success());
    assert_eq!(
        String::from_utf8_lossy(&out.stdout).trim(),
        format!("ipse {}", env!("CARGO_PKG_VERSION"))
    );
}

#[test]
fn help_lists_the_commands() {
    let out = ipse(&["--help"]);
    assert!(out.status.success());
    let help = String::from_utf8_lossy(&out.stdout);
    for command in ["init", "import", "client", "mcp"] {
        assert!(help.contains(command), "help does not list {command}");
    }
}

#[test]
fn init_creates_a_memory() {
    let dir = tempfile::tempdir().unwrap();
    let folder = dir.path().join("memoire");
    let folder_arg = folder.to_str().unwrap();
    let out = ipse(&[
        "init",
        folder_arg,
        "--name",
        "Camille Durand",
        "--language",
        "fr",
        "--time-zone",
        "Europe/Paris",
    ]);
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let said = String::from_utf8_lossy(&out.stdout);
    assert!(said.contains("Your memory is ready"), "{said}");
    assert!(said.contains("camille-durand.md"), "{said}");

    let settings = std::fs::read_to_string(folder.join(".ipse/vault.toml")).unwrap();
    assert!(settings.contains("language = \"fr\""));
    assert!(settings.contains("time_zone = \"Europe/Paris\""));
    let owner = std::fs::read_to_string(folder.join("people/camille-durand.md")).unwrap();
    assert!(owner.contains("owner: true"));

    let again = ipse(&["init", folder_arg, "--name", "Camille Durand"]);
    assert!(!again.status.success());
    assert!(String::from_utf8_lossy(&again.stderr).contains("already holds an Ipse memory"));
}

#[test]
fn init_without_a_name_and_without_a_keyboard_says_what_to_do() {
    let dir = tempfile::tempdir().unwrap();
    let out = Command::new(env!("CARGO_BIN_EXE_ipse"))
        .args(["init", dir.path().to_str().unwrap()])
        .stdin(std::process::Stdio::null())
        .output()
        .unwrap();
    assert!(!out.status.success());
    assert!(String::from_utf8_lossy(&out.stderr).contains("--name"));
    assert!(!dir.path().join(".ipse").exists(), "nothing created");
}

#[test]
fn init_refuses_an_unknown_language() {
    let dir = tempfile::tempdir().unwrap();
    let out = ipse(&[
        "init",
        dir.path().to_str().unwrap(),
        "--name",
        "X",
        "--language",
        "de",
    ]);
    assert!(!out.status.success());
    assert!(String::from_utf8_lossy(&out.stderr).contains("en or fr"));
}

#[test]
fn import_fills_the_memory_with_contacts() {
    let dir = tempfile::tempdir().unwrap();
    let vault = dir.path().to_str().unwrap();
    assert!(
        ipse(&[
            "init",
            vault,
            "--name",
            "Camille Durand",
            "--language",
            "fr"
        ])
        .status
        .success()
    );
    let fixture = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/tests/fixtures/contacts/apple-v4.vcf"
    );

    let out = ipse(&["import", fixture, "--vault", vault]);
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert!(String::from_utf8_lossy(&out.stdout).contains("2 people added, 0 updated."));
    assert!(dir.path().join("people/ana-silva.md").is_file());
    // Camille Durand from the contacts is not the owner: a second file, never a merge.
    assert!(dir.path().join("people/camille-durand-2.md").is_file());

    let again = ipse(&["import", fixture, "--vault", vault]);
    assert!(String::from_utf8_lossy(&again.stdout).contains("already up to date (2 people)"));
}

#[test]
fn import_says_what_it_can_read() {
    let dir = tempfile::tempdir().unwrap();
    let vault = dir.path().to_str().unwrap();
    assert!(ipse(&["init", vault, "--name", "X"]).status.success());
    let out = ipse(&["import", "agenda.ics", "--vault", vault]);
    assert!(!out.status.success());
    assert!(String::from_utf8_lossy(&out.stderr).contains("(.vcf)"));

    let out = ipse(&["import", "missing.vcf", "--vault", vault]);
    assert!(String::from_utf8_lossy(&out.stderr).contains("is not a file"));

    let empty = tempfile::tempdir().unwrap();
    let out = ipse(&["import", "x.vcf", "--vault", empty.path().to_str().unwrap()]);
    assert!(String::from_utf8_lossy(&out.stderr).contains("ipse init"));
}

#[test]
fn client_add_list_remove() {
    let dir = tempfile::tempdir().unwrap();
    let vault = dir.path().to_str().unwrap();
    assert!(
        ipse(&["init", vault, "--name", "Camille Durand"])
            .status
            .success()
    );

    let out = ipse(&["client", "list", "--vault", vault]);
    assert!(String::from_utf8_lossy(&out.stdout).contains("No AI is connected yet"));

    let out = ipse(&["client", "add", "Claude", "--vault", vault]);
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let said = String::from_utf8_lossy(&out.stdout).into_owned();
    assert!(said.contains("Claude can now read your memory: identity, people"));
    assert!(said.contains("Sensitive categories stay closed: location, home, health, private."));
    let token = said
        .split_whitespace()
        .find(|w| w.starts_with("ipse_"))
        .unwrap()
        .to_owned();
    let stored = std::fs::read_to_string(dir.path().join(".ipse/clients.toml")).unwrap();
    assert!(!stored.contains(&token), "the code is shown, never stored");

    let out = ipse(&["client", "list", "--vault", vault]);
    let listed = String::from_utf8_lossy(&out.stdout).into_owned();
    assert!(listed.contains("Claude (claude)"), "{listed}");
    assert!(!listed.contains(&token));

    assert!(
        ipse(&["client", "remove", "claude", "--vault", vault])
            .status
            .success()
    );
    let again = ipse(&["client", "remove", "claude", "--vault", vault]);
    assert!(!again.status.success());
    assert!(String::from_utf8_lossy(&again.stderr).contains("No connected AI is called claude"));
}
