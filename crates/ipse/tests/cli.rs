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
fn help_lists_the_planned_commands() {
    let out = ipse(&["--help"]);
    assert!(out.status.success());
    let help = String::from_utf8_lossy(&out.stdout);
    for command in ["init", "import", "client", "mcp"] {
        assert!(help.contains(command), "help does not list {command}");
    }
    assert!(help.contains("not yet available"));
}

#[test]
fn planned_commands_fail_with_a_plain_message() {
    let out = ipse(&["mcp"]);
    assert!(!out.status.success());
    assert_eq!(
        String::from_utf8_lossy(&out.stderr).trim(),
        "`ipse mcp` is not available yet."
    );
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
