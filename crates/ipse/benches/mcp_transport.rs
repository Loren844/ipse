//! One MCP call as an AI app makes it, over standard input and output and over HTTP:
//! the same `ipse mcp` program, the same tool, the same vault of 1 000 people. Prints
//! p50 and p99 of 500 calls each, round trip included.
#![allow(
    missing_docs,
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::print_stdout
)]

use std::io::{BufRead as _, BufReader, Write as _};
use std::path::Path;
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

use ipse_core::rights::Clients;
use ipse_core::vault::{DateTime, Language, Settings, Vault};
use serde_json::{Value, json};

const PEOPLE: usize = 1_000;
const CALLS: usize = 500;

fn write_vault(root: &Path) -> String {
    Vault::init(
        root,
        "Camille Durand",
        Settings::new(Language::Fr, "Europe/Paris").unwrap(),
    )
    .unwrap();
    for i in 0..PEOPLE {
        let text = format!(
            "---\nid: 01pcx8m2p4q6r8t0v2w4{i:06}\ntype: person\ncategory: people\ncreated: 2026-11-12T19:30:00+01:00\nupdated: 2026-11-12T19:30:00+01:00\nname: Personne Numéro {i}\n---\n\n## Faits\n\n- Travaille à la boulangerie du quartier.\n"
        );
        std::fs::write(root.join(format!("people/personne-numero-{i}.md")), text).unwrap();
    }
    let mut clients = Clients::load(root).unwrap();
    let (_, token) = clients
        .add(
            "Bench",
            DateTime::now(&jiff::tz::TimeZone::UTC),
            &ipse_core::rights::Categories::default(),
            false,
        )
        .unwrap();
    token.expose().to_owned()
}

fn call(id: usize, tool: &str, args: &Value) -> Value {
    json!({
        "jsonrpc": "2.0", "id": id, "method": "tools/call",
        "params": { "name": tool, "arguments": args },
    })
}

fn report(name: &str, mut times: Vec<Duration>) {
    times.sort();
    let p = |percent: usize| times[(times.len() - 1) * percent / 100];
    println!("{name:<40} p50 {:>9.3?}  p99 {:>9.3?}", p(50), p(99));
}

fn stdio(root: &Path, token: &str, tool: &str, args: &Value) -> Vec<Duration> {
    let mut child = Command::new(env!("CARGO_BIN_EXE_ipse"))
        .args(["mcp", "--vault"])
        .arg(root)
        .env("IPSE_TOKEN", token)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .unwrap();
    let mut input = child.stdin.take().unwrap();
    let mut output = BufReader::new(child.stdout.take().unwrap());
    let mut line = String::new();
    let mut send = |message: &Value| {
        writeln!(input, "{message}").unwrap();
        input.flush().unwrap();
    };
    send(&json!({
        "jsonrpc": "2.0", "id": 0, "method": "initialize",
        "params": { "protocolVersion": "2025-06-18", "capabilities": {},
                    "clientInfo": { "name": "bench", "version": "1" } },
    }));
    output.read_line(&mut line).unwrap();
    send(&json!({ "jsonrpc": "2.0", "method": "notifications/initialized" }));

    let mut times = Vec::with_capacity(CALLS);
    for i in 1..=CALLS {
        let start = Instant::now();
        send(&call(i, tool, args));
        line.clear();
        output.read_line(&mut line).unwrap();
        times.push(start.elapsed());
        assert!(line.contains("\"isError\":false"), "{line}");
    }
    drop(input);
    let _ = child.kill();
    let _ = child.wait();
    times
}

fn http(root: &Path, token: &str, tool: &str, args: &Value) -> Vec<Duration> {
    let mut child = Command::new(env!("CARGO_BIN_EXE_ipse"))
        .args(["mcp", "--http", "0", "--vault"])
        .arg(root)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    let mut stderr = BufReader::new(child.stderr.take().unwrap());
    let mut line = String::new();
    stderr.read_line(&mut line).unwrap();
    let url = line[line.find("http://").unwrap()..]
        .split_whitespace()
        .next()
        .unwrap()
        .to_owned();
    let agent = ureq::Agent::new_with_config(ureq::Agent::config_builder().proxy(None).build());
    let auth = format!("Bearer {token}");

    let mut times = Vec::with_capacity(CALLS);
    for i in 1..=CALLS {
        let start = Instant::now();
        let text = agent
            .post(&url)
            .header("Authorization", &auth)
            .header("Content-Type", "application/json")
            .header("Accept", "application/json, text/event-stream")
            .send(call(i, tool, args).to_string())
            .unwrap()
            .body_mut()
            .read_to_string()
            .unwrap();
        times.push(start.elapsed());
        assert!(text.contains("\"isError\":false"), "{text}");
    }
    let _ = child.kill();
    let _ = child.wait();
    times
}

fn main() {
    let dir = tempfile::tempdir().unwrap();
    let token = write_vault(dir.path());
    println!("{PEOPLE} people, {CALLS} calls each, round trip");
    for (tool, args) in [
        ("about_me", json!({})),
        ("search", json!({ "query": "personne 421" })),
    ] {
        report(
            &format!("{tool} over stdio"),
            stdio(dir.path(), &token, tool, &args),
        );
        report(
            &format!("{tool} over HTTP"),
            http(dir.path(), &token, tool, &args),
        );
    }
}
