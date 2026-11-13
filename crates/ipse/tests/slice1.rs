//! Slice 1 acceptance: a person creates their memory, imports their contacts, connects
//! their AI, and the AI answers "who is Élodie?" (issue #30).
//!
//! Every step runs the `ipse` program as the person and their AI app do, nothing
//! through the library. The guide `docs/guide/getting-started.md` walks the same steps.

// Test helpers may panic: a failed expectation is a failed test.
#![allow(clippy::expect_used, clippy::unwrap_used)]

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

#[tokio::test]
async fn ask_your_ai_who_someone_is() {
    let dir = tempfile::tempdir().unwrap();
    let vault = dir.path().join("Ipse");
    let vault_arg = vault.to_str().unwrap();
    let contacts =
        Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/contacts/google-v3.vcf");

    // 1. Create the memory.
    let printed = ipse(&[
        "init",
        vault_arg,
        "--name",
        "Jules Moreau",
        "--language",
        "fr",
        "--time-zone",
        "Europe/Paris",
    ]);
    assert!(printed.contains("Your memory is ready"), "{printed}");

    // 2. Import the contacts exported from the phone.
    let printed = ipse(&["import", contacts.to_str().unwrap(), "--vault", vault_arg]);
    assert!(printed.contains("2 people added"), "{printed}");
    assert!(
        printed.contains("1 unreadable card(s) skipped."),
        "the command line speaks English in a French memory: {printed}"
    );

    // 3. Connect two AIs. The second one is then denied `people`, as the person could
    //    choose on the connection screen.
    let claude = access_code(&ipse(&["client", "add", "Claude", "--vault", vault_arg]));
    let other = access_code(&ipse(&["client", "add", "Autre IA", "--vault", vault_arg]));
    let clients = vault.join(".ipse/clients.toml");
    let text = std::fs::read_to_string(&clients).unwrap();
    let at = text.find("id = \"autre-ia\"").unwrap();
    let (first, second) = text.split_at(at);
    std::fs::write(
        &clients,
        format!("{first}{}", second.replacen("\"people\", ", "", 1)),
    )
    .unwrap();
    let listed = ipse(&["client", "list", "--vault", vault_arg]);
    assert!(listed.contains("Autre IA (autre-ia)") && listed.contains("Claude (claude)"));

    // 4. The AI app starts `ipse mcp`; the person asks "who am I?", then "who is
    //    Élodie?".
    let ai = connect(&vault, &claude).await;
    let me = ask(&ai, "about_me", json!({})).await;
    assert_eq!(me["me"]["name"], "Jules Moreau");

    let found = ask(&ai, "search", json!({ "query": "elodie" })).await;
    assert_eq!(found["results"].as_array().unwrap().len(), 1, "{found}");
    assert_eq!(found["results"][0]["name"], "Élodie Martin");
    let id = found["results"][0]["id"].as_str().unwrap();

    let elodie = ask(&ai, "get", json!({ "id": id })).await;
    let entry = &elodie["entries"][0];
    assert_eq!(entry["name"], "Élodie Martin");
    assert_eq!(entry["aliases"], json!(["Lolo"]));
    assert_eq!(entry["birthday"], "1990-04-02");
    assert_eq!(entry["emails"], json!(["elodie.martin@example.org"]));
    assert_eq!(entry["phones"], json!(["06 12 34 56 78"]));
    assert!(
        entry["notes"].as_str().unwrap().contains("club d'escalade"),
        "{entry}"
    );
    let by_name = ask(&ai, "get", json!({ "name": "lolo" })).await;
    assert_eq!(by_name["entries"][0]["id"], id, "found by her nickname");
    ai.cancel().await.unwrap();

    // 5. The AI denied `people` finds no one, and cannot tell anyone is hidden.
    let ai = connect(&vault, &other).await;
    let me = ask(&ai, "about_me", json!({})).await;
    assert_eq!(me["me"]["name"], "Jules Moreau", "identity stays allowed");
    for query in ["elodie", "lolo", "escalade", "marc", "leroy"] {
        let found = ask(&ai, "search", json!({ "query": query })).await;
        assert_eq!(found, json!({ "results": [], "more": false }), "{query}");
    }
    let nothing = json!({ "entries": [], "more": false });
    assert_eq!(ask(&ai, "get", json!({ "id": id })).await, nothing);
    assert_eq!(
        ask(&ai, "get", json!({ "name": "Élodie Martin" })).await,
        nothing
    );
    ai.cancel().await.unwrap();
}
