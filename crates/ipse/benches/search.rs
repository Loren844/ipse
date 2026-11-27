//! Search while typing (issue #258): on a memory of 22 000 files (20 000 people, 2 000
//! events, a weekly class among them), each letter typed in the side bar asks for a
//! few results of each kind, as the interface asks for them; then Enter asks for all
//! of them. Timed over HTTP, from the request to the whole answer. Data invented.
#![allow(missing_docs, clippy::unwrap_used, clippy::print_stdout)]

use std::fmt::Write as _;
use std::io::{BufRead as _, BufReader, Read as _, Write as _};
use std::net::TcpStream;
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

const PEOPLE: usize = 20_000;
const EVENTS: usize = 2_000;
const RUNS: usize = 21;
const FIRST: [&str; 8] = ["Marie", "Paul", "Zoé", "Hugo", "Léa", "Nina", "Tom", "Inès"];
const LAST: [&str; 8] = [
    "Durand", "Martin", "Petit", "Roux", "Faure", "Blanc", "Garnier", "Morel",
];
const STAMP: &str = "created: 2026-01-10T09:00:00+01:00\nupdated: 2026-01-10T09:00:00+01:00";

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

fn fill(vault: &std::path::Path) {
    for i in 0..PEOPLE {
        let name = format!("{} {} {i}", FIRST[i % 8], LAST[(i / 8) % 8]);
        std::fs::write(
            vault.join(format!("people/p-{i}.md")),
            format!(
                "---\nid: 01jcx8m2p4q6r8t0v2w4{i:06}\ntype: person\ncategory: people\n{STAMP}\nname: {name}\n---\n\n## Faits\n\n- Joue au tennis le samedi.\n\n## Relations\n\n- ami : [[p-{}]]\n",
                (i + 1) % PEOPLE
            ),
        )
        .unwrap();
    }
    let events = vault.join("events/2026");
    std::fs::create_dir_all(&events).unwrap();
    let first = jiff::civil::date(2025, 1, 7);
    for e in 0..EVENTS {
        // One in four a yoga class every Tuesday at 19:00; the others once.
        let (title, day, time) = if e % 4 == 0 {
            let week = i64::try_from(e / 4).unwrap();
            (
                "Yoga".to_owned(),
                first + jiff::Span::new().weeks(week),
                "19:00",
            )
        } else {
            let d = i64::try_from(e).unwrap();
            (
                format!("Café {e}"),
                first + jiff::Span::new().days(d),
                "12:30",
            )
        };
        let mut people = String::new();
        for p in [0, e * 10 + 3] {
            let _ = writeln!(people, "  - \"[[p-{p}]]\"");
        }
        std::fs::write(
            events.join(format!("e-{e}.md")),
            format!(
                "---\nid: 01jcx8m2p4q6r8t0v2w5{e:06}\ntype: event\ncategory: agenda\n{STAMP}\ntitle: {title}\nstart: {day}T{time}:00+01:00\nplace: Studio Lotus\nparticipants:\n{people}---\n"
            ),
        )
        .unwrap();
    }
}

fn main() {
    let dir = tempfile::tempdir().unwrap();
    let vault = dir.path().join("memoire");
    let ipse = env!("CARGO_BIN_EXE_ipse");
    let made = Command::new(ipse)
        .args([
            "init",
            vault.to_str().unwrap(),
            "--name",
            "Jules Moreau",
            "--language",
            "fr",
            "--time-zone",
            "Europe/Paris",
        ])
        .output()
        .unwrap();
    assert!(made.status.success());
    fill(&vault);

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
    let rest = line.trim().strip_prefix("http://127.0.0.1:").unwrap();
    let port: u16 = rest[..rest.find('/').unwrap()].parse().unwrap();
    let login = get(port, &rest[rest.find('/').unwrap()..], None);
    let cookie = login
        .lines()
        .find_map(|l| l.strip_prefix("set-cookie: "))
        .and_then(|c| c.split(';').next())
        .unwrap()
        .to_owned();
    // The first request reads the memory.
    assert!(get(port, "/api/home", Some(&cookie)).starts_with("HTTP/1.1 200"));

    let time = |path: &str| {
        let mut times: Vec<Duration> = (0..RUNS)
            .map(|_| {
                let start = Instant::now();
                let answer = get(port, path, Some(&cookie));
                let took = start.elapsed();
                assert!(answer.starts_with("HTTP/1.1 200"), "{path}: {answer}");
                assert!(answer.contains("\"groups\""), "{path}");
                took
            })
            .collect();
        times.sort();
        (times[RUNS / 2], times[RUNS - 1])
    };
    let mut slowest = Duration::ZERO;
    for typed in [
        "m", "ma", "mar", "mari", "marie", "y", "yo", "yog", "yoga", "t", "te", "ten", "tennis",
    ] {
        let (median, worst) = time(&format!("/api/search?q={typed}&few=1"));
        slowest = slowest.max(median);
        println!("while typing \"{typed}\": median {median:.1?}, slowest {worst:.1?}");
    }
    for query in ["marie", "yoga", "tennis"] {
        let (median, worst) = time(&format!("/api/search?q={query}"));
        println!("all results for \"{query}\": median {median:.1?}, slowest {worst:.1?}");
    }
    println!(
        "search of {} files while typing: slowest median {slowest:.1?} (goal: under 100 ms)",
        PEOPLE + EVENTS + 1
    );
    drop(child.stdin.take());
    child.wait().unwrap();
}
