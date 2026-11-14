//! Milestone 1 acceptance (issue #19): a person creates their memory, imports their
//! contacts and their calendar from files, connects two AIs, and each AI answers from
//! what it may read: the one denied `agenda` sees no event, and cannot tell any exists.
//!
//! Every step runs the `ipse` program as the person and their AI app do, nothing
//! through the library. The guide `docs/guide/getting-started.md` walks the same steps.
//! The scenario's time is printed (`cargo test --test milestone1 -- --nocapture`).

// Test helpers may panic: a failed expectation is a failed test.
#![allow(clippy::expect_used, clippy::unwrap_used, clippy::print_stdout)]

use std::time::Instant;

use std::path::Path;
use std::process::{Command, Stdio};

use rmcp::RoleClient;
use rmcp::ServiceExt;
use rmcp::model::CallToolRequestParams;
use rmcp::service::RunningService;
use rmcp::transport::TokioChildProcess;
use serde_json::{Value, json};

const IPSE: &str = env!("CARGO_BIN_EXE_ipse");

/// Runs `ipse` with `args` and returns what it printed; fails the test if it failed.
fn ipse(args: &[&str]) -> String {
    let out = Command::new(IPSE).args(args).output().unwrap();
    assert!(
        out.status.success(),
        "ipse {args:?}: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    String::from_utf8(out.stdout).unwrap()
}

/// The access code `ipse client add` shows.
fn access_code(printed: &str) -> String {
    printed
        .split_whitespace()
        .find(|w| w.starts_with("ipse_"))
        .expect("an access code is shown")
        .to_owned()
}

/// Starts `ipse mcp` the way the AI app does, as configured in the guide.
async fn connect(vault: &Path, token: &str) -> RunningService<RoleClient, ()> {
    let mut command = tokio::process::Command::new(IPSE);
    command
        .arg("mcp")
        .arg("--vault")
        .arg(vault)
        .env("IPSE_TOKEN", token);
    let (transport, _) = TokioChildProcess::builder(command)
        .stderr(Stdio::null())
        .spawn()
        .unwrap();
    ().serve(transport).await.unwrap()
}

/// Calls `tool` and returns its JSON answer.
async fn ask(ai: &RunningService<RoleClient, ()>, tool: &str, args: Value) -> Value {
    let Value::Object(args) = args else {
        panic!("arguments are an object")
    };
    let result = ai
        .call_tool(CallToolRequestParams::new(tool.to_owned()).with_arguments(args))
        .await
        .unwrap();
    assert_eq!(result.is_error, Some(false), "{tool}: {result:?}");
    result.structured_content.unwrap()
}

/// The `name` of each item in `answer[key]`.
fn names(answer: &Value, key: &str) -> Vec<String> {
    answer[key]
        .as_array()
        .unwrap()
        .iter()
        .map(|r| r["name"].as_str().unwrap().to_owned())
        .collect()
}

#[tokio::test]
async fn import_connect_and_ask() {
    let started = Instant::now();
    let dir = tempfile::tempdir().unwrap();
    let vault = dir.path().join("Ipse");
    let vault_arg = vault.to_str().unwrap();
    let fixtures = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures");
    let contacts = fixtures.join("contacts/google-v3.vcf");
    let calendar = fixtures.join("calendar/google.ics");

    // 1. Create the memory, then import the contacts and the calendar exported from the
    //    person's apps: `ipse import` picks the connector from the file type.
    ipse(&[
        "init",
        vault_arg,
        "--name",
        "Jules Moreau",
        "--language",
        "fr",
        "--time-zone",
        "Europe/Paris",
    ]);
    let printed = ipse(&["import", contacts.to_str().unwrap(), "--vault", vault_arg]);
    assert!(printed.contains("2 people added"), "{printed}");
    let printed = ipse(&["import", calendar.to_str().unwrap(), "--vault", vault_arg]);
    assert!(printed.contains("events added"), "{printed}");

    // 2. Connect two AIs. The second one is denied `agenda`, as the person could choose
    //    on the connection screen.
    let claude = access_code(&ipse(&["client", "add", "Claude", "--vault", vault_arg]));
    let other = access_code(&ipse(&["client", "add", "Autre IA", "--vault", vault_arg]));
    let clients = vault.join(".ipse/clients.toml");
    let text = std::fs::read_to_string(&clients).unwrap();
    let at = text.find("id = \"autre-ia\"").unwrap();
    let (first, second) = text.split_at(at);
    assert!(second.contains("\"agenda\", "));
    std::fs::write(
        &clients,
        format!("{first}{}", second.replacen("\"agenda\", ", "", 1)),
    )
    .unwrap();
    let period = json!({ "from": "2026-11-01", "to": "2027-01-01" });

    // 3. The AI denied `agenda`: it knows the person and their contacts, and no event.
    let ai = connect(&vault, &other).await;
    let me = ask(&ai, "about_me", json!({})).await;
    assert_eq!(me["me"]["name"], "Jules Moreau");
    let found = ask(&ai, "search", json!({ "query": "elodie" })).await;
    assert_eq!(names(&found, "results"), ["Élodie Martin"]);
    assert_eq!(
        ask(&ai, "events", period.clone()).await,
        json!({ "events": [], "more": false })
    );
    for query in ["dîner", "yoga", "noël"] {
        let found = ask(&ai, "search", json!({ "query": query })).await;
        assert_eq!(found, json!({ "results": [], "more": false }), "{query}");
    }
    ai.cancel().await.unwrap();

    // 4. The AI allowed `agenda` gets the events of the period.
    let ai = connect(&vault, &claude).await;
    let found = ask(&ai, "events", period).await;
    let titles = names(&found, "events");
    for title in ["Dîner chez Julie", "Noël chez les parents"] {
        assert!(titles.iter().any(|t| t == title), "{title}: {titles:?}");
    }
    let yoga = ask(&ai, "search", json!({ "query": "yoga" })).await;
    let id = yoga["results"][0]["id"].clone();
    assert!(id.is_string(), "{yoga}");
    ai.cancel().await.unwrap();

    // 5. Even with the event's id, the AI denied `agenda` gets nothing.
    let ai = connect(&vault, &other).await;
    assert_eq!(
        ask(&ai, "get", json!({ "id": id })).await,
        json!({ "entries": [], "more": false })
    );
    ai.cancel().await.unwrap();

    println!("milestone 1 scenario: {:.2?}", started.elapsed());
}
