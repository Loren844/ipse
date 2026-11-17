//! The desktop app's start (issue #151): from starting `ipse ui --app` on the demo
//! memory to the home screen's data answered, as the app's window first asks for it.
//! What the window itself adds (painting the page) is the webview's.
#![allow(missing_docs, clippy::unwrap_used, clippy::print_stdout)]

use std::io::{BufRead as _, BufReader, Read as _, Write as _};
use std::net::TcpStream;
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

const RUNS: usize = 10;

/// One HTTP/1.1 request on `127.0.0.1:port`; returns the response's head and body.
fn get(port: u16, path: &str, cookie: Option<&str>) -> String {
    let mut stream = TcpStream::connect(("127.0.0.1", port)).unwrap();
    let cookie = cookie
        .map(|c| format!("Cookie: {c}\r\n"))
        .unwrap_or_default();
    write!(
        stream,
        "GET {path} HTTP/1.1\r\nHost: 127.0.0.1:{port}\r\nSec-Fetch-Site: same-origin\r\n{cookie}Connection: close\r\n\r\n"
    )
    .unwrap();
    let mut answer = String::new();
    stream.read_to_string(&mut answer).unwrap();
    answer
}

fn main() {
    let dir = tempfile::tempdir().unwrap();
    let vault = dir.path().join("demo");
    let ipse = env!("CARGO_BIN_EXE_ipse");
    let made = Command::new(ipse)
        .args([
            "init",
            vault.to_str().unwrap(),
            "--demo",
            "--language",
            "fr",
            "--time-zone",
            "Europe/Paris",
        ])
        .output()
        .unwrap();
    assert!(made.status.success());
    let mut times: Vec<Duration> = Vec::with_capacity(RUNS);
    for _ in 0..RUNS {
        let start = Instant::now();
        let mut child = Command::new(ipse)
            .args(["ui", "--app", "--vault", vault.to_str().unwrap()])
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .spawn()
            .unwrap();
        let mut line = String::new();
        BufReader::new(child.stdout.take().unwrap())
            .read_line(&mut line)
            .unwrap();
        let address = line.trim();
        let rest = address.strip_prefix("http://127.0.0.1:").unwrap();
        let port: u16 = rest[..rest.find('/').unwrap()].parse().unwrap();
        let login = get(port, &rest[rest.find('/').unwrap()..], None);
        let cookie = login
            .lines()
            .find_map(|l| l.strip_prefix("set-cookie: "))
            .and_then(|c| c.split(';').next())
            .unwrap()
            .to_owned();
        for path in ["/", "/api/app", "/api/home"] {
            let answer = get(port, path, Some(&cookie));
            assert!(answer.starts_with("HTTP/1.1 200"), "{path}: {answer}");
        }
        times.push(start.elapsed());
        drop(child.stdin.take());
        child.wait().unwrap();
    }
    times.sort();
    println!(
        "app start, demo memory ({RUNS} runs): median {:.0?}, fastest {:.0?}, slowest {:.0?}",
        times[RUNS / 2],
        times[0],
        times[RUNS - 1]
    );
}
