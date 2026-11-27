//! What `ipse ui --app` does in the background for the desktop app (issue #153): it
//! syncs the followed sources and tells the app, on its output, what to tell the
//! person.

// Test helpers may panic: a failed expectation is a failed test.
#![allow(clippy::expect_used, clippy::unwrap_used)]

use std::io::{BufRead as _, BufReader, Read as _, Write as _};
use std::net::TcpListener;
use std::path::{Path, PathBuf};
use std::process::{Child, ChildStdin, Command, Stdio};
use std::sync::atomic::{AtomicBool, AtomicU16, Ordering};
use std::sync::{Arc, mpsc};
use std::time::Duration;

const IPSE: &str = env!("CARGO_BIN_EXE_ipse");

/// Longer than any sync of a test, short enough for a test that hangs to fail.
const PATIENCE: Duration = Duration::from_secs(30);

/// A calendar on this computer, answering every request with `status`: 200 sends a
/// small calendar, anything else fails.
struct Calendar {
    url: String,
    status: Arc<AtomicU16>,
    /// Answers only after [`SLOW`], as a slow server does.
    slow: Arc<AtomicBool>,
}

/// How long a slow calendar takes to answer.
const SLOW: Duration = Duration::from_secs(4);

impl Calendar {
    fn start() -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let url = format!(
            "http://{}/calendar/ical/private-0b5e/basic.ics",
            listener.local_addr().unwrap()
        );
        let status = Arc::new(AtomicU16::new(200));
        let slow = Arc::new(AtomicBool::new(false));
        let answer = Arc::clone(&status);
        let late = Arc::clone(&slow);
        std::thread::spawn(move || {
            for stream in listener.incoming() {
                let Ok(mut stream) = stream else { continue };
                if late.load(Ordering::SeqCst) {
                    std::thread::sleep(SLOW);
                }
                let mut reader = BufReader::new(stream.try_clone().unwrap());
                loop {
                    let mut line = String::new();
                    if reader.read_line(&mut line).unwrap_or(0) == 0 || line == "\r\n" {
                        break;
                    }
                }
                let body = "BEGIN:VCALENDAR\r\nVERSION:2.0\r\nBEGIN:VEVENT\r\nUID:yoga@example.org\r\nSUMMARY:Yoga\r\nDTSTART:20261124T180000Z\r\nDTEND:20261124T190000Z\r\nEND:VEVENT\r\nEND:VCALENDAR\r\n";
                let response = match answer.load(Ordering::SeqCst) {
                    200 => format!(
                        "HTTP/1.1 200 OK\r\nContent-Type: text/calendar\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                        body.len()
                    ),
                    code => format!(
                        "HTTP/1.1 {code} Nope\r\nContent-Length: 0\r\nConnection: close\r\n\r\n"
                    ),
                };
                let _ = stream.write_all(response.as_bytes());
            }
        });
        Self { url, status, slow }
    }

    fn fail(&self) {
        self.status.store(503, Ordering::SeqCst);
    }

    fn slow_down(&self) {
        self.slow.store(true, Ordering::SeqCst);
    }
}

/// `ipse ui --app` running on a memory, as the desktop app runs it.
struct App {
    child: Child,
    input: Option<ChildStdin>,
    lines: mpsc::Receiver<String>,
    /// The interface's port.
    port: u16,
    /// The one-time address that opens a session.
    login: String,
}

impl App {
    fn start(vault: &Path, secrets: &Path) -> Self {
        let mut child = Command::new(IPSE)
            .args(["ui", "--app", "--vault", vault.to_str().unwrap()])
            .env("IPSE_TEST_SECRETS", secrets)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .unwrap();
        let input = child.stdin.take();
        let out = child.stdout.take().unwrap();
        let (sent, lines) = mpsc::channel();
        std::thread::spawn(move || {
            for line in BufReader::new(out).lines() {
                let Ok(line) = line else { break };
                if sent.send(line).is_err() {
                    break;
                }
            }
        });
        let address = lines.recv_timeout(PATIENCE).expect("ipse says its address");
        let rest = address
            .strip_prefix("http://127.0.0.1:")
            .unwrap_or_else(|| panic!("{address}"));
        let (port, login) = rest.split_once('/').unwrap();
        Self {
            child,
            input,
            lines,
            port: port.parse().unwrap(),
            login: format!("/{login}"),
        }
    }

    /// Sends one request to the interface; its status and body.
    fn http(&self, method: &str, path: &str, cookie: &str, body: &str) -> (u16, String) {
        http(self.port, method, path, cookie, body)
    }

    /// Opens a session with the one-time address; its cookie.
    fn session(&self) -> String {
        let mut stream = std::net::TcpStream::connect(("127.0.0.1", self.port)).unwrap();
        let text = format!(
            "GET {} HTTP/1.1\r\nHost: 127.0.0.1:{}\r\nConnection: close\r\n\r\n",
            self.login, self.port
        );
        stream.write_all(text.as_bytes()).unwrap();
        let mut answer = String::new();
        stream.read_to_string(&mut answer).unwrap();
        answer
            .lines()
            .find_map(|l| l.strip_prefix("set-cookie: "))
            .and_then(|c| c.split(';').next())
            .unwrap_or_else(|| panic!("a session: {answer}"))
            .to_owned()
    }

    /// How long the interface takes to answer a read.
    fn answers_in(&self, cookie: &str) -> Duration {
        let start = std::time::Instant::now();
        let (status, body) = self.http("GET", "/api/app", cookie, "");
        assert_eq!(status, 200, "{body}");
        start.elapsed()
    }

    /// The next line of the output.
    fn next(&self) -> String {
        self.lines
            .recv_timeout(PATIENCE)
            .expect("ipse says something in time")
    }

    /// The next event, read as JSON.
    fn event(&self) -> serde_json::Value {
        let line = self.next();
        serde_json::from_str(&line).unwrap_or_else(|_| panic!("an event: {line}"))
    }

    fn ask(&mut self, line: &str) {
        let input = self.input.as_mut().unwrap();
        writeln!(input, "{line}").unwrap();
        input.flush().unwrap();
    }
}

impl Drop for App {
    fn drop(&mut self) {
        drop(self.input.take());
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

fn ipse(args: &[&str], secrets: &Path) {
    let out = Command::new(IPSE)
        .args(args)
        .env("IPSE_TEST_SECRETS", secrets)
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "{args:?}: {}",
        String::from_utf8_lossy(&out.stderr)
    );
}

fn memory(dir: &Path, secrets: &Path, demo: bool) -> PathBuf {
    let vault = dir.join("memoire");
    let path = vault.to_str().unwrap();
    if demo {
        ipse(&["init", path, "--demo"], secrets);
    } else {
        ipse(
            &["init", path, "--name", "Inès Roux", "--language", "fr"],
            secrets,
        );
    }
    vault
}

#[test]
fn syncing_now_says_when_a_source_fails() {
    let dir = tempfile::tempdir().unwrap();
    let secrets = dir.path().join("secrets.json");
    let vault = memory(dir.path(), &secrets, false);
    let calendar = Calendar::start();
    ipse(
        &[
            "calendar",
            "subscribe",
            &calendar.url,
            "--name",
            "Perso",
            "--every",
            "1h",
            "--vault",
            vault.to_str().unwrap(),
        ],
        &secrets,
    );
    let mut app = App::start(&vault, &secrets);

    calendar.fail();
    app.ask("sync");
    let event = app.event();
    assert_eq!(event["event"], "sync_failed", "{event}");
    assert_eq!(event["source"], "Perso");

    let runs = std::fs::read_to_string(vault.join(".ipse/connectors/runs.jsonl")).unwrap();
    assert_eq!(
        runs.lines().count(),
        2,
        "the subscription's sync, then this one"
    );
    assert!(
        !runs.contains("private-0b5e"),
        "the secret link is never written"
    );
}

#[test]
fn a_new_proposal_is_told_to_the_app() {
    let dir = tempfile::tempdir().unwrap();
    let secrets = dir.path().join("secrets.json");
    let vault = memory(dir.path(), &secrets, true);
    // One of the demo's proposals waits outside, then arrives while the app runs, as
    // an AI's proposal would.
    let proposals = vault.join(".ipse/proposals");
    let mut waiting = None;
    for entry in std::fs::read_dir(&proposals).unwrap() {
        let path = entry.unwrap().path();
        let mut text = String::new();
        std::fs::File::open(&path)
            .unwrap()
            .read_to_string(&mut text)
            .unwrap();
        if text.contains("\"state\": \"waiting\"") {
            waiting = Some(path);
            break;
        }
    }
    let waiting = waiting.expect("the demo has proposals waiting");
    let aside = dir.path().join("aside.json");
    std::fs::rename(&waiting, &aside).unwrap();

    let app = App::start(&vault, &secrets);
    // Let the watcher count what waits before the proposal arrives.
    std::thread::sleep(Duration::from_secs(1));
    std::fs::rename(&aside, &waiting).unwrap();
    let event = app.event();
    assert_eq!(event["event"], "waiting", "{event}");
    assert_eq!(event["new"], 1);
    assert!(event["count"].as_u64().unwrap() > 1);
}

#[test]
fn a_source_that_is_due_syncs_on_its_own() {
    let dir = tempfile::tempdir().unwrap();
    let secrets = dir.path().join("secrets.json");
    let vault = memory(dir.path(), &secrets, false);
    let calendar = Calendar::start();
    ipse(
        &[
            "calendar",
            "subscribe",
            &calendar.url,
            "--name",
            "Perso",
            "--vault",
            vault.to_str().unwrap(),
        ],
        &secrets,
    );
    // As if it had never run: it is due at once.
    std::fs::remove_file(vault.join(".ipse/sync.toml")).unwrap();
    let runs = vault.join(".ipse/connectors/runs.jsonl");
    let count = || std::fs::read_to_string(&runs).unwrap().lines().count();
    assert_eq!(count(), 1);

    let _app = App::start(&vault, &secrets);
    let waited = std::time::Instant::now();
    while count() < 2 {
        assert!(waited.elapsed() < PATIENCE, "the due source synced");
        std::thread::sleep(Duration::from_millis(100));
    }
    // The run is logged before the sync state is saved: wait for that too.
    while !vault.join(".ipse/sync.toml").exists() {
        assert!(waited.elapsed() < PATIENCE, "its run is recorded");
        std::thread::sleep(Duration::from_millis(100));
    }
    std::thread::sleep(Duration::from_secs(1));
    assert_eq!(count(), 2, "once, then it waits for its interval");
}

/// Sends one request to the interface on `port`; its status and body.
fn http(port: u16, method: &str, path: &str, cookie: &str, body: &str) -> (u16, String) {
    let mut stream = std::net::TcpStream::connect(("127.0.0.1", port)).unwrap();
    let text = format!(
        "{method} {path} HTTP/1.1\r\nHost: 127.0.0.1:{port}\r\nConnection: close\r\nCookie: {cookie}\r\nOrigin: http://127.0.0.1:{port}\r\nContent-Type: application/json\r\nContent-Length: {}\r\n\r\n{body}",
        body.len()
    );
    stream.write_all(text.as_bytes()).unwrap();
    let mut answer = String::new();
    stream.read_to_string(&mut answer).unwrap();
    let (head, body) = answer.split_once("\r\n\r\n").unwrap_or((&answer, ""));
    let status = head.split_whitespace().nth(1).unwrap().parse().unwrap();
    (status, body.to_owned())
}

/// A memory following the calendar `url`, as "Perso".
fn following(dir: &Path, secrets: &Path, url: &str) -> PathBuf {
    let vault = memory(dir, secrets, false);
    ipse(
        &[
            "calendar",
            "subscribe",
            url,
            "--name",
            "Perso",
            "--every",
            "1h",
            "--vault",
            vault.to_str().unwrap(),
        ],
        secrets,
    );
    vault
}

#[test]
fn the_interface_answers_while_a_slow_source_syncs() {
    let dir = tempfile::tempdir().unwrap();
    let secrets = dir.path().join("secrets.json");
    let calendar = Calendar::start();
    let vault = following(dir.path(), &secrets, &calendar.url);
    let mut app = App::start(&vault, &secrets);
    let cookie = app.session();

    calendar.slow_down();
    app.ask("sync");
    // The sync has started, and waits for the calendar.
    std::thread::sleep(Duration::from_millis(500));
    let took = app.answers_in(&cookie);
    assert!(took < SLOW / 2, "answered in {took:?}");
}

#[test]
fn the_interface_answers_while_the_person_syncs_a_slow_source() {
    let dir = tempfile::tempdir().unwrap();
    let secrets = dir.path().join("secrets.json");
    let calendar = Calendar::start();
    let vault = following(dir.path(), &secrets, &calendar.url);
    let app = App::start(&vault, &secrets);
    let cookie = app.session();
    let instance = std::fs::read_to_string(vault.join(".ipse/connectors.toml"))
        .unwrap()
        .lines()
        .find_map(|l| l.strip_prefix("id = "))
        .map(|id| id.trim_matches('"').to_owned())
        .expect("the source is followed");

    calendar.slow_down();
    let syncing = {
        let (port, cookie) = (app.port, cookie.clone());
        std::thread::spawn(move || {
            http(
                port,
                "POST",
                "/api/sources/sync",
                &cookie,
                &format!("{{\"instance\":\"{instance}\"}}"),
            )
        })
    };
    std::thread::sleep(Duration::from_millis(500));
    let took = app.answers_in(&cookie);
    assert!(took < SLOW / 2, "answered in {took:?}");
    let (status, body) = syncing.join().unwrap();
    assert_eq!(status, 200, "{body}");
    assert!(body.contains("\"synced\""), "{body}");
}
