//! Connecting Home Assistant and keeping the person's presence in sync, as
//! `ipse homeassistant add` and `ipse sync` do, against a small server on this computer
//! that answers as Home Assistant's REST API. Every name and zone here is invented.

// Test helpers may panic: a failed expectation is a failed test.
#![allow(clippy::expect_used, clippy::unwrap_used)]

use std::io::{BufRead as _, BufReader, Write as _};
use std::net::TcpListener;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::{Arc, Mutex};

use jiff::{SignedDuration, Timestamp};

const IPSE: &str = env!("CARGO_BIN_EXE_ipse");
const TOKEN: &str = "ha-token-7f3b2c91";

/// What the server holds: the people, and the chosen one's changes of state.
#[derive(Default)]
struct Home {
    people: Vec<(&'static str, &'static str)>,
    /// Each change: when, and the new state.
    changes: Vec<(Timestamp, String)>,
    /// Each request's path.
    seen: Vec<String>,
}

struct Server {
    url: String,
    home: Arc<Mutex<Home>>,
}

impl Server {
    fn start() -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let url = format!("http://{}", listener.local_addr().unwrap());
        let home = Arc::new(Mutex::new(Home::default()));
        let shared = Arc::clone(&home);
        std::thread::spawn(move || {
            for stream in listener.incoming() {
                let Ok(mut stream) = stream else { continue };
                let mut reader = BufReader::new(stream.try_clone().unwrap());
                let mut first = String::new();
                if reader.read_line(&mut first).unwrap_or(0) == 0 {
                    continue;
                }
                let mut authorized = false;
                loop {
                    let mut line = String::new();
                    if reader.read_line(&mut line).unwrap_or(0) == 0 || line == "\r\n" {
                        break;
                    }
                    if line.to_ascii_lowercase().starts_with("authorization:") {
                        authorized =
                            line["authorization:".len()..].trim() == format!("Bearer {TOKEN}");
                    }
                }
                let path = first
                    .split_whitespace()
                    .nth(1)
                    .unwrap_or_default()
                    .to_owned();
                let (status, json) = if authorized {
                    answer(&mut shared.lock().unwrap(), &path)
                } else {
                    (401, "{\"message\":\"Unauthorized\"}".to_owned())
                };
                let response = format!(
                    "HTTP/1.1 {status} X\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{json}",
                    json.len()
                );
                let _ = stream.write_all(response.as_bytes());
            }
        });
        Self { url, home }
    }

    fn with<T>(&self, f: impl FnOnce(&mut Home) -> T) -> T {
        f(&mut self.home.lock().unwrap())
    }
}

fn answer(home: &mut Home, path: &str) -> (u16, String) {
    home.seen.push(path.to_owned());
    if path == "/api/states" {
        let people: Vec<serde_json::Value> = home
            .people
            .iter()
            .map(|(entity, name)| {
                serde_json::json!({"entity_id": entity, "state": "home", "attributes": {"friendly_name": name}})
            })
            .chain([serde_json::json!({"entity_id": "light.salon", "state": "on", "attributes": {}})])
            .collect();
        return (200, serde_json::Value::Array(people).to_string());
    }
    if let Some(rest) = path.strip_prefix("/api/history/period/") {
        let from: Timestamp = rest.split('?').next().unwrap().parse().unwrap();
        // The state at the start of the period, then each change after it.
        let mut items = Vec::new();
        if let Some((_, state)) = home.changes.iter().rev().find(|(at, _)| *at <= from) {
            items.push(serde_json::json!({"entity_id": "person.ines", "state": state, "last_changed": from.to_string()}));
        }
        for (at, state) in home.changes.iter().filter(|(at, _)| *at > from) {
            items.push(serde_json::json!({"state": state, "last_changed": at.to_string()}));
        }
        return (200, serde_json::json!([items]).to_string());
    }
    (404, "{}".to_owned())
}

struct Memory {
    _dir: tempfile::TempDir,
    vault: PathBuf,
    secrets: PathBuf,
}

impl Memory {
    fn new() -> Self {
        let dir = tempfile::tempdir().unwrap();
        let vault = dir.path().join("memoire");
        let secrets = dir.path().join("secrets.json");
        let memory = Self {
            vault,
            secrets,
            _dir: dir,
        };
        let path = memory.vault.to_str().unwrap().to_owned();
        let out = memory.ipse(
            &["init", &path, "--name", "Inès Roux", "--language", "fr"],
            "",
        );
        assert!(out.0, "{}", out.1);
        memory
    }

    /// Runs ipse with `input`; whether it succeeded, and what it said.
    fn ipse(&self, args: &[&str], input: &str) -> (bool, String) {
        let mut child = Command::new(IPSE)
            .args(args)
            .env("IPSE_TEST_SECRETS", &self.secrets)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .unwrap();
        child
            .stdin
            .take()
            .unwrap()
            .write_all(input.as_bytes())
            .unwrap();
        let out = child.wait_with_output().unwrap();
        let said = format!(
            "{}{}",
            String::from_utf8_lossy(&out.stdout),
            String::from_utf8_lossy(&out.stderr)
        );
        (out.status.success(), said)
    }

    fn vault_arg(&self) -> String {
        self.vault.to_str().unwrap().to_owned()
    }

    fn add(&self, server: &Server, extra: &[&str], token: &str) -> (bool, String) {
        let vault = self.vault_arg();
        let mut args = vec!["homeassistant", "add", &server.url, "--vault", &vault];
        args.extend_from_slice(extra);
        self.ipse(&args, &format!("{token}\n"))
    }

    fn sync(&self) -> String {
        let vault = self.vault_arg();
        let (ok, said) = self.ipse(&["sync", "--vault", &vault], "");
        assert!(ok, "{said}");
        said
    }

    /// The `home` state log of this year.
    fn home_log(&self) -> String {
        let dir = self.vault.join("states/home");
        let path = std::fs::read_dir(&dir)
            .unwrap()
            .next()
            .unwrap()
            .unwrap()
            .path();
        std::fs::read_to_string(path).unwrap()
    }

    /// The token is in no file of the memory.
    fn assert_no_token(&self) {
        for path in files(&self.vault) {
            let Ok(text) = std::fs::read_to_string(&path) else {
                continue;
            };
            assert!(!text.contains(TOKEN), "the token is in {}", path.display());
        }
    }
}

fn files(dir: &Path) -> Vec<PathBuf> {
    let mut out = Vec::new();
    let Ok(entries) = std::fs::read_dir(dir) else {
        return out;
    };
    for entry in entries {
        let path = entry.unwrap().path();
        if path.is_dir() {
            out.extend(files(&path));
        } else {
            out.push(path);
        }
    }
    out
}

fn hours_ago(hours: i64) -> Timestamp {
    (Timestamp::now() - SignedDuration::from_hours(hours))
        .round(jiff::Unit::Second)
        .unwrap()
}

#[test]
fn presence_is_followed_and_kept_in_sync() {
    let server = Server::start();
    let back_home = hours_ago(10);
    server.with(|h| {
        h.people.push(("person.ines", "Inès"));
        h.changes = vec![
            (hours_ago(30), "home".to_owned()),
            (hours_ago(20), "not_home".to_owned()),
            (hours_ago(19), "Travail".to_owned()),
            (back_home, "home".to_owned()),
        ];
    });
    let memory = Memory::new();
    let (ok, said) = memory.add(&server, &["--name", "Maison"], TOKEN);
    assert!(ok, "{said}");
    assert!(said.contains("following person.ines"), "{said}");
    assert!(
        said.contains("http://"),
        "the person is told the token is not encrypted: {said}"
    );

    let log = memory.home_log();
    assert!(log.contains("topic: home"), "{log}");
    assert!(log.contains("category: home"), "{log}");
    let row = |name: &str| {
        log.lines()
            .find(|l| l.starts_with(&format!("| {name} ")))
            .unwrap_or_else(|| panic!("no row for {name}: {log}"))
            .to_owned()
    };
    assert!(row("Domicile").contains("| 2 |"), "{log}");
    assert!(row("Travail").contains("| 1 |"), "{log}");
    assert!(!log.contains("not_home"), "being away is not a row: {log}");

    // Leaving home: the next sync reads from the stay going on, and adds the new one.
    server.with(|h| {
        h.seen.clear();
        h.changes.push((hours_ago(1), "Salle de sport".to_owned()));
    });
    memory.sync();
    let asked = server.with(|h| h.seen.clone());
    let from = back_home.to_string();
    assert!(
        asked
            .iter()
            .any(|p| p.starts_with(&format!("/api/history/period/{from}"))),
        "{asked:?}"
    );
    let log = memory.home_log();
    assert!(log.contains("| Salle de sport "), "{log}");
    assert!(
        log.contains("| Domicile ") && log.contains("| 2 |"),
        "{log}"
    );
    memory.assert_no_token();

    let vault = memory.vault_arg();
    let (ok, said) = memory.ipse(&["homeassistant", "list", "--vault", &vault], "");
    assert!(
        ok && said.contains("Maison") && said.contains("person.ines"),
        "{said}"
    );
}

#[test]
fn a_wrong_token_connects_nothing() {
    let server = Server::start();
    server.with(|h| h.people.push(("person.ines", "Inès")));
    let memory = Memory::new();
    let (ok, said) = memory.add(&server, &[], "wrong-token");
    assert!(!ok);
    assert!(said.contains("refused the access token"), "{said}");
    let vault = memory.vault_arg();
    let (_, said) = memory.ipse(&["homeassistant", "list", "--vault", &vault], "");
    assert!(said.contains("No Home Assistant connected yet"), "{said}");
}

#[test]
fn with_several_people_ipse_asks_whom_to_follow() {
    let server = Server::start();
    server.with(|h| {
        h.people.push(("person.ines", "Inès"));
        h.people.push(("person.paul", "Paul"));
        h.changes = vec![(hours_ago(5), "home".to_owned())];
    });
    let memory = Memory::new();
    let (ok, said) = memory.add(&server, &[], TOKEN);
    assert!(!ok);
    assert!(
        said.contains("--person: person.ines (Inès), person.paul (Paul)"),
        "{said}"
    );
    let (ok, said) = memory.add(&server, &["--person", "person.ines"], TOKEN);
    assert!(ok, "{said}");
    assert!(memory.home_log().contains("| Domicile "));
}
