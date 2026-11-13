//! `ipse mcp` as an AI app runs it: started as a child process, the access code in
//! `IPSE_TOKEN`, MCP over standard input and output (`docs/spec/rights.md`, section 6).

// Test helpers may panic: a failed expectation is a failed test.
#![allow(clippy::expect_used, clippy::unwrap_used)]

use std::path::{Path, PathBuf};
use std::process::Stdio;

use ipse_core::rights::Clients;
use ipse_core::time::TimeZone;
use ipse_core::vault::{
    Body, Category, Common, DateTime, EntityFields, Fact, FileType, Language, MemoryFile, Relation,
    Settings, Vault, WikiLink, body, naming,
};
use rmcp::RoleClient;
use rmcp::ServiceExt;
use rmcp::model::{CallToolRequestParams, CallToolResult};
use rmcp::service::RunningService;
use rmcp::transport::TokioChildProcess;
use serde_json::{Value, json};

/// Writes a person and returns their id.
fn person(
    root: &Path,
    category: &str,
    name: &str,
    facts: &[&str],
    relations: &[(&str, &str)],
) -> String {
    let mut common = Common::new(FileType::Person, &TimeZone::UTC).unwrap();
    common.category = Category::new(category).unwrap();
    let id = common.id.to_string();
    let path = naming::entity_path(root, FileType::Person, Some(name), &common.id).unwrap();
    let sections = Body {
        facts: facts
            .iter()
            .map(|f| Fact {
                text: (*f).to_owned(),
                id: None,
            })
            .collect(),
        relations: relations
            .iter()
            .map(|(word, target)| Relation {
                word: (*word).to_owned(),
                link: WikiLink::parse(&format!("[[{target}]]")).unwrap(),
            })
            .collect(),
        notes: None,
    };
    let entity = EntityFields {
        name: Some(name.to_owned()),
        ..EntityFields::default()
    };
    MemoryFile::new(
        &path,
        common,
        Some(entity),
        body::render(&sections, Language::Fr),
    )
    .write()
    .unwrap();
    id
}

struct Fixture {
    _dir: tempfile::TempDir,
    root: PathBuf,
    /// Reads every non-sensitive category.
    allowed: String,
    /// Reads every non-sensitive category but `people`.
    denied: String,
    marie: String,
}

fn fixture() -> Fixture {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path().to_owned();
    Vault::init(
        &root,
        "Camille Durand",
        Settings::new(Language::Fr, "Europe/Paris").unwrap(),
    )
    .unwrap();
    let marie = person(
        &root,
        "people",
        "Marie Dupont",
        &["Cousine de [[lea-secret]].", "Voisine de [[paul-durand]]."],
        &[("ami", "paul-durand"), ("sœur", "lea-secret")],
    );
    person(&root, "people", "Paul Durand", &["Joue du piano."], &[]);
    person(&root, "health", "Léa Secret", &["Suivie à l'hôpital."], &[]);

    let mut clients = Clients::load(&root).unwrap();
    let now = || DateTime::now(&TimeZone::UTC);
    let (_, allowed) = clients.add("Claude", now()).unwrap();
    let (_, denied) = clients.add("Autre IA", now()).unwrap();
    let file = root.join(".ipse/clients.toml");
    let text = std::fs::read_to_string(&file).unwrap();
    let at = text.find("id = \"autre-ia\"").unwrap();
    let (first, second) = text.split_at(at);
    std::fs::write(
        &file,
        format!("{first}{}", second.replace("\"people\", ", "")),
    )
    .unwrap();

    Fixture {
        root,
        allowed: allowed.expose().to_owned(),
        denied: denied.expose().to_owned(),
        marie,
        _dir: dir,
    }
}

/// Starts `ipse mcp` on `root` the way an AI app does.
async fn connect(root: &Path, token: Option<&str>) -> RunningService<RoleClient, ()> {
    let mut command = tokio::process::Command::new(env!("CARGO_BIN_EXE_ipse"));
    command.arg("mcp").arg("--vault").arg(root);
    command.env_remove("IPSE_TOKEN");
    if let Some(token) = token {
        command.env("IPSE_TOKEN", token);
    }
    let (transport, _) = TokioChildProcess::builder(command)
        .stderr(Stdio::null())
        .spawn()
        .unwrap();
    ().serve(transport).await.unwrap()
}

async fn call(ai: &RunningService<RoleClient, ()>, tool: &str, args: Value) -> CallToolResult {
    let Value::Object(args) = args else {
        panic!("arguments are an object")
    };
    ai.call_tool(CallToolRequestParams::new(tool.to_owned()).with_arguments(args))
        .await
        .unwrap()
}

/// The JSON a successful call returned.
async fn read(ai: &RunningService<RoleClient, ()>, tool: &str, args: Value) -> Value {
    let result = call(ai, tool, args).await;
    assert_eq!(result.is_error, Some(false), "{result:?}");
    let text = &result.content[0].as_text().unwrap().text;
    let value: Value = serde_json::from_str(text).unwrap();
    assert_eq!(result.structured_content.as_ref(), Some(&value));
    value
}

/// The message of a refused call.
async fn refused(ai: &RunningService<RoleClient, ()>, tool: &str, args: Value) -> String {
    let result = call(ai, tool, args).await;
    assert_eq!(result.is_error, Some(true), "{result:?}");
    assert!(result.structured_content.is_none());
    result.content[0].as_text().unwrap().text.clone()
}

fn names(results: &Value, key: &str) -> Vec<String> {
    results[key]
        .as_array()
        .unwrap()
        .iter()
        .map(|r| r["name"].as_str().unwrap().to_owned())
        .collect()
}

#[tokio::test]
async fn the_three_tools_explain_themselves() {
    let f = fixture();
    let ai = connect(&f.root, Some(&f.allowed)).await;
    let info = ai.peer_info().unwrap();
    assert_eq!(info.server_info.as_ref().unwrap().name, "ipse");
    assert!(info.instructions.as_deref().unwrap().contains("search"));

    let mut tools = ai.list_all_tools().await.unwrap();
    tools.sort_by(|a, b| a.name.cmp(&b.name));
    let listed: Vec<&str> = tools.iter().map(|t| t.name.as_ref()).collect();
    assert_eq!(listed, ["about_me", "get", "search"]);
    for tool in &tools {
        let description = tool.description.as_deref().unwrap();
        assert!(description.contains("Example"), "{}", tool.name);
    }
    ai.cancel().await.unwrap();
}

#[tokio::test]
async fn an_allowed_ai_reads_people_without_what_is_hidden() {
    let f = fixture();
    let ai = connect(&f.root, Some(&f.allowed)).await;

    let me = read(&ai, "about_me", json!({})).await;
    assert_eq!(me["me"]["name"], "Camille Durand");
    assert_eq!(me["me"]["type"], "person");

    let found = read(&ai, "search", json!({ "query": "marie" })).await;
    assert_eq!(names(&found, "results"), ["Marie Dupont"]);
    assert_eq!(found["results"][0]["id"], f.marie.as_str());
    assert_eq!(found["results"][0]["type"], "person");
    assert_eq!(found["more"], false);

    let durand = read(&ai, "search", json!({ "query": "durand", "limit": 1 })).await;
    assert_eq!(names(&durand, "results").len(), 1);
    assert_eq!(durand["more"], true, "Camille and Paul Durand");

    let marie = read(&ai, "get", json!({ "id": f.marie })).await;
    let entry = &marie["entries"][0];
    assert_eq!(entry["name"], "Marie Dupont");
    assert_eq!(entry["facts"], json!(["Voisine de [[paul-durand]]."]));
    assert_eq!(
        entry["relations"],
        json!([{ "word": "ami", "target": "[[paul-durand]]" }])
    );
    let text = marie.to_string();
    for leak in ["lea", "Léa", "Cousine", "sœur", "hôpital"] {
        assert!(!text.contains(leak), "{leak} in {text}");
    }

    let paul = read(&ai, "get", json!({ "name": "[[paul-durand]]" })).await;
    assert_eq!(names(&paul, "entries"), ["Paul Durand"]);
    let paul = read(&ai, "get", json!({ "name": "PAUL durand" })).await;
    assert_eq!(paul["entries"][0]["facts"], json!(["Joue du piano."]));

    assert!(
        read(&ai, "search", json!({ "query": "lea" })).await["results"]
            .as_array()
            .unwrap()
            .is_empty()
    );
    ai.cancel().await.unwrap();
}

#[tokio::test]
async fn an_ai_denied_people_finds_nothing_about_them() {
    let f = fixture();
    let ai = connect(&f.root, Some(&f.denied)).await;

    let me = read(&ai, "about_me", json!({})).await;
    assert_eq!(
        me["me"]["name"], "Camille Durand",
        "identity is still allowed"
    );

    for query in ["marie", "paul", "piano", "voisine"] {
        let found = read(&ai, "search", json!({ "query": query })).await;
        assert_eq!(found, json!({ "results": [], "more": false }), "{query}");
    }
    let nothing = json!({ "entries": [], "more": false });
    assert_eq!(read(&ai, "get", json!({ "id": f.marie })).await, nothing);
    assert_eq!(
        read(&ai, "get", json!({ "name": "Marie Dupont" })).await,
        nothing
    );
    assert_eq!(
        read(&ai, "get", json!({ "id": "01jd5z8x3k4m2n6p7q9r0s1t2v" })).await,
        nothing,
        "an unknown id looks the same as a hidden one"
    );
    ai.cancel().await.unwrap();
}

#[tokio::test]
async fn a_missing_or_wrong_code_is_refused() {
    let f = fixture();
    let mut wrong = f.allowed.clone();
    let last = if wrong.pop() == Some('0') { '1' } else { '0' };
    wrong.push(last);
    for (token, says) in [
        (None, "IPSE_TOKEN"),
        (Some("ipse_0000"), "not valid"),
        (Some(wrong.as_str()), "not valid"),
    ] {
        let ai = connect(&f.root, token).await;
        for (tool, args) in [
            ("about_me", json!({})),
            ("search", json!({ "query": "marie" })),
            ("get", json!({ "id": f.marie })),
        ] {
            let message = refused(&ai, tool, args).await;
            assert!(message.contains(says), "{tool}: {message}");
            assert!(!message.contains("Marie"));
        }
        ai.cancel().await.unwrap();
    }
}

#[tokio::test]
async fn a_removed_ai_is_refused_at_its_next_request() {
    let f = fixture();
    let ai = connect(&f.root, Some(&f.allowed)).await;
    read(&ai, "search", json!({ "query": "marie" })).await;

    assert!(Clients::load(&f.root).unwrap().remove("claude").unwrap());
    let message = refused(&ai, "search", json!({ "query": "marie" })).await;
    assert!(message.contains("not valid"), "{message}");
    ai.cancel().await.unwrap();
}

#[tokio::test]
async fn get_needs_an_id_or_a_name() {
    let f = fixture();
    let ai = connect(&f.root, Some(&f.allowed)).await;
    for args in [
        json!({}),
        json!({ "name": "  " }),
        json!({ "id": f.marie, "name": "Paul Durand" }),
    ] {
        let message = refused(&ai, "get", args).await;
        assert!(message.contains("`id` or a `name`"), "{message}");
    }
    ai.cancel().await.unwrap();
}

#[tokio::test]
async fn a_folder_that_is_not_a_memory_stops_at_once() {
    let dir = tempfile::tempdir().unwrap();
    let out = std::process::Command::new(env!("CARGO_BIN_EXE_ipse"))
        .arg("mcp")
        .arg("--vault")
        .arg(dir.path())
        .stdin(Stdio::null())
        .output()
        .unwrap();
    assert!(!out.status.success());
    assert!(out.stdout.is_empty(), "nothing but MCP on standard output");
    assert!(!String::from_utf8_lossy(&out.stderr).trim().is_empty());
}
