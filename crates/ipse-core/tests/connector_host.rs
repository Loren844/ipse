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

use ipse_core::connector::{Launch, Outcome, StopReason, run_session};
use ipse_core::journal::Journal;
use ipse_protocol::{DoneSummary, ErrorCode, Mode, NeedKind, Op, Record, Secret};
use serde_json::{Value, json};

fn main() {
    let args: Vec<String> = std::env::args().collect();
    if args.get(1).map(String::as_str) == Some("--fake-connector") {
        fake_connector(args.get(2).map_or("", String::as_str));
        return;
    }
    let tests: [(&str, fn()); 12] = [
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
        (
            "a_complete_sync_records_what_disappeared",
            a_complete_sync_records_what_disappeared,
        ),
        (
            "sync_gets_secrets_config_and_full",
            sync_gets_secrets_config_and_full,
        ),
        (
            "the_rust_helper_pipelines_its_batches",
            the_rust_helper_pipelines_its_batches,
        ),
        (
            "an_unchanged_item_is_not_written_again",
            an_unchanged_item_is_not_written_again,
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
    if mode == "helper" {
        helper_connector();
        return;
    }
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
    let start = read();
    assert_eq!(start["type"], "start");
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
        "sync-complete" | "sync-partial" => {
            let record = |id: &str| {
                json!({"kind": "contact.card", "op": "upsert", "external_id": id,
                "observed_at": "2026-11-12T18:30:00Z", "data": {"n": id}})
            };
            say(json!({"type": "batch", "seq": 1, "records": [record("a"), record("b")]}));
            read();
            let complete = mode == "sync-complete";
            say(json!({"type": "done", "summary": {"upserts": 2, "complete": complete}}));
        }
        "echo" => {
            say(json!({"type": "progress", "done": 0, "message": format!(
                "mode={} config={} secret={} full={}",
                hello["mode"], hello["config"], hello["secrets"]["url"], start["full"]
            )}));
            say(json!({"type": "done", "summary": {}}));
        }
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

/// Records sent by the `helper` mode: five batches, the last one partly full.
const HELPER_RECORDS: usize = 4 * ipse_protocol::MAX_BATCH_RECORDS + 500;

/// A connector written with the Rust helper, which sends batches before their `ack`.
fn helper_connector() {
    use ipse_protocol::connector::Session;
    let (mut session, _hello) = Session::open(
        std::io::stdin().lock(),
        std::io::stdout().lock(),
        "fake",
        "0",
    )
    .unwrap();
    session.wait_start().unwrap();
    let observed_at = "2026-11-12T18:30:00Z".parse().unwrap();
    let records = (0..HELPER_RECORDS).map(|i| Record {
        kind: "contact.card".into(),
        op: Op::Upsert,
        external_id: Some(format!("c{i}")),
        observed_at,
        data: Some(json!({"n": i})),
    });
    let sent = session.send(records, Some(json!({"page": "end"}))).unwrap();
    session
        .done(DoneSummary {
            upserts: sent.accepted,
            ..DoneSummary::default()
        })
        .unwrap();
    session.finish().unwrap();
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
    let report = run_session(&mut journal, &launch("normal")).unwrap();

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
    run_session(&mut journal, &launch("normal")).unwrap();
    let report = run_session(&mut journal, &launch("normal")).unwrap();
    assert!(
        report.progress[0].contains(r#"cursor={"page":2}"#),
        "{:?}",
        report.progress
    );
}

fn invalid_json_stops_the_connector() {
    let vault = tempfile::tempdir().unwrap();
    let report = run_session(&mut Journal::new(vault.path()), &launch("invalid-json")).unwrap();
    assert!(matches!(
        report.outcome,
        Outcome::Stopped(StopReason::BrokeProtocol(_))
    ));
    assert_eq!(report.written, 0);
}

fn error_is_returned_with_its_message() {
    let vault = tempfile::tempdir().unwrap();
    let report = run_session(&mut Journal::new(vault.path()), &launch("error")).unwrap();
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
    let report = run_session(&mut Journal::new(vault.path()), &launch("need")).unwrap();
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
    let report = run_session(&mut Journal::new(vault.path()), &launch("no-ready")).unwrap();
    assert_eq!(report.outcome, Outcome::Stopped(StopReason::NoReady));
    assert!(
        started.elapsed() < Duration::from_secs(4),
        "the connector was stopped, not awaited"
    );
}

fn a_crash_ends_the_session() {
    let vault = tempfile::tempdir().unwrap();
    let report = run_session(&mut Journal::new(vault.path()), &launch("crash")).unwrap();
    assert_eq!(report.outcome, Outcome::Stopped(StopReason::Ended));
}

fn batches_must_be_numbered_in_order() {
    let vault = tempfile::tempdir().unwrap();
    let report = run_session(&mut Journal::new(vault.path()), &launch("bad-seq")).unwrap();
    assert!(matches!(
        report.outcome,
        Outcome::Stopped(StopReason::BrokeProtocol(_))
    ));
}

fn seeded() -> (tempfile::TempDir, Journal) {
    let vault = tempfile::tempdir().unwrap();
    let mut journal = Journal::new(vault.path());
    let record = |id: &str, op: Op| Record {
        kind: "contact.card".into(),
        op,
        external_id: Some(id.into()),
        observed_at: "2026-11-01T00:00:00Z".parse().unwrap(),
        data: (op == Op::Upsert).then(|| json!({"n": id})),
    };
    journal
        .append(
            "fake",
            &[
                record("a", Op::Upsert),
                record("b", Op::Upsert),
                record("c", Op::Upsert),
                record("d", Op::Upsert),
                record("d", Op::Delete),
            ],
            None,
        )
        .unwrap();
    (vault, journal)
}

fn latest_ops(journal: &Journal) -> Vec<(String, Op)> {
    let mut latest = std::collections::BTreeMap::new();
    for record in journal.read("fake", None).unwrap() {
        let record = record.unwrap();
        latest.insert(record.external_id.unwrap(), record.op);
    }
    latest.into_iter().collect()
}

fn a_complete_sync_records_what_disappeared() {
    let (_vault, mut journal) = seeded();
    let mut sync = launch("sync-complete");
    sync.mode = Mode::Sync;
    let report = run_session(&mut journal, &sync).unwrap();
    assert!(matches!(report.outcome, Outcome::Done(s) if s.complete));
    assert_eq!(report.removed, 1, "c only: d was already removed");
    assert_eq!(
        latest_ops(&journal),
        [
            ("a".into(), Op::Upsert),
            ("b".into(), Op::Upsert),
            ("c".into(), Op::Delete),
            ("d".into(), Op::Delete),
        ]
    );

    // A session that sent only what changed removes nothing.
    let (_vault, mut journal) = seeded();
    let mut partial = launch("sync-partial");
    partial.mode = Mode::Sync;
    assert_eq!(run_session(&mut journal, &partial).unwrap().removed, 0);
    // Nor does an import, even one that says it is complete.
    let (_vault, mut journal) = seeded();
    assert_eq!(
        run_session(&mut journal, &launch("sync-complete"))
            .unwrap()
            .removed,
        0
    );
}

fn sync_gets_secrets_config_and_full() {
    let vault = tempfile::tempdir().unwrap();
    let mut journal = Journal::new(vault.path());
    let mut sync = launch("echo");
    sync.mode = Mode::Sync;
    sync.full = true;
    sync.config.insert("name".into(), json!("Perso"));
    sync.secrets
        .insert("url".into(), Secret::new("https://example.org/private.ics"));
    let report = run_session(&mut journal, &sync).unwrap();
    assert_eq!(
        report.progress,
        [
            r#"mode="sync" config={"name":"Perso"} secret="https://example.org/private.ics" full=true"#
        ]
    );
    assert!(
        !format!("{sync:?}").contains("private.ics"),
        "secrets never show in debug output"
    );
}

fn the_rust_helper_pipelines_its_batches() {
    let vault = tempfile::tempdir().unwrap();
    let mut journal = Journal::new(vault.path());
    let report = run_session(&mut journal, &launch("helper")).unwrap();

    assert!(
        matches!(report.outcome, Outcome::Done(s) if s.upserts == HELPER_RECORDS as u64),
        "{:?}",
        report.outcome
    );
    assert_eq!(report.written, HELPER_RECORDS as u64);
    assert_eq!(
        journal.cursor("fake").unwrap(),
        Some(json!({"page": "end"}))
    );
    let ids: Vec<String> = journal
        .read("fake", None)
        .unwrap()
        .map(|r| r.unwrap().external_id.unwrap())
        .collect();
    assert_eq!(ids.len(), HELPER_RECORDS);
    assert_eq!(ids.first().map(String::as_str), Some("c0"));
    assert_eq!(
        ids.last().map(String::as_str),
        Some(format!("c{}", HELPER_RECORDS - 1).as_str())
    );
}

fn an_unchanged_item_is_not_written_again() {
    let vault = tempfile::tempdir().unwrap();
    let mut journal = Journal::new(vault.path());
    run_session(&mut journal, &launch("helper")).unwrap();
    // A new journal reads the saved table of latest records.
    let mut journal = Journal::new(vault.path());
    let report = run_session(&mut journal, &launch("helper")).unwrap();
    assert!(matches!(report.outcome, Outcome::Done(_)));
    assert_eq!(report.written, 0);
    assert_eq!(journal.read("fake", None).unwrap().count(), HELPER_RECORDS);
}
