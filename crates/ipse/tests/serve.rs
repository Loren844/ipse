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

/// The value of `name` in a head, lowercased name.
fn header<'a>(head: &'a str, name: &str) -> Option<&'a str> {
    head.lines()
        .find_map(|l| l.strip_prefix(&format!("{name}: ")))
        .map(str::trim)
}

/// Percent-encodes a form value.
fn form(text: &str) -> String {
    text.bytes()
        .map(|b| {
            if b.is_ascii_alphanumeric() || b"-._~".contains(&b) {
                char::from(b).to_string()
            } else {
                format!("%{b:02X}")
            }
        })
        .collect()
}

/// The PKCE `S256` challenge of `verifier`.
fn challenge(verifier: &str) -> String {
    use sha2::Digest as _;
    const ALPHABET: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789-_";
    let hash = sha2::Sha256::digest(verifier.as_bytes());
    let mut out = String::new();
    for chunk in hash.chunks(3) {
        let b = [
            chunk[0],
            *chunk.get(1).unwrap_or(&0),
            *chunk.get(2).unwrap_or(&0),
        ];
        let n = (u32::from(b[0]) << 16) | (u32::from(b[1]) << 8) | u32::from(b[2]);
        for (i, shift) in [18, 12, 6, 0].into_iter().enumerate() {
            if i <= chunk.len() {
                out.push(char::from(ALPHABET[(n >> shift) as usize & 63]));
            }
        }
    }
    out
}

/// claude.ai's way in (issue #270): discovery, registration, the person's approval
/// behind the interface's code, the token, an MCP call with it, and a refused token
/// once the AI is disconnected.
#[test]
#[allow(clippy::too_many_lines)] // one story, as claude.ai lives it
fn an_ai_app_is_approved_over_oauth_then_disconnected() {
    let server = start();
    let code = access_code(&server.vault);
    let (_, cookie) = login(&server, &code);
    let cookie = cookie.unwrap();
    let cookie = cookie.split(';').next().unwrap();
    let https = ("X-Forwarded-Proto", "https");
    let base = "https://ipse.maison.lan";

    // 1. MCP without a code says where to look.
    let initialize = r#"{"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2025-06-18","capabilities":{},"clientInfo":{"name":"claude-ai","version":"1"}}}"#;
    let json = ("Content-Type", "application/json");
    let accept = ("Accept", "application/json, text/event-stream");
    let (status, head, _) = send(&server, "POST", "/mcp", &[json, accept, https], initialize);
    assert_eq!(status, 401);
    let metadata = format!("{base}/.well-known/oauth-protected-resource/mcp");
    assert_eq!(
        header(&head, "www-authenticate"),
        Some(format!("Bearer resource_metadata=\"{metadata}\"").as_str())
    );
    let (status, _, body) = send(
        &server,
        "GET",
        "/.well-known/oauth-protected-resource/mcp",
        &[https],
        "",
    );
    assert_eq!(status, 200, "{body}");
    let resource: serde_json::Value = serde_json::from_str(&body).unwrap();
    assert_eq!(resource["resource"], format!("{base}/mcp"));
    assert_eq!(resource["authorization_servers"][0], base);
    let (_, _, body) = send(
        &server,
        "GET",
        "/.well-known/oauth-authorization-server",
        &[https],
        "",
    );
    let meta: serde_json::Value = serde_json::from_str(&body).unwrap();
    assert_eq!(meta["issuer"], base);
    assert_eq!(meta["code_challenge_methods_supported"][0], "S256");
    assert_eq!(
        meta["registration_endpoint"],
        format!("{base}/oauth/register")
    );

    // 2. The app registers itself.
    let callback = "https://claude.ai/api/mcp/auth_callback";
    let (status, _, body) = send(
        &server,
        "POST",
        "/oauth/register",
        &[json, https],
        &format!(
            r#"{{"client_name":"Claude","redirect_uris":["{callback}"],"token_endpoint_auth_method":"none"}}"#
        ),
    );
    assert_eq!(status, 201, "{body}");
    let registered: serde_json::Value = serde_json::from_str(&body).unwrap();
    let client_id = registered["client_id"].as_str().unwrap().to_owned();
    let (status, _, _) = send(
        &server,
        "POST",
        "/oauth/register",
        &[json],
        r#"{"client_name":"x","redirect_uris":["http://evil.example/cb"]}"#,
    );
    assert_eq!(status, 400, "plain http elsewhere is refused");

    // 3. The person is sent to approve it, from claude.ai: their session's cookie is
    // not sent from another site, so the page reloads itself from Ipse, then asks to
    // log in when there is still none.
    let verifier = "a-verifier-of-forty-three-characters-or-more-0123456789";
    let query = |state: &str, verifier: &str| {
        format!(
            "/oauth/authorize?response_type=code&client_id={client_id}&redirect_uri={}&code_challenge={}&code_challenge_method=S256&state={state}&resource={}",
            form(callback),
            challenge(verifier),
            form(&format!("{base}/mcp"))
        )
    };
    let cross = ("Sec-Fetch-Site", "cross-site");
    let (status, _, body) = send(&server, "GET", &query("s1", verifier), &[cross], "");
    assert_eq!(status, 200);
    assert!(body.contains("again=1"), "{body}");
    let (status, head, _) = send(
        &server,
        "GET",
        &format!("{}&again=1", query("s1", verifier)),
        &[("Sec-Fetch-Site", "same-origin")],
        "",
    );
    assert_eq!(status, 303);
    assert!(
        header(&head, "location")
            .unwrap()
            .starts_with("/login?next=%2Foauth%2Fauthorize%3F")
    );

    // Logging in from there comes back to the approval page; nowhere else.
    let next = form(&query("s1", verifier));
    let (status, head, _) = send(
        &server,
        "POST",
        "/login",
        &[FORM, ORIGIN, https],
        &format!("code={code}&next={next}"),
    );
    assert_eq!(status, 303);
    assert_eq!(
        header(&head, "location"),
        Some(query("s1", verifier).as_str())
    );
    let (_, head, _) = send(
        &server,
        "POST",
        "/login",
        &[FORM, ORIGIN, https],
        &format!("code={code}&next=https%3A%2F%2Fevil.example"),
    );
    assert_eq!(header(&head, "location"), Some("/"));

    // With the session: the approval page, sensitive categories closed.
    let with = ("Cookie", cookie);
    let (status, head, page) = send(&server, "GET", &query("s1", verifier), &[with, cross], "");
    assert_eq!(status, 200, "{page}");
    assert!(
        page.contains("Connecter Claude à votre mémoire\u{a0}?"),
        "{page}"
    );
    assert!(page.contains("<b>claude.ai</b>"), "{page}");
    assert!(page.contains("value=\"people\" checked"), "{page}");
    assert!(page.contains("value=\"health\">"), "{page}");
    assert!(
        header(&head, "content-security-policy")
            .unwrap()
            .contains("form-action 'self' https://claude.ai"),
        "{head}"
    );

    // Approving: a form of that page, as any change.
    let approve = |state: &str, verifier: &str| {
        let body = format!(
            "response_type=code&client_id={client_id}&redirect_uri={}&state={state}&code_challenge={}&code_challenge_method=S256&name=Claude+%28web%29&read=identity&read=people&decision=approve",
            form(callback),
            challenge(verifier)
        );
        let (status, head, _) = send(
            &server,
            "POST",
            "/oauth/authorize",
            &[FORM, ORIGIN, with, https],
            &body,
        );
        assert_eq!(status, 303, "{head}");
        let to = header(&head, "location").unwrap().to_owned();
        assert!(to.starts_with(&format!("{callback}?code=")), "{to}");
        assert!(
            to.ends_with(&format!("&iss={}&state={state}", form(base))),
            "{to}"
        );
        to.split("code=")
            .nth(1)
            .unwrap()
            .split('&')
            .next()
            .unwrap()
            .to_owned()
    };
    let (status, _, _) = send(
        &server,
        "POST",
        "/oauth/authorize",
        &[FORM, with, https],
        "decision=approve",
    );
    assert_eq!(status, 403, "a change says where it comes from");

    // 4. A wrong verifier spends the code; the right one, on a new approval, gets the
    // token, once.
    let token_body = |code: &str, verifier: &str| {
        format!(
            "grant_type=authorization_code&code={code}&redirect_uri={}&client_id={client_id}&code_verifier={verifier}",
            form(callback)
        )
    };
    let first = approve("s1", verifier);
    let (status, _, body) = send(
        &server,
        "POST",
        "/oauth/token",
        &[FORM],
        &token_body(&first, "not-the-verifier-not-the-verifier-not-the-v"),
    );
    assert_eq!(status, 400, "{body}");
    assert!(body.contains("invalid_grant"));
    let (status, _, _) = send(
        &server,
        "POST",
        "/oauth/token",
        &[FORM],
        &token_body(&first, verifier),
    );
    assert_eq!(status, 400, "a code is used once");
    assert!(
        !std::fs::read_to_string(server.vault.join(".ipse/clients.toml"))
            .unwrap_or_default()
            .contains("Claude (web)")
    );

    let second = approve("s2", verifier);
    let (status, head, body) = send(
        &server,
        "POST",
        "/oauth/token",
        &[FORM],
        &token_body(&second, verifier),
    );
    assert_eq!(status, 200, "{body}");
    assert_eq!(header(&head, "cache-control"), Some("no-store"));
    let answer: serde_json::Value = serde_json::from_str(&body).unwrap();
    assert_eq!(answer["token_type"], "Bearer");
    let token = answer["access_token"].as_str().unwrap().to_owned();
    let clients = std::fs::read_to_string(server.vault.join(".ipse/clients.toml")).unwrap();
    assert!(clients.contains("name = \"Claude (web)\""), "{clients}");
    assert!(
        clients.contains("read = [\"identity\", \"people\"]"),
        "{clients}"
    );
    assert!(!clients.contains(&token), "the token is never stored");

    // 5. MCP with the token, as any connected AI.
    let auth = format!("Bearer {token}");
    let (status, _, body) = send(
        &server,
        "POST",
        "/mcp",
        &[json, accept, ("Authorization", &auth)],
        initialize,
    );
    assert_eq!(status, 200, "{body}");
    assert!(body.contains("serverInfo"), "{body}");

    // 6. Disconnected: its token no longer works.
    let id = clients
        .lines()
        .take_while(|l| !l.contains("Claude (web)"))
        .filter_map(|l| l.strip_prefix("id = \""))
        .last()
        .map_or_else(
            || "claude-web".to_owned(),
            |l| l.trim_end_matches('"').to_owned(),
        );
    let out = Command::new(IPSE)
        .args(["client", "remove", &id, "--vault"])
        .arg(&server.vault)
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let (status, _, _) = send(
        &server,
        "POST",
        "/mcp",
        &[json, accept, ("Authorization", &auth)],
        initialize,
    );
    assert_eq!(status, 401, "disconnecting revokes the token");

    // Refusing sends the app back with access_denied.
    let body = format!(
        "response_type=code&client_id={client_id}&redirect_uri={}&state=s3&code_challenge={}&code_challenge_method=S256&decision=refuse",
        form(callback),
        challenge(verifier)
    );
    let (_, head, _) = send(
        &server,
        "POST",
        "/oauth/authorize",
        &[FORM, ORIGIN, with, https],
        &body,
    );
    assert_eq!(
        header(&head, "location"),
        Some(format!("{callback}?error=access_denied&state=s3").as_str())
    );
}
