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
    let mut info: serde_json::Value = serde_json::from_str(&body).unwrap();
    // The owner's file, for the menu under their name: an id made at init.
    let me = info["me"].take();
    assert!(me.as_str().is_some_and(|id| id.len() == 26), "{me}");
    assert_eq!(
        info,
        serde_json::json!({"name": "Inès Roux", "me": null, "language": "fr", "time_zone": "Europe/Paris", "waiting": 0, "failing": 0})
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

/// The category `name` of the AIs' screen.
fn category(view: &serde_json::Value, name: &str) -> serde_json::Value {
    view["categories"]
        .as_array()
        .unwrap()
        .iter()
        .find(|c| c["name"] == name)
        .cloned()
        .unwrap_or_default()
}

/// True when the AI `id` of the AIs' screen reads `etudes`.
fn reads(view: &serde_json::Value, id: &str) -> bool {
    view["ais"]
        .as_array()
        .unwrap()
        .iter()
        .find(|a| a["id"] == id)
        .unwrap()["read"]
        .as_array()
        .unwrap()
        .contains(&"etudes".into())
}

#[test]
fn the_ais_and_their_rights_through_the_api() {
    let ui = start_with(|vault| {
        let out = Command::new(IPSE)
            .args(["client", "add", "Claude Desktop", "--propose", "people"])
            .args(["--vault", vault.to_str().unwrap()])
            .output()
            .unwrap();
        assert!(out.status.success());
        std::fs::create_dir_all(vault.join(".ipse/access")).unwrap();
        std::fs::write(
            vault.join(".ipse/access/2026-11.jsonl"),
            "{\"at\":\"2026-11-12T20:41:03+01:00\",\"client\":\"claude-desktop\",\"tool\":\"now\",\"categories\":[],\"results\":0}\n",
        )
        .unwrap();
        let settings = vault.join(".ipse/vault.toml");
        let mut text = std::fs::read_to_string(&settings).unwrap();
        text.push_str("\n[categories.club]\nname = \"Club\"\n");
        std::fs::write(&settings, text).unwrap();
    });
    let cookie = login(&ui);
    let clients = || std::fs::read_to_string(ui.vault.join(".ipse/clients.toml")).unwrap();

    let (status, view) = api(&ui, &cookie, "/api/ais");
    assert_eq!(status, 200);
    let ai = &view["ais"][0];
    assert_eq!(ai["name"], "Claude Desktop");
    assert_eq!(ai["last_access"], "2026-11-12T20:41:03+01:00");
    assert_eq!(ai["propose"], serde_json::json!(["people"]));
    let club = category(&view, "club");
    assert_eq!(club["sensitive"], true, "left out: sensitive");
    assert_eq!(club["implicit"], true);
    assert_eq!(club["label"], "Club");
    assert_eq!(category(&view, "health")["sensitive"], true);
    assert_eq!(category(&view, "health")["custom"], false);

    // Connecting: the code is shown once, never written.
    let (status, connected) = post(
        &ui,
        &cookie,
        "/api/ais",
        r#"{"name": "LM Studio", "local": true}"#,
    );
    assert_eq!(status, 200, "{connected}");
    let code = connected["code"].as_str().unwrap();
    assert!(code.starts_with("ipse_"));
    assert!(!clients().contains(code));
    assert!(clients().contains("id = \"lm-studio\"") && clients().contains("local = true"));
    assert!(std::path::Path::new(connected["program"].as_str().unwrap()).is_file());
    assert!(std::path::Path::new(connected["vault"].as_str().unwrap()).is_dir());
    assert_eq!(post(&ui, &cookie, "/api/ais", r#"{"name": "  "}"#).0, 409);

    // Opening a sensitive category to one AI, then closing it.
    let rights = "/api/ais/lm-studio/rights";
    let (status, _) = post(
        &ui,
        &cookie,
        rights,
        r#"{"read": ["people", "health"], "propose": ["people"]}"#,
    );
    assert_eq!(status, 200);
    assert!(clients().contains("read = [\"people\", \"health\"]\npropose = [\"people\"]"));
    let (status, refused) = post(
        &ui,
        &cookie,
        rights,
        r#"{"read": ["people"], "propose": ["health"]}"#,
    );
    assert_eq!(status, 409, "proposes only where it reads: {refused}");
    assert_eq!(
        post(
            &ui,
            &cookie,
            rights,
            r#"{"read": ["sport"], "propose": []}"#
        )
        .0,
        409
    );

    // A category of the person's own, opened to the AIs they choose.
    let (status, view) = post(
        &ui,
        &cookie,
        "/api/categories",
        r#"{"label": "Études", "sensitive": false, "open_to": ["claude-desktop"]}"#,
    );
    assert_eq!(status, 200, "{view}");
    let etudes = category(&view, "etudes");
    assert_eq!(
        (&etudes["label"], &etudes["sensitive"], &etudes["implicit"]),
        (&"Études".into(), &false.into(), &false.into())
    );
    assert!(reads(&view, "claude-desktop") && !reads(&view, "lm-studio"));

    let (status, view) = post(&ui, &cookie, "/api/ais/lm-studio/remove", "");
    assert_eq!(status, 200);
    assert_eq!(view["ais"].as_array().unwrap().len(), 1);
    assert!(!clients().contains("lm-studio"));
    assert_eq!(post(&ui, &cookie, "/api/ais/lm-studio/remove", "").0, 409);
}

/// Imports `file` of the test fixtures into `vault`.
fn import(vault: &std::path::Path, file: &str) {
    let fixtures = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures");
    let out = Command::new(IPSE)
        .args(["import", fixtures.join(file).to_str().unwrap()])
        .args(["--vault", vault.to_str().unwrap()])
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
}

/// The segment of the journal `source` wrote last.
fn last_segment(vault: &std::path::Path, source: &str) -> std::path::PathBuf {
    let mut segments: Vec<_> = walk(&vault.join("journal").join(source))
        .into_iter()
        .filter(|p| p.extension().is_some_and(|e| e == "jsonl"))
        .collect();
    segments.sort();
    segments.pop().unwrap()
}

fn walk(dir: &std::path::Path) -> Vec<std::path::PathBuf> {
    let mut found = Vec::new();
    for entry in std::fs::read_dir(dir).unwrap() {
        let path = entry.unwrap().path();
        if path.is_dir() {
            found.extend(walk(&path));
        } else {
            found.push(path);
        }
    }
    found
}

#[test]
fn the_history_through_the_api() {
    let ui = start_with(|vault| {
        import(vault, "contacts/google-v3.vcf");
        // A crash in the middle of a write leaves a torn last line; the next import
        // that writes to that month cuts it off.
        let segment = last_segment(vault, "contacts");
        let mut text = std::fs::read_to_string(&segment).unwrap();
        text.push_str("{\"id\":\"01torn");
        std::fs::write(&segment, text).unwrap();
        import(vault, "contacts/apple-v4.vcf");
        std::fs::create_dir_all(vault.join(".ipse/access")).unwrap();
        let call = |at: &str, categories: &str| {
            format!(
                "{{\"at\":\"{at}\",\"client\":\"claude\",\"tool\":\"search\",\"categories\":[{categories}],\"results\":2}}\n"
            )
        };
        let today = jiff::Zoned::now()
            .with_time_zone(jiff::tz::TimeZone::get("Europe/Paris").unwrap())
            .date();
        let month = today.strftime("%Y-%m").to_string();
        std::fs::write(
            vault.join(format!(".ipse/access/{month}.jsonl")),
            call(&format!("{today}T09:00:00+01:00"), "\"people\"")
                + &call(&format!("{today}T10:00:00+01:00"), "\"people\", \"agenda\""),
        )
        .unwrap();
    });
    let cookie = login(&ui);

    let (status, history) = api(&ui, &cookie, "/api/history");
    assert_eq!(status, 200);
    let changes = history["changes"].as_array().unwrap();
    assert_eq!(changes[0]["by"]["kind"], "source");
    assert_eq!(changes[0]["by"]["name"], "contacts");
    assert_eq!(changes[0]["summary"], "Import contacts");
    assert!(changes[0]["at"].as_str().unwrap().contains('T'));
    assert!(
        changes[0]["files"]
            .as_array()
            .unwrap()
            .iter()
            .any(|f| f["id"].is_string()),
        "the files touched can be opened"
    );

    let (_, runs) = api(&ui, &cookie, "/api/runs");
    let runs = runs["runs"].as_array().unwrap();
    assert_eq!(runs.len(), 2);
    assert_eq!(
        (&runs[0]["instance"], &runs[0]["mode"], &runs[0]["outcome"]),
        (&"contacts".into(), &"import".into(), &"done".into())
    );
    assert_eq!(runs[0]["repaired"], 1, "the torn line is noted");
    assert_eq!(runs[1]["repaired"], 0);
    assert!(runs[1]["added"].as_u64().unwrap() > 0);
    let text = serde_json::to_string(&runs).unwrap();
    assert!(
        !text.contains("01torn"),
        "never the content of a repaired line"
    );
    let (status, diagnostics) = api(&ui, &cookie, "/api/runs/contacts/diagnostics");
    assert_eq!(status, 200);
    assert!(diagnostics["lines"].is_array());
    assert_eq!(
        api(&ui, &cookie, "/api/runs/..%2Fclients/diagnostics").0,
        404
    );

    let (_, accesses) = api(&ui, &cookie, "/api/accesses");
    let day = &accesses["days"][0];
    assert_eq!(
        (&day["ai"], &day["calls"], &day["results"]),
        (&"claude".into(), &2.into(), &4.into())
    );
    assert_eq!(
        day["categories"],
        serde_json::json!([{"name": "agenda", "calls": 1}, {"name": "people", "calls": 2}])
    );

    // Undoing the last import: the files it added go away, as a new change.
    let marie = changes[0]["id"].as_str().unwrap().to_owned();
    let before: Vec<String> = std::fs::read_dir(ui.vault.join("people"))
        .unwrap()
        .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
        .collect();
    let (status, undone) = post(&ui, &cookie, &format!("/api/history/{marie}/undo"), "");
    assert_eq!(status, 200, "{undone}");
    assert_eq!(undone["changed"], true);
    let after = std::fs::read_dir(ui.vault.join("people")).unwrap().count();
    assert!(after < before.len(), "{before:?}");
    let (_, history) = api(&ui, &cookie, "/api/history");
    assert_eq!(history["changes"][0]["by"]["kind"], "person");
    assert_eq!(history["changes"][0]["summary"], "Undo a change");
    // The files it removed are named as they were, not by their file name.
    let names: Vec<&str> = history["changes"][0]["files"]
        .as_array()
        .unwrap()
        .iter()
        .map(|f| f["name"].as_str().unwrap())
        .collect();
    assert!(names.contains(&"Ana Silva"), "{names:?}");
    assert!(history["changes"][0]["files"][0]["id"].is_null());
    assert_eq!(
        history["changes"][1]["id"],
        marie.as_str(),
        "history is never rewritten"
    );
    assert_eq!(post(&ui, &cookie, "/api/history/0000/undo", "").0, 409);
}

#[test]
fn the_summaries_through_the_api() {
    let ui = start_with(|vault| {
        import(vault, "calendar/google.ics");
        let json = r#"{"format": 1, "id": "01jd3k8m2p4q6r8t0v2w4y6z8a", "by": "client:claude", "created": "2026-11-15T09:00:00+01:00",
            "reason": "Résumé de samedi.", "categories": ["summaries"],
            "changes": [{"kind": "write_summary", "file": "summaries/day/2026/2026-11-14.md", "period": "day", "start": "2026-11-14", "text": "Dîner chez Julie."}],
            "status": {"state": "waiting"}}"#;
        std::fs::create_dir_all(vault.join(".ipse/proposals")).unwrap();
        std::fs::write(
            vault.join(".ipse/proposals/01jd3k8m2p4q6r8t0v2w4y6z8a.json"),
            json,
        )
        .unwrap();
    });
    let cookie = login(&ui);

    let (status, empty) = api(&ui, &cookie, "/api/summaries");
    assert_eq!(status, 200);
    assert_eq!(empty["items"], serde_json::json!([]));
    assert_eq!(empty["waiting"], 1, "one waits in the inbox");
    assert_eq!(api(&ui, &cookie, "/api/summaries?period=decade").0, 400);

    let accept = "/api/proposals/01jd3k8m/accept";
    assert_eq!(post(&ui, &cookie, accept, "").0, 200);
    let (_, days) = api(&ui, &cookie, "/api/summaries?period=day");
    let day = &days["items"][0];
    assert_eq!(
        (&day["label"], &day["text"]),
        (&"2026-11-14".into(), &"Dîner chez Julie.".into())
    );
    assert_eq!(day["events"][0]["name"], "Dîner chez Julie");
    assert!(
        day["events"][0]["id"].is_string(),
        "the event can be opened"
    );
    assert_eq!(days["waiting"], 0);

    let (_, agenda) = api(&ui, &cookie, "/api/events?from=2026-11-14&to=2026-11-14");
    assert_eq!(
        agenda["summaries"],
        serde_json::json!([{"day": "2026-11-14", "text": "Dîner chez Julie."}])
    );
    assert_eq!(
        api(&ui, &cookie, "/api/home").1["latest"]["label"],
        "2026-11-14"
    );

    // The person corrects it: their own change, in the file and the history.
    let (status, corrected) = post(
        &ui,
        &cookie,
        "/api/summaries/day/2026-11-14",
        r#"{"text": "Dîner chez Julie, très drôle."}"#,
    );
    assert_eq!(status, 200, "{corrected}");
    assert_eq!(corrected["text"], "Dîner chez Julie, très drôle.");
    let file = std::fs::read_to_string(ui.vault.join("summaries/day/2026/2026-11-14.md")).unwrap();
    assert!(file.contains("\n## Résumé\n\nDîner chez Julie, très drôle.\n\n## Événements\n"));
    let (_, history) = api(&ui, &cookie, "/api/history");
    assert_eq!(history["changes"][0]["by"]["kind"], "person");
    assert_eq!(history["changes"][0]["summary"], "Correct a summary");

    let correct = |path: &str, body: &str| post(&ui, &cookie, path, body).0;
    assert_eq!(
        correct("/api/summaries/day/2026-11-14", r#"{"text": "  "}"#),
        409
    );
    assert_eq!(
        correct("/api/summaries/week/2026-11-10", r#"{"text": "Non."}"#),
        409
    );
    assert_eq!(
        correct("/api/summaries/day/2026-11-01", r#"{"text": "Rien."}"#),
        409
    );
}

#[test]
fn the_questions_through_the_api() {
    let ui = start_with(|vault| {
        std::fs::write(
            vault.join("people/jo.md"),
            "---\nid: 01jcx8m2p4q6r8t0v2w4y6z801\ntype: person\ncategory: people\ncreated: 2026-01-10T09:00:00+01:00\nupdated: 2026-01-10T09:00:00+01:00\nname: Jo plombier\nphones:\n- +33 6 00 00 00 01\n---\n",
        )
        .unwrap();
        std::fs::write(
            vault.join("people/lou.md"),
            "---\nid: 01jcx8m2p4q6r8t0v2w4y6z802\ntype: person\ncategory: people\ncreated: 2026-01-10T09:00:00+01:00\nupdated: 2026-01-10T09:00:00+01:00\nname: Lou\n---\n",
        )
        .unwrap();
    });
    let cookie = login(&ui);

    let (status, view) = api(&ui, &cookie, "/api/questions");
    assert_eq!(status, 200, "{view}");
    let questions = view["questions"].as_array().unwrap();
    assert_eq!(questions.len(), 2);
    let jo = questions
        .iter()
        .find(|q| q["name"] == "Jo plombier")
        .unwrap();
    assert_eq!(jo["phones"], serde_json::json!(["+33 6 00 00 00 01"]));
    assert_eq!(jo["person"], "01jcx8m2p4q6r8t0v2w4y6z801");
    assert_eq!(api(&ui, &cookie, "/api/app").1["waiting"], 2);
    let (_, home) = api(&ui, &cookie, "/api/home");
    assert_eq!(
        (&home["waiting"], &home["questions"]),
        (&0.into(), &2.into()),
        "{home}"
    );

    let jo_id = jo["id"].as_str().unwrap().to_owned();
    let answer = format!("/api/questions/{jo_id}/answer");
    assert_eq!(post(&ui, &cookie, &answer, r#"{"name": " "}"#).0, 409);
    let (status, after) = post(
        &ui,
        &cookie,
        &answer,
        r#"{"name": "Jo Lambert", "word": "plombier"}"#,
    );
    assert_eq!(status, 200, "{after}");
    assert_eq!(after["questions"].as_array().unwrap().len(), 1);
    let jo_file = std::fs::read_to_string(ui.vault.join("people/jo.md")).unwrap();
    assert!(jo_file.contains("name: Jo Lambert\n"), "{jo_file}");
    assert!(jo_file.contains("aliases:\n  - Jo plombier\n"), "{jo_file}");
    let me = std::fs::read_to_string(ui.vault.join("people/ines-roux.md")).unwrap();
    assert!(me.contains("- plombier : [[jo]]"), "{me}");
    let (_, history) = api(&ui, &cookie, "/api/history");
    assert_eq!(history["changes"][0]["by"]["kind"], "person");
    assert_eq!(post(&ui, &cookie, &answer, r#"{"name": "Jo L"}"#).0, 409);

    let lou = after["questions"][0]["id"].as_str().unwrap().to_owned();
    let (status, none) = post(&ui, &cookie, &format!("/api/questions/{lou}/skip"), "{}");
    assert_eq!(status, 200);
    assert_eq!(none["questions"], serde_json::json!([]));
    assert_eq!(api(&ui, &cookie, "/api/app").1["waiting"], 0);
    assert_eq!(
        post(&ui, &cookie, &format!("/api/questions/{lou}/wave"), "{}").0,
        404
    );
}

#[test]
fn the_settings_through_the_api() {
    let ui = start();
    let cookie = login(&ui);
    let (status, view) = api(&ui, &cookie, "/api/settings");
    assert_eq!(status, 200);
    assert_eq!(
        (&view["language"], &view["time_zone"]),
        (&"fr".into(), &"Europe/Paris".into())
    );
    assert_eq!(view["version"], env!("CARGO_PKG_VERSION"));
    assert!(view["folder"].as_str().unwrap().ends_with("memoire"));

    let (status, view) = post(
        &ui,
        &cookie,
        "/api/settings",
        r#"{"language": "en", "time_zone": "America/Toronto"}"#,
    );
    assert_eq!(status, 200, "{view}");
    assert_eq!(view["time_zone"], "America/Toronto");
    assert_eq!(
        api(&ui, &cookie, "/api/app").1["time_zone"],
        "America/Toronto"
    );
    let (_, history) = api(&ui, &cookie, "/api/history");
    assert_eq!(history["changes"][0]["by"]["kind"], "person");
    assert_eq!(history["changes"][0]["summary"], "Change the settings");

    for bad in [
        r#"{"language": "de", "time_zone": "Europe/Paris"}"#,
        r#"{"language": "fr", "time_zone": "Mars/Olympus"}"#,
        r#"{"language": "fr"}"#,
    ] {
        assert_eq!(post(&ui, &cookie, "/api/settings", bad).0, 409, "{bad}");
    }
    assert_eq!(api(&ui, &cookie, "/api/settings").1["language"], "en");
}

#[test]
fn the_whole_memory_through_the_api() {
    let ui = start_with(|vault| {
        let write = |rel: &str, i: u8, front: &str, body: &str| {
            let path = vault.join(rel);
            std::fs::create_dir_all(path.parent().unwrap()).unwrap();
            std::fs::write(
                path,
                format!("---\nid: 01jcx8m2p4q6r8t0v2w4y6z8{i:02}\n{front}\ncreated: 2026-01-10T09:00:00+01:00\nupdated: 2026-01-10T09:00:00+01:00\n---\n\n{body}"),
            )
            .unwrap();
        };
        write(
            "organizations/atelier-nord.md",
            1,
            "type: organization\ncategory: work\nname: Atelier Nord",
            "## Faits\n\n- Studio de design.\n",
        );
        write(
            "people/marie-durand.md",
            2,
            "type: person\ncategory: people\nname: Marie Durand",
            "## Relations\n\n- travaille chez : [[atelier-nord]]\n",
        );
        write(
            "states/music/2026.md",
            3,
            "type: state-log\ncategory: media\ntopic: music\nyear: 2026",
            &format!(
                "| Subject | First seen | Last seen | Count | Source |\n| --- | --- | --- | --- | --- |\n| Mira Sol | 2026-01-01 | {} | 12 | device |\n",
                jiff::Zoned::now().date()
            ),
        );
    });
    let cookie = login(&ui);

    let (status, memory) = api(&ui, &cookie, "/api/memory");
    assert_eq!(status, 200, "{memory}");
    let count = |kind: &str| {
        memory["kinds"]
            .as_array()
            .unwrap()
            .iter()
            .find(|k| k["kind"] == kind)
            .unwrap()["count"]
            .clone()
    };
    assert_eq!(count("organization"), 1);
    assert_eq!(count("person"), 2);
    assert_eq!(memory["now"][0]["subject"]["name"], "Mira Sol");
    assert_eq!(memory["now"][0]["topic"], "music");
    assert_eq!(api(&ui, &cookie, "/api/home").1["now"][0]["topic"], "music");

    let (_, list) = api(&ui, &cookie, "/api/memory/organizations");
    assert_eq!(list["kind"], "organization");
    assert_eq!(list["items"][0]["name"], "Atelier Nord");
    assert_eq!(api(&ui, &cookie, "/api/memory/nowhere").0, 404);

    let id = list["items"][0]["id"].as_str().unwrap().to_owned();
    let (_, file) = api(&ui, &cookie, &format!("/api/file/{id}"));
    let back = &file["value"]["backlinks"][0];
    assert_eq!(back["from"]["name"], "Marie Durand");
    assert_eq!(back["from"]["kind"], "person");
    assert_eq!(back["word"], "travaille chez");

    let (_, found) = api(&ui, &cookie, "/api/search?q=atelier");
    let names: Vec<&str> = found["items"]
        .as_array()
        .unwrap()
        .iter()
        .map(|f| f["name"].as_str().unwrap())
        .collect();
    assert_eq!(names, ["Atelier Nord", "Marie Durand"]);
    let marie = &found["items"][1];
    assert_eq!(
        marie["excerpt"], "travaille chez : Atelier Nord",
        "no [[link]] syntax"
    );
    assert_eq!(
        api(&ui, &cookie, "/api/search").1["items"],
        serde_json::json!([])
    );
}

#[test]
#[allow(clippy::too_many_lines)] // one story: edit, create, delete
fn editing_creating_and_deleting_through_the_api() {
    let ui = start_with(|vault| {
        std::fs::write(
            vault.join("people/marie-durand.md"),
            "---\nid: 01jcx8m2p4q6r8t0v2w4y6z801\ntype: person\ncategory: people\nname: Marie Durand\ncreated: 2026-01-10T09:00:00+01:00\nupdated: 2026-01-10T09:00:00+01:00\n---\n\n## Faits\n\n- Aime le jazz ^f-ab12\n- Née à Lyon ^f-cd34\n\n## Relations\n\n- frère : [[paul-roux]]\n",
        )
        .unwrap();
    });
    let cookie = login(&ui);
    let id = "01jcx8m2p4q6r8t0v2w4y6z801";

    let (status, form) = api(&ui, &cookie, &format!("/api/file/{id}/edit"));
    assert_eq!(status, 200, "{form}");
    assert_eq!(form["kind"], "person");
    assert_eq!(
        form["facts"],
        serde_json::json!(["Aime le jazz", "Née à Lyon"])
    );
    assert!(
        !form.to_string().contains("f-ab12"),
        "fact ids stay out of the interface"
    );
    let relation = &form["relations"][0];
    assert_eq!(relation["target"], "paul-roux");
    assert_eq!(relation["link"]["id"], serde_json::Value::Null, "dangling");
    let version = form["version"].as_str().unwrap().to_owned();

    let edit = serde_json::json!({
        "version": version,
        "keys": {"name": "Marie Durand", "phones": ["+33612345678"], "birthday": null},
        "facts": [{"was": 1, "text": "Née à Lyon en 1990"}],
        "relations": [{"word": "frère", "target": "paul-roux"}],
        "notes": "Appelle le dimanche."
    });
    let (status, file) = post(
        &ui,
        &cookie,
        &format!("/api/file/{id}/edit"),
        &edit.to_string(),
    );
    assert_eq!(status, 200, "{file}");
    assert_eq!(
        file["value"]["facts"],
        serde_json::json!(["Née à Lyon en 1990"])
    );
    assert_eq!(file["value"]["phones"][0], "+33612345678");
    assert_eq!(file["value"]["notes"], "Appelle le dimanche.");

    // The same form, sent again, was read before that change.
    let (status, stale) = post(
        &ui,
        &cookie,
        &format!("/api/file/{id}/edit"),
        &edit.to_string(),
    );
    assert_eq!(status, 409);
    assert!(stale["error"].as_str().unwrap().contains("changed since"));

    let (_, history) = api(&ui, &cookie, "/api/history");
    assert_eq!(history["changes"][0]["summary"], "Edit a file", "{history}");

    let (_, view) = api(&ui, &cookie, &format!("/api/file/{id}"));
    assert_eq!(
        view["value"]["relations"][0]["missing"], "paul-roux",
        "{view}"
    );

    // An event's times come as the form's fields give them, in the vault's time zone.
    let event = serde_json::json!({
        "kind": "event",
        "keys": {"title": "Concert", "start": "2026-12-04T20:00", "end": "2026-12-04T22:30"}
    });
    let (status, created) = post(&ui, &cookie, "/api/files", &event.to_string());
    assert_eq!(status, 200, "{created}");
    let concert =
        std::fs::read_to_string(ui.vault.join("events/2026/2026-12-04-concert.md")).unwrap();
    assert!(concert.contains("2026-12-04T20:00:00+01:00"), "{concert}");
    assert!(concert.contains("2026-12-04T22:30:00+01:00"), "{concert}");

    // Create the file the dangling link points to.
    let create = serde_json::json!({
        "kind": "person", "keys": {"name": "Paul Roux"}, "stem": "paul-roux"
    });
    let (status, created) = post(&ui, &cookie, "/api/files", &create.to_string());
    assert_eq!(status, 200, "{created}");
    assert_eq!(created["kind"], "person");
    let paul = created["id"].as_str().unwrap().to_owned();
    assert!(ui.vault.join("people/paul-roux.md").is_file());
    let (_, form) = api(&ui, &cookie, &format!("/api/file/{id}/edit"));
    assert_eq!(form["relations"][0]["link"]["id"], paul.as_str());
    let (_, view) = api(&ui, &cookie, &format!("/api/file/{id}"));
    assert_eq!(
        view["value"]["relations"][0]["missing"],
        serde_json::Value::Null
    );

    let (status, refused) = post(
        &ui,
        &cookie,
        "/api/files",
        r#"{"kind": "event", "keys": {"title": "Concert"}}"#,
    );
    assert_eq!(status, 409);
    assert!(refused["error"].as_str().unwrap().contains("start"));

    // Delete Paul: Marie's file links to him.
    let (_, form) = api(&ui, &cookie, &format!("/api/file/{paul}/edit"));
    assert_eq!(form["links"][0]["name"], "Marie Durand");
    let body = serde_json::json!({"version": form["version"]});
    let (status, deleted) = post(
        &ui,
        &cookie,
        &format!("/api/file/{paul}/delete"),
        &body.to_string(),
    );
    assert_eq!(status, 200, "{deleted}");
    assert!(!ui.vault.join("people/paul-roux.md").exists());
    assert_eq!(api(&ui, &cookie, &format!("/api/file/{paul}")).0, 404);

    // The owner's own file cannot be deleted.
    let (_, home) = api(&ui, &cookie, "/api/home");
    let me = home["me"]["id"].as_str().unwrap().to_owned();
    let (_, form) = api(&ui, &cookie, &format!("/api/file/{me}/edit"));
    assert_eq!(form["owner"], true);
    let body = serde_json::json!({"version": form["version"]});
    assert_eq!(
        post(
            &ui,
            &cookie,
            &format!("/api/file/{me}/delete"),
            &body.to_string()
        )
        .0,
        409
    );
}

#[test]
fn the_graph_through_the_api() {
    let ui = start_with(|vault| {
        let write = |rel: &str, i: u8, front: &str, body: &str| {
            let path = vault.join(rel);
            std::fs::create_dir_all(path.parent().unwrap()).unwrap();
            std::fs::write(
                path,
                format!("---\nid: 01jcx8m2p4q6r8t0v2w4y6z8{i:02}\n{front}\ncreated: 2026-01-10T09:00:00+01:00\nupdated: 2026-01-10T09:00:00+01:00\n---\n\n{body}"),
            )
            .unwrap();
        };
        write(
            "organizations/atelier-nord.md",
            1,
            "type: organization\ncategory: work\nname: Atelier Nord",
            "",
        );
        write(
            "people/marie-durand.md",
            2,
            "type: person\ncategory: people\nname: Marie Durand",
            "## Relations\n\n- travaille chez : [[atelier-nord]]\n",
        );
        write(
            "people/dr-morel.md",
            3,
            "type: person\ncategory: health\nname: Dr Morel",
            "## Relations\n\n- patiente : [[marie-durand]]\n",
        );
    });
    let cookie = login(&ui);

    // The owner's file by default: nobody links to it yet.
    let (status, mine) = api(&ui, &cookie, "/api/graph");
    assert_eq!(status, 200, "{mine}");
    assert_eq!(mine["nodes"][0]["name"], "Inès Roux");
    assert!(
        mine["categories"]
            .as_array()
            .unwrap()
            .iter()
            .any(|c| c["name"] == "health" && c["sensitive"] == true)
    );

    let marie = "01jcx8m2p4q6r8t0v2w4y6z802";
    let (_, graph) = api(&ui, &cookie, &format!("/api/graph?id={marie}&depth=2"));
    let names: Vec<&str> = graph["nodes"]
        .as_array()
        .unwrap()
        .iter()
        .map(|n| n["name"].as_str().unwrap())
        .collect();
    assert_eq!(names, ["Marie Durand", "Atelier Nord", "Dr Morel"]);
    assert_eq!(graph["nodes"][2]["sensitive"], true);
    assert_eq!(graph["edges"][0]["word"], "travaille chez");

    let (_, people) = api(
        &ui,
        &cookie,
        &format!("/api/graph?id={marie}&kinds=person&categories=people,health"),
    );
    assert_eq!(people["nodes"].as_array().unwrap().len(), 2);
    let (_, work) = api(
        &ui,
        &cookie,
        &format!("/api/graph?id={marie}&categories=work"),
    );
    assert_eq!(work["nodes"][1]["name"], "Atelier Nord");
    assert_eq!(work["nodes"].as_array().unwrap().len(), 2);

    assert_eq!(
        api(
            &ui,
            &cookie,
            &format!("/api/graph?id={marie}&kinds=summary")
        )
        .0,
        400
    );
    assert_eq!(
        api(&ui, &cookie, "/api/graph?id=01jcx8m2p4q6r8t0v2w4y6z899").0,
        404
    );
}

#[test]
fn what_is_accepted_in_advance_through_the_api() {
    let now = jiff::Zoned::now()
        .with_time_zone(jiff::tz::TimeZone::get("Europe/Paris").unwrap())
        .strftime("%Y-%m-%dT%H:%M:%S%:z")
        .to_string();
    let ui = start_with(|vault| {
        let json = format!(
            r#"{{"format": 1, "id": "01jd3k8m2p4q6r8t0v2w4y6z8b", "by": "client:claude", "created": "{now}",
            "reason": "Résumé de samedi.", "categories": ["summaries"], "task": "01jd3k8m2p4q6r8t0v2w4y6z8c",
            "changes": [{{"kind": "write_summary", "file": "summaries/day/2026/2026-11-14.md", "period": "day", "start": "2026-11-14", "text": "Dîner chez Julie."}}],
            "status": {{"state": "accepted", "at": "{now}"}}, "automatic": true}}"#
        );
        std::fs::create_dir_all(vault.join(".ipse/proposals")).unwrap();
        std::fs::write(
            vault.join(".ipse/proposals/01jd3k8m2p4q6r8t0v2w4y6z8b.json"),
            json,
        )
        .unwrap();
    });
    let cookie = login(&ui);

    let (_, home) = api(&ui, &cookie, "/api/home");
    assert_eq!(
        home["automatic"],
        serde_json::json!([{"standing": "summaries", "count": 1}])
    );
    assert_eq!(home["waiting"], 0, "the inbox does not wait on it");
    let (_, inbox) = api(&ui, &cookie, "/api/proposals");
    assert_eq!(inbox["waiting"], serde_json::json!([]));
    assert_eq!(inbox["decided"][0]["automatic"], true);
    assert_eq!(inbox["offers"], serde_json::json!([]));
    // Undone in one click.
    assert_eq!(
        post(
            &ui,
            &cookie,
            "/api/proposals/01jd3k8m2p4q6r8t0v2w4y6z8b/undo",
            ""
        )
        .0,
        200
    );
    assert_eq!(
        api(&ui, &cookie, "/api/home").1["automatic"],
        serde_json::json!([])
    );

    // Turned on and off by the person, per AI; only the two kinds.
    let (status, connected) = post(&ui, &cookie, "/api/ais", r#"{"name": "Claude"}"#);
    assert_eq!(status, 200);
    let id = connected["ai"]["id"].as_str().unwrap().to_owned();
    assert_eq!(
        connected["ai"]["auto_accept"],
        serde_json::json!([]),
        "off by default"
    );
    let (status, view) = post(
        &ui,
        &cookie,
        &format!("/api/ais/{id}/auto"),
        r#"{"standing": "summaries", "on": true}"#,
    );
    assert_eq!(status, 200);
    assert_eq!(
        view["ais"][0]["auto_accept"],
        serde_json::json!(["summaries"])
    );
    let (status, _) = post(
        &ui,
        &cookie,
        &format!("/api/ais/{id}/auto"),
        r#"{"standing": "facts", "on": true}"#,
    );
    assert_eq!(status, 409, "facts are never accepted in advance");
    let (_, view) = post(
        &ui,
        &cookie,
        &format!("/api/ais/{id}/decline"),
        r#"{"standing": "event_kinds"}"#,
    );
    assert_eq!(view["ais"][0]["offers"], serde_json::json!([]));
    let clients = std::fs::read_to_string(ui.vault.join(".ipse/clients.toml")).unwrap();
    assert!(
        clients.contains("auto_accept = [\"summaries\"]"),
        "{clients}"
    );
    assert!(
        clients.contains("declined = [\"event_kinds\"]"),
        "{clients}"
    );
}
