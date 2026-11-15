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
    /// The memory folder.
    vault: std::path::PathBuf,
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
        vault,
        port: port.parse().unwrap(),
        code: query.to_owned(),
    }
}

/// The status line's code, the headers and the body of one request.
fn request(ui: &Ui, method: &str, path: &str, headers: &[(&str, String)]) -> (u16, String, String) {
    send(ui, method, path, headers, "")
}

/// Sends `body` to `path`; the status, the head and the body of the answer.
fn send(
    ui: &Ui,
    method: &str,
    path: &str,
    headers: &[(&str, String)],
    body: &str,
) -> (u16, String, String) {
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
        let _ = write!(text, "Content-Length: {}\r\n", body.len());
    }
    text.push_str("\r\n");
    text.push_str(body);
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

/// POSTs `body` as JSON to `path`, as the interface does; the status and the answer.
fn post(ui: &Ui, cookie: &str, path: &str, body: &str) -> (u16, serde_json::Value) {
    let headers = [
        ("Cookie", cookie.to_owned()),
        ("Origin", format!("http://127.0.0.1:{}", ui.port)),
        ("Content-Type", "application/json".to_owned()),
    ];
    let (status, _, answer) = send(ui, "POST", path, &headers, body);
    (
        status,
        serde_json::from_str(&answer).unwrap_or(serde_json::Value::Null),
    )
}

#[test]
fn the_review_through_the_api() {
    let ui = start_with(|vault| {
        // Proposals as an AI leaves them, in the format of `docs/spec/vault.md` §12.
        let proposal = |id: &str, change: &str| {
            let json = format!(
                r#"{{"format": 1, "id": "{id}", "by": "client:claude", "created": "2026-11-14T19:02:11+01:00",
                "reason": "Dit dans la conversation.", "categories": ["people"],
                "changes": [{change}], "status": {{"state": "waiting"}}}}"#
            );
            std::fs::create_dir_all(vault.join(".ipse/proposals")).unwrap();
            std::fs::write(vault.join(format!(".ipse/proposals/{id}.json")), json).unwrap();
        };
        let fact = |text: &str| {
            format!(r#"{{"kind": "add_fact", "file": "people/ines-roux.md", "text": "{text}"}}"#)
        };
        proposal("01jd3k8m2p4q6r8t0v2w4y6z8a", &fact("Joue du violoncelle."));
        proposal("01jd3k9m2p4q6r8t0v2w4y6z8a", &fact("Parle italien."));
        proposal("01jd3kam2p4q6r8t0v2w4y6z8a", &fact("Court le dimanche."));
        proposal("01jd3kbm2p4q6r8t0v2w4y6z8a", &fact("Lit des polars."));
    });
    let cookie = login(&ui);
    let owner = ui.vault.join("people/ines-roux.md");
    let says = |text: &str| std::fs::read_to_string(&owner).unwrap().contains(text);

    let (status, inbox) = api(&ui, &cookie, "/api/proposals");
    assert_eq!(status, 200);
    let waiting = inbox["waiting"].as_array().unwrap();
    assert_eq!(waiting.len(), 4);
    assert_eq!(
        waiting[0]["id"], "01jd3kbm2p4q6r8t0v2w4y6z8a",
        "newest first"
    );
    assert_eq!(waiting[0]["by"]["name"], "claude");
    assert_eq!(waiting[0]["touches"][0], "Inès Roux");
    assert_eq!(api(&ui, &cookie, "/api/app").1["waiting"], 4);

    let (status, one) = api(&ui, &cookie, "/api/proposals/01jd3k8m");
    assert_eq!(status, 200);
    let file = &one["files"][0];
    assert_eq!(file["name"], "Inès Roux");
    assert!(file["id"].is_string(), "the file can be opened");
    assert_eq!(file["changes"][0]["what"], "fact");
    assert_eq!(file["changes"][0]["before"], serde_json::Value::Null);
    assert_eq!(file["changes"][0]["after"], "Joue du violoncelle.");
    assert!(!one.to_string().contains("f-"), "never a fact id");
    assert_eq!(api(&ui, &cookie, "/api/proposals/01zzzzzz").0, 404);

    // A decision must come from the interface, as JSON.
    let accept = "/api/proposals/01jd3k8m/accept";
    let (status, _, _) = request(&ui, "POST", accept, &[("Cookie", cookie.clone())]);
    assert_eq!(status, 403);
    let from_here = vec![
        ("Cookie", cookie.clone()),
        ("Origin", format!("http://127.0.0.1:{}", ui.port)),
    ];
    assert_eq!(request(&ui, "POST", accept, &from_here).0, 415);
    assert!(!says("violoncelle"));

    let (status, decided) = post(&ui, &cookie, accept, "");
    assert_eq!(status, 200, "{decided}");
    assert_eq!(decided["decided"][0]["status"], "accepted");
    assert!(says("- Joue du violoncelle. ^f-"));
    let (status, again) = post(&ui, &cookie, accept, "");
    assert_eq!(status, 409, "already accepted: {again}");

    let refuse = "/api/proposals/01jd3k9m/refuse";
    assert_eq!(post(&ui, &cookie, refuse, "{not json").0, 409);
    let (status, _) = post(&ui, &cookie, refuse, r#"{"comment": "Pas vrai"}"#);
    assert_eq!(status, 200);
    assert!(!says("italien"));
    let (_, refused) = api(&ui, &cookie, "/api/proposals/01jd3k9m");
    assert_eq!(refused["line"]["status"], "refused");
    assert_eq!(refused["comment"], "Pas vrai");
    assert_eq!(
        refused["files"][0]["changes"][0]["after"], "Parle italien.",
        "a decided proposal still says what it held"
    );

    // Several in one action, each still its own proposal.
    let (status, group) = post(
        &ui,
        &cookie,
        "/api/proposals/accept",
        r#"{"ids": ["01jd3kam", "01jd3kbm"]}"#,
    );
    assert_eq!(status, 200, "{group}");
    assert_eq!(group["decided"].as_array().unwrap().len(), 2);
    assert!(says("Court le dimanche.") && says("Lit des polars."));

    // Undoing the first would lose the later changes to the same file: it says so.
    let (status, refused) = post(&ui, &cookie, "/api/proposals/01jd3k8m/undo", "");
    assert_eq!(status, 409);
    assert!(refused["error"].is_string(), "{refused}");
    assert!(says("violoncelle"));
    let (status, _) = post(&ui, &cookie, "/api/proposals/01jd3kbm/undo", "");
    assert_eq!(status, 200);
    assert!(!says("polars") && says("Court le dimanche."));

    let (_, inbox) = api(&ui, &cookie, "/api/proposals");
    assert!(inbox["waiting"].as_array().unwrap().is_empty());
    let decided: Vec<&str> = inbox["decided"]
        .as_array()
        .unwrap()
        .iter()
        .map(|p| p["status"].as_str().unwrap())
        .collect();
    assert_eq!(decided.len(), 4);
    assert!(decided.contains(&"undone") && decided.contains(&"refused"));
    assert_eq!(api(&ui, &cookie, "/api/app").1["waiting"], 0);
    assert_eq!(
        post(&ui, &cookie, "/api/proposals/01jd3k8m/nothing", "").0,
        404
    );
}
