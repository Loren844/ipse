//! `ipse ui`: who may use the interface's server (decision 0012).
//!
//! Each test starts `ipse ui --no-open` on an invented memory and speaks plain HTTP to
//! it, as a browser and as other sites would.

// Test helpers may panic: a failed expectation is a failed test.
#![allow(clippy::expect_used, clippy::unwrap_used)]

use std::fmt::Write as _;
use std::io::{BufRead, BufReader, Read, Write};
use std::net::TcpStream;
use std::process::{Child, Command, Stdio};

const IPSE: &str = env!("CARGO_BIN_EXE_ipse");

/// A running `ipse ui`, stopped when dropped.
struct Ui {
    child: Child,
    /// Kept open: the server may still write to it.
    _out: BufReader<std::process::ChildStdout>,
    _dir: tempfile::TempDir,
    port: u16,
    code: String,
}

impl Drop for Ui {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

fn start() -> Ui {
    start_with(|_| {})
}

/// Starts `ipse ui` on a new memory that `fill` completes first.
fn start_with(fill: impl FnOnce(&std::path::Path)) -> Ui {
    let dir = tempfile::tempdir().unwrap();
    let vault = dir.path().join("memoire");
    let out = Command::new(IPSE)
        .args([
            "init",
            vault.to_str().unwrap(),
            "--name",
            "Inès Roux",
            "--language",
            "fr",
        ])
        .args(["--time-zone", "Europe/Paris"])
        .output()
        .unwrap();
    assert!(out.status.success());
    fill(&vault);
    let mut child = Command::new(IPSE)
        .args(["ui", "--no-open", "--vault", vault.to_str().unwrap()])
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .unwrap();
    let mut line = String::new();
    let mut out = BufReader::new(child.stdout.take().unwrap());
    out.read_line(&mut line).unwrap();
    let url = line
        .split_whitespace()
        .find(|w| w.starts_with("http://"))
        .unwrap_or_else(|| panic!("an address is printed: {line}"));
    let rest = url.strip_prefix("http://127.0.0.1:").unwrap();
    let (port, query) = rest.split_once("/login?code=").unwrap();
    Ui {
        child,
        _out: out,
        _dir: dir,
        port: port.parse().unwrap(),
        code: query.to_owned(),
    }
}

/// The status line's code, the headers and the body of one request.
fn request(ui: &Ui, method: &str, path: &str, headers: &[(&str, String)]) -> (u16, String, String) {
    let mut stream = TcpStream::connect(("127.0.0.1", ui.port)).unwrap();
    let mut text = format!("{method} {path} HTTP/1.1\r\nConnection: close\r\n");
    if !headers
        .iter()
        .any(|(name, _)| name.eq_ignore_ascii_case("host"))
    {
        let _ = write!(text, "Host: 127.0.0.1:{}\r\n", ui.port);
    }
    for (name, value) in headers {
        let _ = write!(text, "{name}: {value}\r\n");
    }
    if method == "POST" {
        text.push_str("Content-Length: 0\r\n");
    }
    text.push_str("\r\n");
    stream.write_all(text.as_bytes()).unwrap();
    let mut answer = String::new();
    stream.read_to_string(&mut answer).unwrap();
    let (head, body) = answer.split_once("\r\n\r\n").unwrap_or((&answer, ""));
    let status = head.split_whitespace().nth(1).unwrap().parse().unwrap();
    (status, head.to_owned(), body.to_owned())
}

/// Logs in with the one-time code; returns the cookie.
fn login(ui: &Ui) -> String {
    let (status, head, _) = request(ui, "GET", &format!("/login?code={}", ui.code), &[]);
    assert_eq!(status, 303, "{head}");
    assert!(head.contains("location: /"), "{head}");
    let cookie = head
        .lines()
        .find_map(|l| l.strip_prefix("set-cookie: "))
        .expect("a session cookie");
    assert!(
        cookie.contains("HttpOnly") && cookie.contains("SameSite=Strict"),
        "{cookie}"
    );
    cookie.split(';').next().unwrap().to_owned()
}

#[test]
fn the_one_time_code_opens_one_session() {
    let ui = start();
    let cookie = login(&ui);
    let with = |extra: &[(&'static str, String)]| {
        let mut headers = vec![("Cookie", cookie.clone())];
        headers.extend_from_slice(extra);
        headers
    };

    let (status, head, body) = request(&ui, "GET", "/", &with(&[]));
    assert_eq!(status, 200);
    assert!(body.contains("<div id=\"ipse\">"), "the interface's page");
    assert!(
        head.contains("content-security-policy: default-src 'self'; script-src 'self' 'sha256-"),
        "{head}"
    );
    assert!(head.contains("frame-ancestors 'none'"), "{head}");
    let (status, _, body) = request(&ui, "GET", "/history", &with(&[]));
    assert_eq!(status, 200, "every page is the interface's");
    assert!(body.contains("<div id=\"ipse\">"));

    let (status, head, body) = request(
        &ui,
        "GET",
        "/api/app",
        &with(&[("Sec-Fetch-Site", "same-origin".into())]),
    );
    assert_eq!(status, 200, "{head}");
    let info: serde_json::Value = serde_json::from_str(&body).unwrap();
    assert_eq!(
        info,
        serde_json::json!({"name": "Inès Roux", "language": "fr", "time_zone": "Europe/Paris", "waiting": 0})
    );

    // The code works once.
    let (status, head, _) = request(&ui, "GET", &format!("/login?code={}", ui.code), &[]);
    assert_eq!(status, 403);
    assert!(!head.contains("set-cookie"));
}

#[test]
fn everything_else_is_refused() {
    let ui = start();
    // Without a cookie: not even the interface's files.
    assert_eq!(request(&ui, "GET", "/", &[]).0, 403);
    assert_eq!(request(&ui, "GET", "/favicon.svg", &[]).0, 403);
    assert_eq!(request(&ui, "GET", "/api/app", &[]).0, 401);
    assert_eq!(request(&ui, "GET", "/login?code=0000", &[]).0, 403);
    assert_eq!(request(&ui, "GET", "/login", &[]).0, 403);

    let cookie = login(&ui);
    let name = cookie.split('=').next().unwrap();
    assert_eq!(
        request(
            &ui,
            "GET",
            "/api/app",
            &[("Cookie", format!("{name}=forged"))]
        )
        .0,
        401
    );
    let ok = |extra: (&'static str, String)| vec![("Cookie", cookie.clone()), extra];
    // Another site, through the person's browser.
    assert_eq!(
        request(
            &ui,
            "GET",
            "/api/app",
            &ok(("Origin", "https://evil.example".into()))
        )
        .0,
        403
    );
    assert_eq!(
        request(
            &ui,
            "GET",
            "/api/app",
            &ok(("Origin", "http://127.0.0.1:3000".into()))
        )
        .0,
        403
    );
    assert_eq!(
        request(
            &ui,
            "GET",
            "/api/app",
            &ok(("Sec-Fetch-Site", "cross-site".into()))
        )
        .0,
        403
    );
    assert_eq!(
        request(
            &ui,
            "GET",
            "/api/app",
            &ok(("Sec-Fetch-Site", "same-site".into()))
        )
        .0,
        403
    );
    // DNS rebinding: a name that is not this computer.
    assert_eq!(
        request(
            &ui,
            "GET",
            "/api/app",
            &ok(("Host", format!("evil.example:{}", ui.port)))
        )
        .0,
        421
    );
    // A change must say where it comes from, and `/api/app` changes nothing.
    assert_eq!(
        request(
            &ui,
            "POST",
            "/api/app",
            &ok(("Sec-Fetch-Site", "same-origin".into()))
        )
        .0,
        403
    );
    let origin = format!("http://127.0.0.1:{}", ui.port);
    assert_eq!(
        request(&ui, "POST", "/api/app", &ok(("Origin", origin))).0,
        405
    );
    assert_eq!(
        request(&ui, "GET", "/api/nothing", &ok(("Accept", "*/*".into()))).0,
        404
    );
}

/// GETs `path` with the session cookie; the status and the JSON answer.
fn api(ui: &Ui, cookie: &str, path: &str) -> (u16, serde_json::Value) {
    let (status, _, body) = request(ui, "GET", path, &[("Cookie", cookie.to_owned())]);
    (
        status,
        serde_json::from_str(&body).unwrap_or(serde_json::Value::Null),
    )
}

#[test]
fn the_memory_through_the_api() {
    let fixtures = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures");
    let ui = start_with(|vault| {
        for file in ["contacts/google-v3.vcf", "calendar/google.ics"] {
            let out = Command::new(IPSE)
                .args([
                    "import",
                    fixtures.join(file).to_str().unwrap(),
                    "--vault",
                    vault.to_str().unwrap(),
                ])
                .output()
                .unwrap();
            assert!(out.status.success());
        }
        // A custom category without `sensitive`: closed to AIs, and shown as such.
        let settings = vault.join(".ipse/vault.toml");
        let mut text = std::fs::read_to_string(&settings).unwrap();
        text.push_str("\n[categories.soins]\n");
        std::fs::write(&settings, text).unwrap();
        let elodie = vault.join("people/elodie-martin.md");
        let text = std::fs::read_to_string(&elodie).unwrap();
        std::fs::write(&elodie, text.replace("category: people", "category: soins")).unwrap();
    });
    let cookie = login(&ui);

    let (status, people) = api(&ui, &cookie, "/api/people");
    assert_eq!(status, 200);
    let names: Vec<&str> = people["items"]
        .as_array()
        .unwrap()
        .iter()
        .map(|p| p["name"].as_str().unwrap())
        .collect();
    assert_eq!(
        names,
        ["Élodie Martin", "Inès Roux", "Marc Leroy"],
        "by name, accents aside"
    );
    assert_eq!(people["total"], 3);
    assert_eq!(people["items"][0]["sensitive"], true);
    assert_eq!(people["items"][0]["aliases"], serde_json::json!(["Lolo"]));
    assert_eq!(people["items"][2]["sensitive"], false);

    let (_, found) = api(&ui, &cookie, "/api/people?q=l%C3%A9roy");
    assert_eq!(found["items"][0]["name"], "Marc Leroy");
    assert_eq!(found["total"], 1);
    let marc = found["items"][0]["id"].as_str().unwrap().to_owned();

    let (status, file) = api(&ui, &cookie, &format!("/api/file/{marc}"));
    assert_eq!(status, 200);
    assert_eq!(file["type"], "entity");
    let person = &file["value"];
    assert_eq!(person["name"], "Marc Leroy");
    assert_eq!(person["kind"], "person");
    assert_eq!(person["sources"], serde_json::json!(["contacts"]));
    assert_eq!(person["owner"], false);
    let events = person["events"].as_array().unwrap();
    assert_eq!(events.len(), 1);
    assert_eq!(events[0]["title"], "Dîner chez Julie");
    let dinner = events[0]["id"].as_str().unwrap().to_owned();

    let (_, file) = api(&ui, &cookie, &format!("/api/file/{dinner}"));
    assert_eq!(file["type"], "event");
    let people: Vec<_> = file["value"]["people"]
        .as_array()
        .unwrap()
        .iter()
        .map(|p| (p["name"].as_str().unwrap(), p["id"].is_string()))
        .collect();
    assert_eq!(
        people,
        [("Julie Dupont", false), ("Marc Leroy", true)],
        "a name as written, a link resolved"
    );
    assert_eq!(file["value"]["sources"], serde_json::json!(["calendar"]));

    let elodie = names_to_id(&ui, &cookie, "lolo");
    let (_, file) = api(&ui, &cookie, &format!("/api/file/{elodie}"));
    assert_eq!(file["value"]["category"], "soins");
    assert_eq!(file["value"]["sensitive"], true);

    let (_, events) = api(&ui, &cookie, "/api/events?from=2026-11-01&to=2026-12-31");
    assert!(
        events["events"]
            .as_array()
            .unwrap()
            .iter()
            .any(|e| e["title"] == "Dîner chez Julie")
    );
    assert_eq!(
        (events["from"].as_str(), events["to"].as_str()),
        (Some("2026-11-01"), Some("2026-12-31"))
    );
    assert_eq!(
        api(&ui, &cookie, "/api/events?from=2026-12-01&to=2026-11-01").0,
        400
    );
    assert_eq!(api(&ui, &cookie, "/api/events?from=demain").0, 400);

    let (status, home) = api(&ui, &cookie, "/api/home");
    assert_eq!(status, 200);
    assert_eq!(home["me"]["name"], "Inès Roux");
    assert_eq!(home["waiting"], 0);

    assert_eq!(
        api(&ui, &cookie, "/api/file/01zzzzzzzzzzzzzzzzzzzzzzzz").0,
        404
    );
    assert_eq!(api(&ui, &cookie, "/api/file/not-an-id").0, 404);
}

fn names_to_id(ui: &Ui, cookie: &str, query: &str) -> String {
    let (_, found) = api(ui, cookie, &format!("/api/people?q={query}"));
    found["items"][0]["id"].as_str().unwrap().to_owned()
}
