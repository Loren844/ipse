//! What `ipse ui --app` does in the background for the desktop app (issue #153): it
//! syncs the followed sources and tells the app, on its output, what to tell the
//! person.

// Test helpers may panic: a failed expectation is a failed test.
#![allow(clippy::expect_used, clippy::unwrap_used)]

use std::io::{BufRead as _, BufReader, Read as _, Write as _};
use std::net::TcpListener;
use std::path::{Path, PathBuf};
use std::process::{Child, ChildStdin, Command, Stdio};
use std::sync::atomic::{AtomicU16, Ordering};
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
}

impl Calendar {
    fn start() -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let url = format!(
            "http://{}/calendar/ical/private-0b5e/basic.ics",
            listener.local_addr().unwrap()
        );
        let status = Arc::new(AtomicU16::new(200));
        let answer = Arc::clone(&status);
        std::thread::spawn(move || {
            for stream in listener.incoming() {
                let Ok(mut stream) = stream else { continue };
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
        Self { url, status }
    }

    fn fail(&self) {
        self.status.store(503, Ordering::SeqCst);
    }
}

/// `ipse ui --app` running on a memory, as the desktop app runs it.
struct App {
    child: Child,
    input: Option<ChildStdin>,
    lines: mpsc::Receiver<String>,
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
        let app = Self {
            child,
            input,
            lines,
        };
        let address = app.next();
        assert!(address.starts_with("http://127.0.0.1:"), "{address}");
        app
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
    assert!(
        vault.join(".ipse/sync.toml").exists(),
        "its run is recorded"
    );
    std::thread::sleep(Duration::from_secs(1));
    assert_eq!(count(), 2, "once, then it waits for its interval");
}
