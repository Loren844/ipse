//! Connecting Jellyfin and keeping what the person watches in sync, as `ipse jellyfin
//! add` and `ipse sync` do, against a small server on this computer that answers as
//! Jellyfin's REST API. Every title and name here is invented.

// Test helpers may panic: a failed expectation is a failed test.
#![allow(clippy::expect_used, clippy::unwrap_used)]

use std::io::{BufRead as _, BufReader, Write as _};
use std::net::TcpListener;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::{Arc, Mutex};

use jiff::{SignedDuration, Timestamp};

const IPSE: &str = env!("CARGO_BIN_EXE_ipse");
const TOKEN: &str = "jf-key-5e2d8b07";

/// What the server holds: its users, and the items the first one played.
#[derive(Default)]
struct Media {
    users: Vec<(&'static str, &'static str)>,
    /// Each item: its JSON, as Jellyfin sends it.
    played: Vec<serde_json::Value>,
    /// Each request's path.
    seen: Vec<String>,
}

struct Server {
    url: String,
    media: Arc<Mutex<Media>>,
}

impl Server {
    fn start() -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let url = format!("http://{}", listener.local_addr().unwrap());
        let media = Arc::new(Mutex::new(Media::default()));
        let shared = Arc::clone(&media);
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
                        authorized = line.contains(&format!("Token=\"{TOKEN}\""));
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
                    (401, String::new())
                };
                let response = format!(
                    "HTTP/1.1 {status} X\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{json}",
                    json.len()
                );
                let _ = stream.write_all(response.as_bytes());
            }
        });
        Self { url, media }
    }

    fn with<T>(&self, f: impl FnOnce(&mut Media) -> T) -> T {
        f(&mut self.media.lock().unwrap())
    }
}

fn answer(media: &mut Media, path: &str) -> (u16, String) {
    media.seen.push(path.to_owned());
    if path == "/Users" {
        let users: Vec<serde_json::Value> = media
            .users
            .iter()
            .map(|(id, name)| serde_json::json!({"Id": id, "Name": name}))
            .collect();
        return (200, serde_json::Value::Array(users).to_string());
    }
    if path.starts_with("/Users/u1/Items?") {
        // Played items only: none is under way here.
        let items = if path.contains("Filters=IsPlayed") {
            let mut items = media.played.clone();
            items.sort_by_key(|i| {
                std::cmp::Reverse(i["UserData"]["LastPlayedDate"].as_str().unwrap().to_owned())
            });
            items
        } else {
            Vec::new()
        };
        return (
            200,
            serde_json::json!({"Items": items, "TotalRecordCount": items.len()}).to_string(),
        );
    }
    (404, "{}".to_owned())
}

fn episode(id: &str, series: &str, season: u64, days_ago: i64) -> serde_json::Value {
    serde_json::json!({
        "Id": id, "Type": "Episode", "Name": format!("Épisode {id}"), "SeriesName": series,
        "ParentIndexNumber": season, "IndexNumber": 1,
        "UserData": {"LastPlayedDate": ago(days_ago), "Played": true}
    })
}

fn film(id: &str, title: &str, days_ago: i64) -> serde_json::Value {
    serde_json::json!({
        "Id": id, "Type": "Movie", "Name": title, "ProductionYear": 2019,
        "UserData": {"LastPlayedDate": ago(days_ago), "Played": true}
    })
}

fn ago(days: i64) -> String {
    // Within the current year, whatever the day the test runs.
    let now = Timestamp::now();
    let start_of_year = now
        .to_zoned(jiff::tz::TimeZone::get("Europe/Paris").unwrap())
        .start_of_day()
        .unwrap()
        .with()
        .day_of_year(1)
        .build()
        .unwrap()
        .timestamp();
    let at = (now - SignedDuration::from_hours(days * 24))
        .max(start_of_year + SignedDuration::from_hours(1));
    at.to_string()
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
        let mut args = vec!["jellyfin", "add", &server.url, "--vault", &vault];
        args.extend_from_slice(extra);
        self.ipse(&args, &format!("{token}\n"))
    }

    fn sync(&self) -> String {
        let vault = self.vault_arg();
        let (ok, said) = self.ipse(&["sync", "--vault", &vault], "");
        assert!(ok, "{said}");
        said
    }

    /// The `watching` state log of this year.
    fn watching_log(&self) -> String {
        let dir = self.vault.join("states/watching");
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

#[test]
fn what_is_watched_is_followed_and_kept_in_sync() {
    let server = Server::start();
    server.with(|m| {
        m.users.push(("u1", "Inès"));
        m.played = vec![
            episode("e1", "Les Archives", 1, 3),
            episode("e2", "Les Archives", 2, 2),
            film("m1", "Le Phare", 1),
        ];
    });
    let memory = Memory::new();
    // A file of the person's works: the series links to it.
    std::fs::create_dir_all(memory.vault.join("works")).unwrap();
    std::fs::write(
        memory.vault.join("works/les-archives.md"),
        "---\nid: 01jcx9a1b2c3d4e5f6g7h8j9k0\ntype: work\ncategory: media\ncreated: 2026-01-03T10:00:00+01:00\nupdated: 2026-01-03T10:00:00+01:00\nname: Les Archives\n---\n",
    )
    .unwrap();
    let (ok, said) = memory.add(&server, &["--name", "Films"], TOKEN);
    assert!(ok, "{said}");
    assert!(said.contains("following u1"), "{said}");

    let log = memory.watching_log();
    assert!(log.contains("topic: watching"), "{log}");
    assert!(log.contains("category: media"), "{log}");
    let row = |subject: &str| {
        log.lines()
            .find(|l| l.starts_with(&format!("| {subject} ")))
            .unwrap_or_else(|| panic!("no row for {subject}: {log}"))
            .to_owned()
    };
    assert!(
        row("[[les-archives]]").contains("| 2 |"),
        "a series is one row: {log}"
    );
    assert!(row("Le Phare").contains("| 1 |"), "{log}");

    // A new episode: the next sync reads it, and only what is newer.
    server.with(|m| {
        m.seen.clear();
        m.played.push(episode("e3", "Les Archives", 2, 0));
    });
    memory.sync();
    let log = memory.watching_log();
    assert!(
        memory_row(&log, "[[les-archives]]").contains("| 3 |"),
        "{log}"
    );
    memory.assert_no_token();
}

fn memory_row(log: &str, subject: &str) -> String {
    log.lines()
        .find(|l| l.starts_with(&format!("| {subject} ")))
        .unwrap_or_default()
        .to_owned()
}

#[test]
fn a_wrong_key_connects_nothing() {
    let server = Server::start();
    server.with(|m| m.users.push(("u1", "Inès")));
    let memory = Memory::new();
    let (ok, said) = memory.add(&server, &[], "wrong-key");
    assert!(!ok);
    assert!(said.contains("Jellyfin refused the API key"), "{said}");
    let vault = memory.vault_arg();
    let (_, said) = memory.ipse(&["jellyfin", "list", "--vault", &vault], "");
    assert!(said.contains("No Jellyfin connected yet"), "{said}");
}
