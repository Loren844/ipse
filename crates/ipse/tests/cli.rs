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
    let out = ipse(&["init"]);
    assert!(!out.status.success());
    assert_eq!(
        String::from_utf8_lossy(&out.stderr).trim(),
        "`ipse init` is not available yet."
    );
}
