//! The calendar connector, run as the core runs it: a separate process speaking the
//! protocol on its standard input and output.
//!
//! Which occurrences of a repeating event are sent depends on today's date (one year
//! back, 90 days ahead): those are checked with a fixed date in the connector's own
//! tests. Here, only what does not depend on the date is checked.

// Test helpers may panic: a failed expectation is a failed test.
#![allow(clippy::expect_used, clippy::unwrap_used, clippy::format_push_string)]

use std::collections::HashMap;
use std::io::Write as _;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use serde_json::{Value, json};

fn fixture(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/calendar")
        .join(name)
}

/// Runs a session and returns the connector's messages. The core acknowledges every
/// batch in advance: the connector reads the answers in order.
fn session(files: &[PathBuf], language: &str) -> Vec<Value> {
    let mut input = String::new();
    let hello = json!({
        "type": "hello", "protocol": 1, "instance": "calendar", "mode": "import",
        "language": language, "time_zone": "Europe/Paris", "cursor": null, "files": files,
    });
    input.push_str(&format!("{hello}\n{{\"type\":\"start\",\"full\":false}}\n"));
    for seq in 1..=4 {
        input.push_str(&format!("{{\"type\":\"ack\",\"seq\":{seq}}}\n"));
    }
    input.push_str("{\"type\":\"bye\"}\n");

    let mut child = Command::new(env!("CARGO_BIN_EXE_ipse"))
        .args(["connector", "calendar"])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    child
        .stdin
        .take()
        .unwrap()
        .write_all(input.as_bytes())
        .unwrap();
    let out = child.wait_with_output().unwrap();
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    String::from_utf8(out.stdout)
        .unwrap()
        .lines()
        .map(|l| serde_json::from_str(l).unwrap())
        .collect()
}

fn types(messages: &[Value]) -> Vec<&str> {
    messages
        .iter()
        .map(|m| m["type"].as_str().unwrap())
        .collect()
}

/// The records sent, by external id.
fn records(messages: &[Value]) -> HashMap<String, Value> {
    messages
        .iter()
        .filter(|m| m["type"] == "batch")
        .flat_map(|m| m["records"].as_array().unwrap())
        .map(|r| (r["external_id"].as_str().unwrap().to_owned(), r.clone()))
        .collect()
}

#[test]
fn imports_a_google_calendar_export() {
    let messages = session(&[fixture("google.ics")], "en");
    assert_eq!(types(&messages), ["ready", "batch", "progress", "done"]);
    assert_eq!(messages[0]["connector"], "calendar");
    assert_eq!(messages[2]["message"], "1 unreadable event(s) skipped.");
    let records = records(&messages);
    for (id, r) in &records {
        assert_eq!(r["kind"], "calendar.event", "{id}");
    }

    let dinner = &records["5f2a9c1e8b7d4c3a@google.com"]["data"];
    assert_eq!(
        *dinner,
        json!({
            "uid": "5f2a9c1e8b7d4c3a@google.com",
            "title": "Dîner chez Julie",
            "start": "2026-11-14T20:00:00+01:00",
            "end": "2026-11-14T23:00:00+01:00",
            "location": "12 Rue des Lilas, 69003 Lyon, France",
            "description": "Apporter le dessert.\nCode de la porte : 1234B.",
            "status": "confirmed",
            "organizer": {"name": "Julie Dupont", "email": "julie.dupont@example.org"},
            "attendees": [
                {"name": "Julie Dupont", "email": "julie.dupont@example.org", "response": "accepted"},
                {"email": "marc.leroy@example.org", "response": "needs-action"},
            ],
        }),
        "UTC times shown in the vault's time zone; the alarm left out"
    );

    let yoga = &records["3k9w2m4p6q8r@google.com"]["data"];
    assert_eq!(yoga["rule"], "FREQ=WEEKLY;BYDAY=TU");
    assert_eq!(yoga["start"], "2026-11-03T18:30:00+01:00");
    assert_eq!(yoga["time_zone"], "Europe/Paris");

    let moved = &records["3k9w2m4p6q8r@google.com#2026-11-17T17:30:00Z"]["data"];
    assert_eq!(moved["title"], "Yoga (décalé au mercredi)");
    assert_eq!(moved["start"], "2026-11-18T18:30:00+01:00");
    assert_eq!(moved["recurrence_id"], "2026-11-17T17:30:00Z");

    let christmas = &records["7t1v3x5z@google.com"]["data"];
    assert_eq!(christmas["start"], "2026-12-24");
    assert_eq!(christmas["end"], "2026-12-27");

    let mut upserts = 0;
    for (id, r) in &records {
        if let Some(rid) = r["data"]["recurrence_id"].as_str() {
            assert_eq!(*id, format!("{}#{rid}", r["data"]["uid"].as_str().unwrap()));
            assert!(r["data"].get("rule").is_none());
        }
        if r["op"] == "upsert" {
            upserts += 1;
        } else {
            assert_eq!(
                (r["op"].as_str(), r.get("data")),
                (Some("delete"), None),
                "{id}"
            );
            assert_eq!(id, "3k9w2m4p6q8r@google.com#2026-11-24T17:30:00Z");
        }
    }
    assert_eq!(messages[3]["summary"]["upserts"], upserts);
}

#[test]
fn imports_an_apple_calendar_export() {
    let messages = session(&[fixture("apple.ics")], "fr");
    assert_eq!(types(&messages), ["ready", "batch", "done"]);
    let records = records(&messages);

    let dentist = &records["4B2E6F1A-3C5D-4E7F-9A0B-1C2D3E4F5A6B"]["data"];
    assert_eq!(dentist["title"], "Dentiste");
    assert_eq!(dentist["start"], "2026-11-20T10:00:00+01:00");
    assert_eq!(dentist["end"], "2026-11-20T11:00:00+01:00");
    assert_eq!(
        dentist["location"],
        "Cabinet du Dr Martin\n8 Place Bellecour, 69002 Lyon"
    );

    let birthday = &records["8D7C6B5A-4F3E-2D1C-0B9A-8F7E6D5C4B3A"]["data"];
    assert_eq!(birthday["rule"], "FREQ=YEARLY");
    assert_eq!(
        (birthday["start"].as_str(), birthday["end"].as_str()),
        (Some("2020-03-12"), Some("2020-03-13"))
    );
    let occurrences: Vec<&Value> = records
        .values()
        .filter(|r| r["data"]["uid"] == "8D7C6B5A-4F3E-2D1C-0B9A-8F7E6D5C4B3A")
        .filter(|r| r["data"].get("recurrence_id").is_some())
        .collect();
    assert!(!occurrences.is_empty(), "15 months always hold a 12 March");
    for o in occurrences {
        assert!(o["data"]["start"].as_str().unwrap().ends_with("-03-12"));
        assert!(o["data"]["end"].as_str().unwrap().ends_with("-03-13"));
    }

    let workshop = &records["6A5B4C3D-2E1F-0A9B-8C7D-6E5F4A3B2C1D"]["data"];
    assert_eq!(workshop["status"], "cancelled");
}

#[test]
fn the_same_file_gives_the_same_records() {
    let files = [fixture("google.ics"), fixture("apple.ics")];
    let ids = |messages: &[Value]| {
        let mut ids: Vec<String> = records(messages).into_keys().collect();
        ids.sort();
        ids
    };
    assert_eq!(ids(&session(&files, "en")), ids(&session(&files, "en")));
}

#[test]
fn a_missing_file_is_reported_in_plain_words() {
    let messages = session(&[PathBuf::from("/no/such/agenda.ics")], "en");
    assert_eq!(types(&messages), ["ready", "error"]);
    assert_eq!(messages[1]["code"], "bad_input");
    let message = messages[1]["message"].as_str().unwrap();
    assert!(
        message.contains(".ics") && !message.contains("/no/such"),
        "{message}"
    );
}

#[test]
fn the_core_runs_it_and_fills_the_journal() {
    use ipse_core::connector::{Launch, Outcome, run_import};
    use ipse_core::journal::Journal;

    let vault = tempfile::tempdir().unwrap();
    let mut journal = Journal::new(vault.path());
    let mut launch = Launch::new(
        vec![
            env!("CARGO_BIN_EXE_ipse").into(),
            "connector".into(),
            "calendar".into(),
        ],
        "calendar",
        "en",
        "Europe/Paris",
    );
    launch.files = vec![fixture("google.ics")];
    launch.kinds = Some(vec!["calendar.event".into()]);

    let report = run_import(&mut journal, &launch).unwrap();
    assert!(matches!(report.outcome, Outcome::Done(_)), "{report:?}");
    assert_eq!(report.rejected, 0);
    let titles: Vec<String> = journal
        .read("calendar", Some(&["calendar.event"]))
        .unwrap()
        .filter_map(|r| r.unwrap().data)
        .filter_map(|d| d["title"].as_str().map(str::to_owned))
        .collect();
    assert!(
        titles.contains(&"Dîner chez Julie".to_owned()),
        "{titles:?}"
    );
    assert!(titles.contains(&"Noël chez les parents".to_owned()));
}
