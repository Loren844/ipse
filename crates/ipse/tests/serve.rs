//! `ipse serve`: the memory on a home server (decision 0019). The interface asks for
//! its own access code, MCP for each AI's, both on one port.

// Test helpers may panic: a failed expectation is a failed test.
#![allow(clippy::expect_used, clippy::unwrap_used)]

use std::fmt::Write as _;
use std::io::{BufRead as _, BufReader, Read as _, Write as _};
use std::net::TcpStream;
use std::path::Path;
use std::process::{Child, Command, Stdio};

use ipse_core::rights::{Categories, Clients};
use ipse_core::time::TimeZone;
use ipse_core::vault::DateTime;

const IPSE: &str = env!("CARGO_BIN_EXE_ipse");

/// A running `ipse serve`, stopped when dropped.
struct Server {
    child: Child,
    /// Kept open: the server may still write to it.
    _out: BufReader<std::process::ChildStdout>,
    _dir: tempfile::TempDir,
    vault: std::path::PathBuf,
    port: u16,
    /// What it said when it started.
    said: String,
}

impl Drop for Server {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

fn start() -> Server {
    let dir = tempfile::tempdir().unwrap();
    let vault = dir.path().join("memoire");
    let out = Command::new(IPSE)
        .args(["init", vault.to_str().unwrap(), "--name", "Inès Roux"])
        .args(["--language", "fr", "--time-zone", "Europe/Paris"])
        .output()
        .unwrap();
    assert!(out.status.success());
    let mut child = Command::new(IPSE)
        .args(["serve", "--listen", "127.0.0.1:0", "--vault"])
        .arg(&vault)
        .env("IPSE_SECRETS_FILE", dir.path().join("secrets.json"))
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .unwrap();
    let mut out = BufReader::new(child.stdout.take().unwrap());
    let mut said = String::new();
    // The first lines, up to the one that says how to make a code.
    for _ in 0..2 {
        out.read_line(&mut said).unwrap();
    }
    let at = said
        .find("http://127.0.0.1:")
        .unwrap_or_else(|| panic!("{said}"));
    let port = said[at + "http://127.0.0.1:".len()..]
        .split(|c: char| !c.is_ascii_digit())
        .next()
        .unwrap()
        .parse()
        .unwrap();
    Server {
        child,
        _out: out,
        _dir: dir,
        vault,
        port,
        said,
    }
}

/// Sends one request; the status, the head and the body of the answer.
fn send(
    server: &Server,
    method: &str,
    path: &str,
    headers: &[(&str, &str)],
    body: &str,
) -> (u16, String, String) {
    let mut stream = TcpStream::connect(("127.0.0.1", server.port)).unwrap();
    let mut text = format!("{method} {path} HTTP/1.1\r\nConnection: close\r\n");
    if !headers.iter().any(|(n, _)| n.eq_ignore_ascii_case("host")) {
        text.push_str("Host: ipse.maison.lan\r\n");
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

/// Makes a new access code for the interface.
fn access_code(vault: &Path) -> String {
    let out = Command::new(IPSE)
        .args(["access-code", "--vault"])
        .arg(vault)
        .output()
        .unwrap();
    assert!(out.status.success());
    String::from_utf8(out.stdout)
        .unwrap()
        .split_whitespace()
        .find(|w| w.starts_with("ipse_ui_"))
        .unwrap()
        .to_owned()
}

const FORM: (&str, &str) = ("Content-Type", "application/x-www-form-urlencoded");
const ORIGIN: (&str, &str) = ("Origin", "https://ipse.maison.lan");

/// Logs in with `code`; the status and the cookie, if one was set.
fn login(server: &Server, code: &str) -> (u16, Option<String>) {
    let (status, head, _) = send(
        server,
        "POST",
        "/login",
        &[FORM, ORIGIN, ("X-Forwarded-Proto", "https")],
        &format!("code={code}"),
    );
    let cookie = head
        .lines()
        .find_map(|l| l.strip_prefix("set-cookie: "))
        .map(str::to_owned);
    (status, cookie)
}

#[test]
fn the_interface_asks_for_its_own_code() {
    let server = start();
    assert!(
        server.said.contains("no access code yet"),
        "{}",
        server.said
    );

    // No code made yet: the login page says how, and nothing opens.
    let (status, head, _) = send(&server, "GET", "/", &[], "");
    assert_eq!(status, 303);
    assert!(head.contains("location: /login"), "{head}");
    let (_, _, page) = send(&server, "GET", "/login", &[], "");
    assert!(page.contains("ipse access-code"), "{page}");
    assert_eq!(login(&server, "ipse_ui_guess").0, 200);

    let code = access_code(&server.vault);
    let (status, cookie) = login(&server, "ipse_ui_wrong");
    assert_eq!((status, cookie), (401, None));
    let (status, cookie) = login(&server, &code);
    assert_eq!(status, 303);
    let cookie = cookie.unwrap();
    for part in ["HttpOnly", "SameSite=Strict", "Secure", "Max-Age=2592000"] {
        assert!(cookie.contains(part), "{cookie}");
    }
    let session = cookie.split(';').next().unwrap().to_owned();

    // Any name the server is reached by, with the session.
    let (status, _, body) = send(&server, "GET", "/api/app", &[("Cookie", &session)], "");
    assert_eq!(status, 200, "{body}");
    let (status, _, _) = send(
        &server,
        "GET",
        "/api/app",
        &[("Host", "192.168.1.10:7420"), ("Cookie", &session)],
        "",
    );
    assert_eq!(status, 200);
    // Without it, or from another site, nothing.
    assert_eq!(send(&server, "GET", "/api/app", &[], "").0, 401);
    let (status, _, _) = send(
        &server,
        "GET",
        "/api/app",
        &[("Cookie", &session), ("Origin", "https://evil.example")],
        "",
    );
    assert_eq!(status, 403);
    // A login from another site's form is refused before the code is read.
    let (status, _, _) = send(
        &server,
        "POST",
        "/login",
        &[FORM, ("Origin", "https://evil.example")],
        &format!("code={code}"),
    );
    assert_eq!(status, 403);

    // A new code closes the sessions opened with the old one.
    let new = access_code(&server.vault);
    assert_eq!(
        send(&server, "GET", "/api/app", &[("Cookie", &session)], "").0,
        401
    );
    assert_eq!(login(&server, &code).0, 401);
    assert_eq!(login(&server, &new).0, 303);
}

#[test]
fn ai_apps_reach_mcp_on_the_same_port() {
    let server = start();
    let mut clients = Clients::load(&server.vault).unwrap();
    let (_, token) = clients
        .add(
            "Claude",
            DateTime::now(&TimeZone::UTC),
            &Categories::default(),
            false,
        )
        .unwrap();
    let initialize = r#"{"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2025-06-18","capabilities":{},"clientInfo":{"name":"test","version":"1"}}}"#;
    let json = ("Content-Type", "application/json");
    let accept = ("Accept", "application/json, text/event-stream");
    let auth = format!("Bearer {}", token.expose());
    let (status, _, body) = send(
        &server,
        "POST",
        "/mcp",
        &[json, accept, ("Authorization", &auth)],
        initialize,
    );
    assert_eq!(status, 200, "{body}");
    assert!(body.contains("serverInfo"), "{body}");

    assert_eq!(
        send(&server, "POST", "/mcp", &[json, accept], initialize).0,
        401,
        "no code, no memory"
    );
    let (status, _, _) = send(
        &server,
        "POST",
        "/mcp",
        &[json, accept, ("Authorization", &auth), ORIGIN],
        initialize,
    );
    assert_eq!(status, 403, "no browser reaches MCP");
}

#[test]
fn listening_on_the_network_is_said() {
    let out = Command::new(IPSE)
        .args(["serve", "--listen", "nas.lan", "--vault", "."])
        .output()
        .unwrap();
    assert!(!out.status.success());
    assert!(String::from_utf8_lossy(&out.stderr).contains("--listen takes"));
}
