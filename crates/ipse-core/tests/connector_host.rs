//! The connector host against a fake connector.
//!
//! This test has no libtest harness: the same executable is both the test runner and,
//! when started with `--fake-connector <mode>`, the fake connector, so that the tests run
//! a real separate process on every operating system without extra programs.

// Test code may panic: a failed expectation is a failed test.
#![allow(
    clippy::expect_used,
    clippy::unwrap_used,
    clippy::print_stdout,
    clippy::print_stderr,
    missing_docs
)]

use std::io::{BufRead, Write};
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::time::Duration;

use ipse_core::connector::{Launch, Outcome, StopReason, run_import};
use ipse_core::journal::Journal;
use ipse_protocol::{ErrorCode, NeedKind};
use serde_json::{Value, json};

fn main() {
    let args: Vec<String> = std::env::args().collect();
    if args.get(1).map(String::as_str) == Some("--fake-connector") {
        fake_connector(args.get(2).map_or("", String::as_str));
        return;
    }
    let tests: [(&str, fn()); 8] = [
        ("normal_session", normal_session),
        (
            "cursor_is_given_back_next_time",
            cursor_is_given_back_next_time,
        ),
        (
            "invalid_json_stops_the_connector",
            invalid_json_stops_the_connector,
        ),
        (
            "error_is_returned_with_its_message",
            error_is_returned_with_its_message,
        ),
        (
            "need_is_returned_with_its_link",
            need_is_returned_with_its_link,
        ),
        ("no_ready_in_time", no_ready_in_time),
        ("a_crash_ends_the_session", a_crash_ends_the_session),
        (
            "batches_must_be_numbered_in_order",
            batches_must_be_numbered_in_order,
        ),
    ];
    let mut failed = 0;
    for (name, test) in tests {
        if catch_unwind(AssertUnwindSafe(test)).is_ok() {
            println!("test {name} ... ok");
        } else {
            println!("test {name} ... FAILED");
            failed += 1;
        }
    }
    println!("{} passed; {failed} failed", tests.len() - failed);
    if failed > 0 {
        std::process::exit(1);
    }
}

// ---------------------------------------------------------------- the fake connector

fn fake_connector(mode: &str) {
    let stdin = std::io::stdin();
    let mut lines = stdin.lock().lines();
    let mut read = || -> Value {
        lines
            .next()
            .map_or(Value::Null, |l| serde_json::from_str(&l.unwrap()).unwrap())
    };
    let mut out = std::io::stdout().lock();
    let mut say = |message: Value| {
        writeln!(out, "{message}").unwrap();
        out.flush().unwrap();
    };

    let hello = read();
    if mode == "no-ready" {
        std::thread::sleep(Duration::from_secs(5));
        return;
    }
    say(json!({"type": "ready", "protocol": 1, "connector": "fake", "version": "0"}));
    if mode == "invalid-json" {
        say(json!("this is not a protocol message"));
        std::thread::sleep(Duration::from_secs(5));
        return;
    }
    assert_eq!(read()["type"], "start");
    eprintln!("diagnostic line");

    match mode {
        "error" => say(json!({
            "type": "error", "code": "rate_limited", "retry_after": "10m",
            "message": "Trop de requêtes, nouvelle tentative dans 10 minutes."
        })),
        "need" => say(json!({
            "type": "need", "what": "file", "message": "Téléchargez votre historique.",
            "url": "https://example.org/export"
        })),
        "crash" => std::process::exit(3),
        "bad-seq" => say(json!({"type": "batch", "seq": 2, "records": []})),
        _ => {
            let env: Vec<String> = std::env::vars().map(|(k, _)| k).collect();
            say(
                json!({"type": "batch", "seq": 1, "cursor": {"page": 2}, "records": [
                    {"kind": "contact.card", "op": "upsert", "external_id": "a",
                     "observed_at": "2026-11-12T18:30:00Z", "data": {"env": env}},
                    {"kind": "contact.card", "op": "upsert",
                     "observed_at": "2026-11-12T18:30:00Z", "data": {}},
                    {"kind": "contact.card", "op": "delete", "external_id": "b",
                     "observed_at": "2026-11-12T18:30:00Z"},
                    {"kind": "calendar.event", "op": "delete", "external_id": "c",
                     "observed_at": "2026-11-12T18:30:00Z"}
                ]}),
            );
            let ack = read();
            say(json!({"type": "progress", "done": 4, "message": format!(
                "rejected={} cursor={} files={}",
                ack["rejected"], hello["cursor"], hello["files"]
            )}));
            say(json!({"type": "done", "summary": {"upserts": 1, "deletes": 1}}));
        }
    }
    // Wait for `bye`.
    while !read().is_null() {}
}

// ---------------------------------------------------------------- the tests

fn launch(mode: &str) -> Launch {
    let exe = std::env::current_exe().unwrap();
    let mut launch = Launch::new(
        vec![
            exe.to_string_lossy().into_owned(),
            "--fake-connector".into(),
            mode.into(),
        ],
        "fake",
        "fr",
        "Europe/Paris",
    );
    launch.kinds = Some(vec!["contact.card".into()]);
    launch.files = vec!["/picked/contacts.vcf".into()];
    launch.ready_timeout = Duration::from_millis(500);
    launch.bye_timeout = Duration::from_millis(500);
    launch.idle_timeout = Duration::from_secs(5);
    launch
}

fn normal_session() {
    let vault = tempfile::tempdir().unwrap();
    let mut journal = Journal::new(vault.path());
    let report = run_import(&mut journal, &launch("normal")).unwrap();

    assert!(matches!(report.outcome, Outcome::Done(s) if s.upserts == 1 && s.deletes == 1));
    assert_eq!((report.written, report.rejected), (2, 2));
    assert!(
        report.progress[0].starts_with("rejected=[1,3] cursor=null"),
        "{:?}",
        report.progress
    );
    assert!(report.progress[0].contains("contacts.vcf"));
    assert_eq!(report.diagnostics, ["diagnostic line"]);

    let records: Vec<_> = journal
        .read("fake", None)
        .unwrap()
        .map(Result::unwrap)
        .collect();
    assert_eq!(records.len(), 2);
    // Cargo gives the test runner variables such as CARGO_PKG_NAME; none may reach
    // the connector.
    let env = records[0].data.as_ref().unwrap()["env"].as_array().unwrap();
    let parent: Vec<String> = std::env::vars().map(|(k, _)| k).collect();
    assert!(
        parent.iter().any(|k| k == "CARGO_PKG_NAME"),
        "run through cargo"
    );
    // Some systems add their own variables to every process (macOS adds
    // __CF_USER_TEXT_ENCODING), so the check is on what the parent had.
    for key in env {
        let key = key.as_str().unwrap();
        assert!(
            !key.starts_with("CARGO"),
            "{key} leaked into the connector's environment"
        );
    }
    assert!(env.iter().any(|k| k == "PATH"), "PATH is kept");
    assert_eq!(journal.cursor("fake").unwrap(), Some(json!({"page": 2})));
}

fn cursor_is_given_back_next_time() {
    let vault = tempfile::tempdir().unwrap();
    let mut journal = Journal::new(vault.path());
    run_import(&mut journal, &launch("normal")).unwrap();
    let report = run_import(&mut journal, &launch("normal")).unwrap();
    assert!(
        report.progress[0].contains(r#"cursor={"page":2}"#),
        "{:?}",
        report.progress
    );
}

fn invalid_json_stops_the_connector() {
    let vault = tempfile::tempdir().unwrap();
    let report = run_import(&mut Journal::new(vault.path()), &launch("invalid-json")).unwrap();
    assert!(matches!(
        report.outcome,
        Outcome::Stopped(StopReason::BrokeProtocol(_))
    ));
    assert_eq!(report.written, 0);
}

fn error_is_returned_with_its_message() {
    let vault = tempfile::tempdir().unwrap();
    let report = run_import(&mut Journal::new(vault.path()), &launch("error")).unwrap();
    assert_eq!(
        report.outcome,
        Outcome::Failed {
            code: ErrorCode::RateLimited,
            message: "Trop de requêtes, nouvelle tentative dans 10 minutes.".into(),
            retry_after: Some("10m".into()),
        }
    );
}

fn need_is_returned_with_its_link() {
    let vault = tempfile::tempdir().unwrap();
    let report = run_import(&mut Journal::new(vault.path()), &launch("need")).unwrap();
    assert_eq!(
        report.outcome,
        Outcome::Needs {
            what: NeedKind::File,
            message: "Téléchargez votre historique.".into(),
            url: Some("https://example.org/export".into()),
        }
    );
}

fn no_ready_in_time() {
    let vault = tempfile::tempdir().unwrap();
    let started = std::time::Instant::now();
    let report = run_import(&mut Journal::new(vault.path()), &launch("no-ready")).unwrap();
    assert_eq!(report.outcome, Outcome::Stopped(StopReason::NoReady));
    assert!(
        started.elapsed() < Duration::from_secs(4),
        "the connector was stopped, not awaited"
    );
}

fn a_crash_ends_the_session() {
    let vault = tempfile::tempdir().unwrap();
    let report = run_import(&mut Journal::new(vault.path()), &launch("crash")).unwrap();
    assert_eq!(report.outcome, Outcome::Stopped(StopReason::Ended));
}

fn batches_must_be_numbered_in_order() {
    let vault = tempfile::tempdir().unwrap();
    let report = run_import(&mut Journal::new(vault.path()), &launch("bad-seq")).unwrap();
    assert!(matches!(
        report.outcome,
        Outcome::Stopped(StopReason::BrokeProtocol(_))
    ));
}
