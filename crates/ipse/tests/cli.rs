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
        "init",
        "import",
        "calendar",
        "sync",
        "check",
        "proposals",
        "tasks",
        "client",
        "mcp",
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

#[test]
fn review_a_proposal_from_the_command_line() {
    let dir = tempfile::tempdir().unwrap();
    let vault = dir.path().join("memoire");
    let vault_arg = vault.to_str().unwrap();
    let ok = |args: &[&str]| {
        let out = ipse(args);
        assert!(
            out.status.success(),
            "{args:?}: {}",
            String::from_utf8_lossy(&out.stderr)
        );
        String::from_utf8(out.stdout).unwrap()
    };
    ok(&[
        "init",
        vault_arg,
        "--name",
        "Camille Durand",
        "--language",
        "fr",
        "--time-zone",
        "Europe/Paris",
    ]);
    assert_eq!(
        ok(&["proposals", "list", "--vault", vault_arg]).trim(),
        "Nothing to review."
    );

    // A proposal as an AI leaves it, in the format of `docs/spec/vault.md` section 12.
    let proposal = |id: &str, text: &str| {
        let json = format!(
            r#"{{"format": 1, "id": "{id}", "by": "client:claude", "created": "2026-11-14T19:02:11+01:00",
            "reason": "Dit dans la conversation.", "categories": ["people"],
            "changes": [{{"kind": "add_fact", "file": "people/camille-durand.md", "text": "{text}"}}],
            "status": {{"state": "waiting"}}}}"#
        );
        std::fs::create_dir_all(vault.join(".ipse/proposals")).unwrap();
        std::fs::write(vault.join(format!(".ipse/proposals/{id}.json")), json).unwrap();
    };
    proposal("01jd3k8m2p4q6r8t0v2w4y6z8a", "Joue du violoncelle.");
    proposal("01jd3k9m2p4q6r8t0v2w4y6z8a", "Parle italien.");

    let listed = ok(&["proposals", "list", "--vault", vault_arg]);
    assert!(
        listed.starts_with("01jd3k9m2p4q6r  waiting   client:claude  Dit dans la conversation."),
        "{listed}"
    );
    let shown = ok(&["proposals", "show", "01jd3k8m", "--vault", vault_arg]);
    assert!(shown.contains("Why: Dit dans la conversation."), "{shown}");
    assert!(
        shown.contains("Camille Durand (people/camille-durand.md)\n  + fact: Joue du violoncelle."),
        "{shown}"
    );

    assert!(ok(&["proposals", "accept", "01jd3k8m", "--vault", vault_arg]).starts_with("Accepted"));
    let owner = vault.join("people/camille-durand.md");
    assert!(
        std::fs::read_to_string(&owner)
            .unwrap()
            .contains("- Joue du violoncelle. ^f-")
    );
    assert!(
        ok(&[
            "proposals",
            "refuse",
            "01jd3k9m",
            "--comment",
            "Pas vrai",
            "--vault",
            vault_arg
        ])
        .starts_with("Refused")
    );
    assert!(
        ok(&["proposals", "show", "01jd3k9m", "--vault", vault_arg]).contains("You said: Pas vrai")
    );
    assert!(ok(&["proposals", "undo", "01jd3k8m", "--vault", vault_arg]).starts_with("Undone"));
    assert!(
        !std::fs::read_to_string(&owner)
            .unwrap()
            .contains("violoncelle")
    );
    assert_eq!(
        ok(&["proposals", "list", "--vault", vault_arg]).trim(),
        "Nothing to review."
    );
    assert_eq!(
        ok(&["proposals", "list", "--all", "--vault", vault_arg])
            .lines()
            .count(),
        2
    );

    let out = ipse(&["proposals", "accept", "01jd3k9m", "--vault", vault_arg]);
    assert!(!out.status.success());
    assert_eq!(
        String::from_utf8_lossy(&out.stderr).trim(),
        "This proposal is already refused."
    );
}

#[test]
fn notes_from_an_import_become_a_task_for_the_ai() {
    let dir = tempfile::tempdir().unwrap();
    let vault = dir.path().join("memoire");
    let vault_arg = vault.to_str().unwrap();
    let ok = |args: &[&str]| {
        let out = ipse(args);
        assert!(
            out.status.success(),
            "{args:?}: {}",
            String::from_utf8_lossy(&out.stderr)
        );
        String::from_utf8(out.stdout).unwrap()
    };
    ok(&[
        "init",
        vault_arg,
        "--name",
        "Camille Durand",
        "--language",
        "fr",
        "--time-zone",
        "Europe/Paris",
    ]);
    let contacts = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/contacts/google-v3.vcf");
    let contacts = contacts.to_str().unwrap();
    let printed = ok(&["import", contacts, "--vault", vault_arg]);
    assert!(
        printed.contains("1 task for your AI: notes to turn into facts (`ipse tasks list`)."),
        "{printed}"
    );
    let printed = ok(&["import", contacts, "--vault", vault_arg]);
    assert!(
        !printed.contains("task"),
        "same notes, no new task: {printed}"
    );

    let listed = ok(&["tasks", "list", "--vault", vault_arg]);
    assert_eq!(listed.lines().count(), 1, "{listed}");
    assert!(
        listed.contains("  extract   open      Read the notes of"),
        "{listed}"
    );
    let id = listed.split_whitespace().next().unwrap();
    let shown = ok(&["tasks", "show", id, "--vault", vault_arg]);
    assert!(
        shown.contains("Élodie Martin (people/elodie-martin.md, people)"),
        "{shown}"
    );
    assert!(shown.contains("club d'escalade"), "{shown}");
    assert!(shown.contains("Example of a good proposal:"), "{shown}");
}
