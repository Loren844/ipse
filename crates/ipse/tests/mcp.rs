//! `ipse mcp` as an AI app runs it: started as a child process, the access code in
//! `IPSE_TOKEN`, MCP over standard input and output (`docs/spec/rights.md`, section 6).

// Test helpers may panic: a failed expectation is a failed test.
#![allow(
    clippy::expect_used,
    clippy::unwrap_used,
    clippy::needless_pass_by_value
)]

use std::path::{Path, PathBuf};
use std::process::Stdio;

use ipse_core::rights::Clients;
use ipse_core::time::TimeZone;
use ipse_core::vault::{
    Body, Category, Common, DateTime, EntityFields, EventFields, EventTime, Fact, FileType,
    Language, LinkOrText, MemoryFile, Relation, Settings, StateLogFields, Topic, Vault, WikiLink,
    body, naming,
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
    let tomorrow = jiff::Timestamp::now()
        .to_zoned(jiff::tz::TimeZone::get("Europe/Paris").unwrap())
        .date()
        .tomorrow()
        .unwrap();
    event(
        &root,
        "agenda",
        "Dîner du samedi",
        &tomorrow.to_string(),
        &["[[paul-durand]]", "[[lea-secret]]", "Zoé"],
    );
    event(&root, "health", "Prise de sang", &tomorrow.to_string(), &[]);
    music(&root, &tomorrow.yesterday().unwrap().to_string());

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

fn event(root: &Path, category: &str, title: &str, day: &str, who: &[&str]) {
    let mut common = Common::new(FileType::Event, &TimeZone::UTC).unwrap();
    common.category = Category::new(category).unwrap();
    let start = EventTime::parse(day).unwrap();
    let mut fields = EventFields::new(title, start);
    fields.participants = who.iter().map(|w| LinkOrText::parse(w)).collect();
    let path = naming::event_path(root, title, &start, &common.id).unwrap();
    MemoryFile::new_event(&path, common, fields, "")
        .write()
        .unwrap();
}

fn music(root: &Path, today: &str) {
    let mut common = Common::new(FileType::Event, &TimeZone::UTC).unwrap();
    common.file_type = FileType::StateLog;
    common.category = Category::new("media").unwrap();
    let topic = Topic::new("music").unwrap();
    let year: i16 = today[..4].parse().unwrap();
    let path = naming::state_log_path(root, &topic, year);
    let body = format!(
        "\n| Subject | First seen | Last seen | Count | Source |\n| --- | --- | --- | --- | --- |\n| Nina Simone | {today} | {today} | 12 | device |\n"
    );
    MemoryFile::new_state_log(&path, common, StateLogFields { topic, year }, body)
        .write()
        .unwrap();
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
async fn the_six_tools_explain_themselves() {
    let f = fixture();
    let ai = connect(&f.root, Some(&f.allowed)).await;
    let info = ai.peer_info().unwrap();
    assert_eq!(info.server_info.as_ref().unwrap().name, "ipse");
    assert!(info.instructions.as_deref().unwrap().contains("search"));

    let mut tools = ai.list_all_tools().await.unwrap();
    tools.sort_by(|a, b| a.name.cmp(&b.name));
    let listed: Vec<&str> = tools.iter().map(|t| t.name.as_ref()).collect();
    assert_eq!(
        listed,
        [
            "about_me",
            "events",
            "get",
            "now",
            "search",
            "states_history"
        ]
    );
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

#[tokio::test]
async fn the_agenda_and_what_is_going_on_now() {
    let f = fixture();
    let ai = connect(&f.root, Some(&f.allowed)).await;
    let today = jiff::Timestamp::now()
        .to_zoned(jiff::tz::TimeZone::get("Europe/Paris").unwrap())
        .date();
    let week = json!({ "from": today.to_string(), "to": today.checked_add(jiff::ToSpan::days(7)).unwrap().to_string() });

    let found = read(&ai, "events", week.clone()).await;
    assert_eq!(
        names(&found, "events"),
        ["Dîner du samedi"],
        "the blood test is health"
    );
    assert_eq!(found["events"][0]["type"], "event");
    assert_eq!(
        found["events"][0]["participants"],
        json!(["[[paul-durand]]", "Zoé"])
    );
    assert_eq!(found["more"], false);

    let paul = read(&ai, "search", json!({ "query": "piano" })).await;
    let mut with_paul = week.clone();
    with_paul["with"] = paul["results"][0]["id"].clone();
    assert_eq!(
        names(&read(&ai, "events", with_paul).await, "events"),
        ["Dîner du samedi"]
    );
    let mut with_marie = week.clone();
    with_marie["with"] = json!(f.marie);
    let none = read(&ai, "events", with_marie).await;
    assert!(none["events"].as_array().unwrap().is_empty());

    let now = read(&ai, "now", json!({})).await;
    assert_eq!(now["states"][0]["subject"], "Nina Simone");
    assert!(now["states"][0].get("status").is_none());

    for bad in [
        json!({ "from": "demain", "to": "2026-11-20" }),
        json!({ "from": "2026-11-20", "to": "2026-11-14" }),
    ] {
        let message = refused(&ai, "events", bad).await;
        assert!(message.contains("2026-11-14"), "{message}");
    }
    ai.cancel().await.unwrap();

    // An AI denied `people` still sees the agenda, but not who is in it.
    let ai = connect(&f.root, Some(&f.denied)).await;
    let found = read(&ai, "events", week).await;
    assert_eq!(found["events"][0]["participants"], json!(["Zoé"]));
    ai.cancel().await.unwrap();
}

// ---------------------------------------------------------------------------------------
// MCP over Streamable HTTP (`ipse mcp --http`), as an AI app that speaks HTTP uses it.

/// `ipse mcp --http` running on a free port; stopped when dropped.
struct HttpServer {
    child: std::process::Child,
    url: String,
}

impl Drop for HttpServer {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

/// Starts `ipse mcp --http 0` and reads the address it says it serves.
fn serve_http(root: &Path) -> HttpServer {
    use std::io::BufRead as _;
    let mut child = std::process::Command::new(env!("CARGO_BIN_EXE_ipse"))
        .args(["mcp", "--http", "0", "--vault"])
        .arg(root)
        .env_remove("IPSE_TOKEN")
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    let mut stderr = std::io::BufReader::new(child.stderr.take().unwrap());
    let mut line = String::new();
    stderr.read_line(&mut line).unwrap();
    let start = line.find("http://").unwrap_or_else(|| panic!("{line}"));
    let url = line[start..]
        .split_whitespace()
        .next()
        .unwrap()
        .trim_end_matches('.')
        .to_owned();
    // Keep reading standard error so that the server never blocks on it.
    std::thread::spawn(move || for _ in stderr.lines() {});
    HttpServer { child, url }
}

/// An HTTP answer: its status and its JSON, if any.
struct Answer {
    status: u16,
    json: Option<Value>,
}

fn post(url: &str, headers: &[(&str, &str)], body: &Value) -> Answer {
    let agent = ureq::Agent::new_with_config(
        ureq::Agent::config_builder()
            .http_status_as_error(false)
            .proxy(None)
            .build(),
    );
    let mut request = agent
        .post(url)
        .header("Content-Type", "application/json")
        .header("Accept", "application/json, text/event-stream");
    for (name, value) in headers {
        request = request.header(*name, *value);
    }
    let mut response = request.send(body.to_string()).unwrap();
    let status = response.status().as_u16();
    let text = response.body_mut().read_to_string().unwrap_or_default();
    let json = serde_json::from_str(&text).ok().or_else(|| {
        // An event stream: the last `data:` line holds the answer.
        text.lines()
            .rev()
            .filter_map(|l| l.strip_prefix("data:"))
            .find_map(|d| serde_json::from_str(d.trim()).ok())
    });
    Answer { status, json }
}

fn bearer(token: &str) -> String {
    format!("Bearer {token}")
}

/// Calls `tool` over HTTP with `token`; returns the tool's result.
fn http_call(server: &HttpServer, token: &str, tool: &str, args: Value) -> Value {
    let auth = bearer(token);
    let answer = post(
        &server.url,
        &[("Authorization", &auth)],
        &json!({
            "jsonrpc": "2.0", "id": 2, "method": "tools/call",
            "params": { "name": tool, "arguments": args },
        }),
    );
    assert_eq!(answer.status, 200);
    answer.json.unwrap()["result"].clone()
}

fn http_read(server: &HttpServer, token: &str, tool: &str, args: Value) -> Value {
    let result = http_call(server, token, tool, args);
    assert_eq!(result["isError"], false, "{result}");
    result["structuredContent"].clone()
}

#[test]
fn over_http_each_tool_with_an_allowed_and_a_denied_category() {
    let f = fixture();
    let server = serve_http(&f.root);

    let initialized = post(
        &server.url,
        &[("Authorization", &bearer(&f.allowed))],
        &json!({
            "jsonrpc": "2.0", "id": 1, "method": "initialize",
            "params": {
                "protocolVersion": "2025-06-18", "capabilities": {},
                "clientInfo": { "name": "test", "version": "1" },
            },
        }),
    );
    assert_eq!(initialized.status, 200);
    let info = &initialized.json.unwrap()["result"];
    assert_eq!(info["serverInfo"]["name"], "ipse");

    let listed = post(
        &server.url,
        &[("Authorization", &bearer(&f.allowed))],
        &json!({ "jsonrpc": "2.0", "id": 3, "method": "tools/list" }),
    );
    let tools = listed.json.unwrap()["result"]["tools"]
        .as_array()
        .unwrap()
        .len();
    assert_eq!(tools, 6);

    let today = jiff::Timestamp::now()
        .to_zoned(jiff::tz::TimeZone::get("Europe/Paris").unwrap())
        .date();
    let week = json!({
        "from": today.to_string(),
        "to": today.checked_add(jiff::ToSpan::days(7)).unwrap().to_string(),
    });
    let history = json!({
        "topic": "music",
        "from": today.checked_sub(jiff::ToSpan::days(7)).unwrap().to_string(),
        "to": today.to_string(),
    });

    // Allowed to read people.
    let a = &f.allowed;
    assert_eq!(
        http_read(&server, a, "about_me", json!({}))["me"]["name"],
        "Camille Durand"
    );
    let found = http_read(&server, a, "search", json!({ "query": "marie" }));
    assert_eq!(names(&found, "results"), ["Marie Dupont"]);
    let marie = http_read(&server, a, "get", json!({ "id": f.marie }));
    assert_eq!(names(&marie, "entries"), ["Marie Dupont"]);
    assert!(!marie.to_string().contains("Léa"), "health stays hidden");
    let events = http_read(&server, a, "events", week.clone());
    assert_eq!(names(&events, "events"), ["Dîner du samedi"]);
    let now = http_read(&server, a, "now", json!({}));
    assert_eq!(now["states"][0]["subject"], "Nina Simone");
    let past = http_read(&server, a, "states_history", history.clone());
    assert_eq!(past["states"][0]["subject"], "Nina Simone");

    // Denied people.
    let d = &f.denied;
    assert_eq!(
        http_read(&server, d, "search", json!({ "query": "marie" })),
        json!({ "results": [], "more": false })
    );
    assert_eq!(
        http_read(&server, d, "get", json!({ "id": f.marie })),
        json!({ "entries": [], "more": false })
    );
    let events = http_read(&server, d, "events", week);
    assert!(
        !events.to_string().contains("paul-durand"),
        "a guest of a denied category is left out: {events}"
    );
    assert_eq!(
        http_read(&server, d, "now", json!({}))["states"][0]["subject"],
        "Nina Simone"
    );
    http_read(&server, d, "states_history", history);
    http_read(&server, d, "about_me", json!({}));
}

#[test]
fn over_http_no_code_or_a_wrong_code_gets_401() {
    let f = fixture();
    let server = serve_http(&f.root);
    let call = json!({
        "jsonrpc": "2.0", "id": 1, "method": "tools/call",
        "params": { "name": "search", "arguments": { "query": "marie" } },
    });
    let wrong = bearer("ipse_0000000000000000");
    let basic = format!("Basic {}", f.allowed);
    for headers in [
        vec![],
        vec![("Authorization", wrong.as_str())],
        vec![("Authorization", basic.as_str())],
        vec![("Authorization", "Bearer")],
    ] {
        let answer = post(&server.url, &headers, &call);
        assert_eq!(answer.status, 401, "{headers:?}");
        assert!(answer.json.is_none(), "no detail");
    }

    // An AI removed while the server runs is refused at its next request.
    let a = f.allowed.clone();
    http_read(&server, &a, "about_me", json!({}));
    assert!(Clients::load(&f.root).unwrap().remove("claude").unwrap());
    assert_eq!(
        post(&server.url, &[("Authorization", &bearer(&a))], &call).status,
        401
    );
}

#[test]
fn over_http_foreign_hosts_and_origins_are_refused() {
    let f = fixture();
    let server = serve_http(&f.root);
    let auth = bearer(&f.allowed);
    let call = json!({
        "jsonrpc": "2.0", "id": 1, "method": "tools/call",
        "params": { "name": "about_me", "arguments": {} },
    });
    for (name, value) in [
        ("Host", "evil.example"),
        ("Host", "evil.example:80"),
        ("Origin", "https://evil.example"),
        ("Origin", "null"),
    ] {
        let answer = post(
            &server.url,
            &[("Authorization", &auth), (name, value)],
            &call,
        );
        assert_eq!(answer.status, 403, "{name}: {value}");
    }
    let local = post(
        &server.url,
        &[
            ("Authorization", &auth),
            ("Origin", "http://localhost:3000"),
        ],
        &call,
    );
    assert_eq!(local.status, 200, "a page on this computer");
    let elsewhere = post(
        &server.url.replace("/mcp", "/other"),
        &[("Authorization", &auth)],
        &call,
    );
    assert_eq!(elsewhere.status, 404);
}

#[test]
fn over_http_only_this_computer() {
    let f = fixture();
    for address in [
        "0.0.0.0:7400",
        "192.168.1.10:7400",
        "[::]:7400",
        "example.org:7400",
    ] {
        let out = std::process::Command::new(env!("CARGO_BIN_EXE_ipse"))
            .args(["mcp", "--http", address, "--vault"])
            .arg(&f.root)
            .output()
            .unwrap();
        assert!(!out.status.success(), "{address}");
        let said = String::from_utf8_lossy(&out.stderr);
        assert!(said.contains("only on this computer"), "{said}");
    }
}
