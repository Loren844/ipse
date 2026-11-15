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
        serde_json::json!({"name": "Inès Roux", "language": "fr", "waiting": 0})
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
