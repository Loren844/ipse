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
    for command in [
        "init", "import", "calendar", "sync", "check", "client", "mcp",
    ] {
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
fn import_a_calendar_after_the_contacts() {
    let dir = tempfile::tempdir().unwrap();
    let vault = dir.path().to_str().unwrap();
    let ok = |args: &[&str]| {
        let out = ipse(args);
        assert!(
            out.status.success(),
            "{}",
            String::from_utf8_lossy(&out.stderr)
        );
        String::from_utf8_lossy(&out.stdout).into_owned()
    };
    ok(&[
        "init",
        vault,
        "--name",
        "Camille Durand",
        "--language",
        "fr",
        "--time-zone",
        "Europe/Paris",
    ]);
    let fixtures = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures");
    ok(&[
        "import",
        &format!("{fixtures}/contacts/google-v3.vcf"),
        "--vault",
        vault,
    ]);
    let printed = ok(&[
        "import",
        &format!("{fixtures}/calendar/google.ics"),
        "--vault",
        vault,
    ]);
    assert!(
        printed.contains("1 unreadable event(s) skipped."),
        "{printed}"
    );
    assert!(printed.contains("events added, 0 updated."), "{printed}");

    let dinner = std::fs::read_to_string(
        dir.path()
            .join("events/2026/2026-11-14-diner-chez-julie.md"),
    )
    .unwrap();
    assert!(
        dinner.contains("\"[[marc-leroy]]\""),
        "linked by email: {dinner}"
    );
    assert!(dinner.contains("  - Julie Dupont\n"), "no file: plain text");
    assert!(dinner.contains("Apporter le dessert."));
    assert!(
        dir.path()
            .join("events/2026/2026-12-24-noel-chez-les-parents.md")
            .is_file()
    );
    assert!(
        dir.path()
            .join("events/2026/2026-11-18-yoga-decale-au-mercredi.md")
            .is_file(),
        "a moved occurrence"
    );

    let again = ok(&[
        "import",
        &format!("{fixtures}/calendar/google.ics"),
        "--vault",
        vault,
    ]);
    assert!(
        again.contains("Every event was already up to date"),
        "{again}"
    );
}

#[test]
fn import_says_what_it_can_read() {
    let dir = tempfile::tempdir().unwrap();
    let vault = dir.path().to_str().unwrap();
    assert!(ipse(&["init", vault, "--name", "X"]).status.success());
    let out = ipse(&["import", "notes.txt", "--vault", vault]);
    assert!(!out.status.success());
    let message = String::from_utf8_lossy(&out.stderr);
    assert!(
        message.contains("(.vcf)") && message.contains("(.ics)"),
        "{message}"
    );

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

#[test]
fn subscriptions_without_any_yet() {
    let dir = tempfile::tempdir().unwrap();
    let vault = dir.path().to_str().unwrap();
    assert!(
        ipse(&["init", vault, "--name", "Camille Durand"])
            .status
            .success()
    );

    let out = ipse(&["sync", "--vault", vault]);
    assert!(out.status.success());
    assert!(String::from_utf8_lossy(&out.stdout).contains("Nothing to sync yet"));
    let due = ipse(&["sync", "--due", "--vault", vault]);
    assert!(due.status.success());
    assert!(due.stdout.is_empty(), "a scheduled job says nothing");

    let list = ipse(&["calendar", "list", "--vault", vault]);
    assert!(String::from_utf8_lossy(&list.stdout).contains("No calendar subscriptions yet"));
    let unknown = ipse(&["calendar", "unsubscribe", "calendar-2", "--vault", vault]);
    assert!(!unknown.status.success());
    assert!(String::from_utf8_lossy(&unknown.stderr).contains("no calendar subscription"));
}

#[test]
fn subscribe_refuses_links_it_cannot_follow_safely() {
    let dir = tempfile::tempdir().unwrap();
    let vault = dir.path().to_str().unwrap();
    assert!(
        ipse(&["init", vault, "--name", "Camille Durand"])
            .status
            .success()
    );
    for (link, said) in [
        ("http://cal.example/private-abc/basic.ics", "secure links"),
        ("agenda.ics", "not a calendar link"),
    ] {
        let out = ipse(&["calendar", "subscribe", link, "--vault", vault]);
        assert!(!out.status.success(), "{link}");
        let error = String::from_utf8_lossy(&out.stderr);
        assert!(error.contains(said), "{error}");
        assert!(!error.contains("private-abc"), "the link is never shown");
    }
    let every = ipse(&[
        "calendar",
        "subscribe",
        "https://cal.example/x.ics",
        "--every",
        "1m",
        "--vault",
        vault,
    ]);
    assert!(String::from_utf8_lossy(&every.stderr).contains("at least 5 minutes"));
    assert!(
        !dir.path().join(".ipse/connectors.toml").exists(),
        "nothing kept"
    );
}

#[test]
fn an_outlook_export_fills_address_title_and_phones() {
    let dir = tempfile::tempdir().unwrap();
    let vault = dir.path().to_str().unwrap();
    let init = ipse(&[
        "init",
        vault,
        "--name",
        "Camille Durand",
        "--language",
        "fr",
        "--time-zone",
        "Europe/Paris",
    ]);
    assert!(init.status.success());
    let fixture = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/tests/fixtures/contacts/outlook-v21.vcf"
    );
    let out = ipse(&["import", fixture, "--vault", vault]);
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let rene = std::fs::read_to_string(dir.path().join("people/rene-lefevre.md")).unwrap();
    for expected in [
        "address: \"3 Montée de la Butte, 69001 Lyon, France\"\n",
        "  - \"+33478123456\"\n",
        "- Organisation : Atelier du Théâtre, Costumes. ^f-",
        "- Poste : Costumier en chef. ^f-",
        "Préfère qu’on l’appelle le matin.",
    ] {
        assert!(rene.contains(expected), "{expected:?} in {rene}");
    }
}

#[test]
fn check_says_what_to_fix() {
    let dir = tempfile::tempdir().unwrap();
    let vault = dir.path().to_str().unwrap();
    assert!(
        ipse(&["init", vault, "--name", "Camille Durand"])
            .status
            .success()
    );
    let ok = ipse(&["check", "--vault", vault]);
    assert!(ok.status.success());
    assert!(String::from_utf8_lossy(&ok.stdout).contains("Every file reads well (1 file)."));

    let owner = dir.path().join("people/camille-durand.md");
    let text = std::fs::read_to_string(&owner).unwrap();
    std::fs::write(
        &owner,
        format!("{text}- Aime le thé. ^f-3k9w\n- Aime le café. ^f-3k9w\n"),
    )
    .unwrap();
    let out = ipse(&["check", "--vault", vault]);
    assert!(!out.status.success());
    let said = String::from_utf8_lossy(&out.stdout);
    assert!(
        said.contains("camille-durand.md: the fact id ^f-3k9w"),
        "{said}"
    );
    assert!(!said.contains("thé"), "the file is not quoted");
}
