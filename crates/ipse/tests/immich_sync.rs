//! Connecting Immich and keeping who is on the photos in sync, as `ipse immich add` and
//! `ipse sync` do, against a small server on this computer that answers as Immich's
//! REST API. Every name here is invented.

// Test helpers may panic: a failed expectation is a failed test.
#![allow(clippy::expect_used, clippy::unwrap_used)]

use std::io::{BufRead as _, BufReader, Read as _, Write as _};
use std::net::TcpListener;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::{Arc, Mutex};

use jiff::{SignedDuration, Timestamp};

const IPSE: &str = env!("CARGO_BIN_EXE_ipse");
const TOKEN: &str = "im-key-1f6a90c3";

/// One person of the photos: id, name, how many photos, days since the first and the
/// last.
type Person = (&'static str, &'static str, u64, i64, i64);

#[derive(Default)]
struct Library {
    people: Vec<Person>,
}

struct Server {
    url: String,
    library: Arc<Mutex<Library>>,
}

impl Server {
    fn start() -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let url = format!("http://{}", listener.local_addr().unwrap());
        let library = Arc::new(Mutex::new(Library::default()));
        let shared = Arc::clone(&library);
        std::thread::spawn(move || {
            for stream in listener.incoming() {
                let Ok(mut stream) = stream else { continue };
                let mut reader = BufReader::new(stream.try_clone().unwrap());
                let mut first = String::new();
                if reader.read_line(&mut first).unwrap_or(0) == 0 {
                    continue;
                }
                let mut authorized = false;
                let mut length = 0;
                loop {
                    let mut line = String::new();
                    if reader.read_line(&mut line).unwrap_or(0) == 0 || line == "\r\n" {
                        break;
                    }
                    let lower = line.to_ascii_lowercase();
                    if let Some(v) = lower.strip_prefix("content-length:") {
                        length = v.trim().parse().unwrap_or(0);
                    }
                    if lower.starts_with("x-api-key:") {
                        authorized = line["x-api-key:".len()..].trim() == TOKEN;
                    }
                }
                let mut body = vec![0; length];
                reader.read_exact(&mut body).unwrap();
                let body = String::from_utf8(body).unwrap();
                let path = first
                    .split_whitespace()
                    .nth(1)
                    .unwrap_or_default()
                    .to_owned();
                let (status, json) = if authorized {
                    answer(&shared.lock().unwrap(), &path, &body)
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
        Self { url, library }
    }

    fn with<T>(&self, f: impl FnOnce(&mut Library) -> T) -> T {
        f(&mut self.library.lock().unwrap())
    }
}

fn days_ago(days: i64) -> String {
    (Timestamp::now() - SignedDuration::from_hours(days * 24)).to_string()
}

fn answer(library: &Library, path: &str, body: &str) -> (u16, String) {
    if path == "/api/users/me" {
        return (
            200,
            serde_json::json!({"id": "u1", "name": "Inès Roux", "email": "ines@example.org"})
                .to_string(),
        );
    }
    if path.starts_with("/api/people?") {
        let people: Vec<serde_json::Value> = library
            .people
            .iter()
            .map(|(id, name, ..)| serde_json::json!({"id": id, "name": name, "isHidden": false}))
            .collect();
        return (
            200,
            serde_json::json!({"people": people, "total": people.len(), "hasNextPage": false})
                .to_string(),
        );
    }
    let find = |id: &str| library.people.iter().find(|p| p.0 == id);
    if let Some(id) = path
        .strip_prefix("/api/people/")
        .and_then(|p| p.strip_suffix("/statistics"))
    {
        return (
            200,
            serde_json::json!({"assets": find(id).map_or(0, |p| p.2)}).to_string(),
        );
    }
    if path == "/api/search/metadata" {
        let query: serde_json::Value = serde_json::from_str(body).unwrap();
        let id = query["personIds"][0].as_str().unwrap();
        let person = find(id).unwrap();
        let days = if query["order"] == "asc" {
            person.3
        } else {
            person.4
        };
        return (200, serde_json::json!({"assets": {"total": 1, "count": 1, "items": [{"id": "a", "fileCreatedAt": days_ago(days)}]}}).to_string());
    }
    (404, "{}".to_owned())
}

fn person_file(name: &str, id: &str) -> String {
    format!(
        "---\nid: {id}\ntype: person\ncategory: people\ncreated: 2026-01-03T10:00:00+01:00\nupdated: 2026-01-03T10:00:00+01:00\nname: {name}\n---\n"
    )
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
        let mut args = vec!["immich", "add", &server.url, "--vault", &vault];
        args.extend_from_slice(extra);
        self.ipse(&args, &format!("{token}\n"))
    }

    fn sync(&self) -> String {
        let vault = self.vault_arg();
        let (ok, said) = self.ipse(&["sync", "--vault", &vault], "");
        assert!(ok, "{said}");
        said
    }

    /// The `photos` state log of this year.
    fn photos_log(&self) -> String {
        let dir = self.vault.join("states/photos");
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
fn the_people_of_the_photos_are_found_again_or_proposed() {
    let server = Server::start();
    server.with(|l| {
        l.people = vec![
            ("p1", "Gaspard Lemoine", 42, 3, 1),
            ("p2", "Paul", 17, 2, 1),
            ("p3", "Inès Roux", 300, 3, 0),
            ("p4", "Zoé", 5, 1, 1),
            ("p5", "", 9, 1, 1),
        ];
    });
    let memory = Memory::new();
    std::fs::write(
        memory.vault.join("people/gaspard-lemoine.md"),
        person_file("Gaspard Lemoine", "01jcx9a1b2c3d4e5f6g7h8j9k1"),
    )
    .unwrap();
    std::fs::write(
        memory.vault.join("people/paul-martin.md"),
        person_file("Paul Martin", "01jcx9a1b2c3d4e5f6g7h8j9k2"),
    )
    .unwrap();
    let (ok, said) = memory.add(&server, &["--name", "Photos"], TOKEN);
    assert!(ok, "{said}");
    assert!(
        said.contains("following u1"),
        "the key's user is followed at once: {said}"
    );
    assert!(
        said.contains("1 person of the photos may be someone"),
        "{said}"
    );

    let log = memory.photos_log();
    assert!(
        log.contains("topic: photos") && log.contains("category: people"),
        "{log}"
    );
    let row = |subject: &str| {
        log.lines()
            .find(|l| l.starts_with(&format!("| {subject} ")))
            .unwrap_or_else(|| panic!("no row for {subject}: {log}"))
            .to_owned()
    };
    assert!(row("[[gaspard-lemoine]]").contains("| 42 |"), "{log}");
    assert!(
        row("Paul").contains("| 17 |"),
        "not linked before the person says: {log}"
    );
    assert!(row("Zoé").contains("| 5 |"), "{log}");
    assert!(
        !log.contains("Inès Roux") && !log.contains("ines-roux"),
        "the owner is no row: {log}"
    );

    // The proposal: add "Paul" to Paul Martin's other names.
    let vault = memory.vault_arg();
    let (ok, said) = memory.ipse(&["proposals", "list", "--vault", &vault], "");
    assert!(ok && said.contains("Est-ce Paul Martin"), "{said}");
    let id = said
        .lines()
        .find(|l| l.contains("Paul Martin"))
        .and_then(|l| l.split_whitespace().next())
        .unwrap()
        .to_owned();
    let (ok, said) = memory.ipse(&["proposals", "accept", &id, "--vault", &vault], "");
    assert!(ok, "{said}");
    let paul = std::fs::read_to_string(memory.vault.join("people/paul-martin.md")).unwrap();
    assert!(paul.contains("Paul"), "{paul}");

    // Accepted, the next sync links Paul's photos to Paul Martin, and asks nothing again.
    let said = memory.sync();
    assert!(!said.contains("may be someone"), "{said}");
    let log = memory.photos_log();
    assert!(
        log.lines()
            .any(|l| l.starts_with("| [[paul-martin]] ") && l.contains("| 17 |")),
        "{log}"
    );
    memory.assert_no_token();
}
