//! The raw journal as it sits on disk.

// Test helpers may panic: a failed expectation is a failed test.
#![allow(clippy::expect_used, clippy::unwrap_used)]

use std::io::Write as _;

use ipse_core::Error;
use ipse_core::journal::{Journal, JournalRecord, data_hash};
use ipse_core::vault::Id;
use ipse_protocol::{Op, Record};
use serde_json::json;

fn record(
    op: Op,
    external_id: Option<&str>,
    observed_at: &str,
    data: Option<serde_json::Value>,
) -> Record {
    Record {
        kind: "contact.card".into(),
        op,
        external_id: external_id.map(Into::into),
        observed_at: observed_at.parse().unwrap(),
        data,
    }
}

fn read_all(journal: &Journal, source: &str) -> Vec<JournalRecord> {
    journal
        .read(source, None)
        .unwrap()
        .collect::<Result<Vec<_>, _>>()
        .unwrap()
}

#[test]
fn a_batch_is_written_then_read_back() {
    let vault = tempfile::tempdir().unwrap();
    let mut journal = Journal::new(vault.path());
    let batch = [
        record(
            Op::Upsert,
            Some("c-1"),
            "2026-11-12T18:30:00Z",
            Some(json!({"fn": "Ana Silva"})),
        ),
        record(Op::Delete, Some("c-2"), "2026-11-12T18:30:01Z", None),
        record(
            Op::Observe,
            None,
            "2026-11-12T18:30:02Z",
            Some(json!({"track": "x"})),
        ),
    ];
    let written = journal.append("contacts", &batch, None).unwrap();
    assert_eq!(written.len(), 3);
    assert!(
        written[0]
            .hash
            .as_deref()
            .is_some_and(|h| h.starts_with("b3:"))
    );
    assert!(
        written[1].hash.is_none() && written[2].hash.is_none(),
        "hash only for upsert"
    );
    for (w, r) in written.iter().zip(&batch) {
        assert_eq!(w.v, 1);
        assert_eq!(w.source, "contacts");
        assert_eq!(
            (&w.kind, w.op, &w.external_id, w.observed_at),
            (&r.kind, r.op, &r.external_id, r.observed_at)
        );
        assert!(Id::parse(&w.id).is_some(), "ids are lowercase ULIDs");
    }
    assert!(
        written.windows(2).all(|w| w[0].id < w[1].id),
        "ids sort by write time"
    );

    assert!(
        vault
            .path()
            .join("journal/contacts/2026/2026-11.jsonl")
            .is_file()
    );
    assert_eq!(read_all(&journal, "contacts"), written);
}

#[test]
fn each_month_has_its_segment_and_reading_follows_them() {
    let vault = tempfile::tempdir().unwrap();
    let mut journal = Journal::new(vault.path());
    let november = journal
        .append(
            "contacts",
            &[record(Op::Delete, Some("b"), "2026-11-01T00:00:00Z", None)],
            None,
        )
        .unwrap();
    let october = journal
        .append(
            "contacts",
            &[record(Op::Delete, Some("a"), "2026-10-31T23:59:59Z", None)],
            None,
        )
        .unwrap();
    assert!(
        vault
            .path()
            .join("journal/contacts/2026/2026-10.jsonl")
            .is_file()
    );
    assert!(
        vault
            .path()
            .join("journal/contacts/2026/2026-11.jsonl")
            .is_file()
    );

    let ids: Vec<_> = read_all(&journal, "contacts")
        .into_iter()
        .map(|r| r.external_id)
        .collect();
    assert_eq!(
        ids,
        [
            october[0].external_id.clone(),
            november[0].external_id.clone()
        ]
    );
}

#[test]
fn month_is_taken_in_utc() {
    let vault = tempfile::tempdir().unwrap();
    let mut journal = Journal::new(vault.path());
    journal
        .append(
            "contacts",
            &[record(
                Op::Delete,
                Some("a"),
                "2026-12-01T00:30:00+01:00",
                None,
            )],
            None,
        )
        .unwrap();
    assert!(
        vault
            .path()
            .join("journal/contacts/2026/2026-11.jsonl")
            .is_file()
    );
}

#[test]
fn hash_ignores_key_order() {
    let vault = tempfile::tempdir().unwrap();
    let mut journal = Journal::new(vault.path());
    let a = record(
        Op::Upsert,
        Some("c"),
        "2026-11-12T18:30:00Z",
        Some(json!({"a": 1, "b": [1, 2]})),
    );
    let b = record(
        Op::Upsert,
        Some("c"),
        "2026-11-12T18:30:00Z",
        Some(serde_json::from_str(r#"{"b":[1,2],"a":1}"#).unwrap()),
    );
    let written = journal.append("contacts", &[a, b], None).unwrap();
    assert_eq!(written[0].hash, written[1].hash);
    assert_eq!(
        written[0].hash.as_deref(),
        Some(data_hash(&json!({"b": [1, 2], "a": 1})).as_str())
    );
}

#[test]
fn unknown_versions_kinds_and_broken_lines_are_skipped_and_counted() {
    let vault = tempfile::tempdir().unwrap();
    let mut journal = Journal::new(vault.path());
    journal
        .append(
            "contacts",
            &[record(Op::Delete, Some("a"), "2026-11-12T18:30:00Z", None)],
            None,
        )
        .unwrap();
    let segment = vault.path().join("journal/contacts/2026/2026-11.jsonl");
    let mut file = std::fs::OpenOptions::new()
        .append(true)
        .open(&segment)
        .unwrap();
    writeln!(file, r#"{{"v":2,"something":"new"}}"#).unwrap();
    writeln!(file, "not json").unwrap();
    writeln!(file).unwrap();
    drop(file);
    journal
        .append(
            "contacts",
            &[Record {
                kind: "calendar.event".into(),
                ..record(Op::Delete, Some("e"), "2026-11-12T18:31:00Z", None)
            }],
            None,
        )
        .unwrap();

    let mut reader = journal.read("contacts", Some(&["contact.card"])).unwrap();
    let records: Vec<_> = reader.by_ref().collect::<Result<_, _>>().unwrap();
    assert_eq!(records.len(), 1);
    assert_eq!(
        reader.skipped_unknown(),
        2,
        "version 2 and the calendar kind"
    );
    assert_eq!(reader.skipped_malformed(), 1);
}

#[test]
fn cursor_is_saved_with_its_batch() {
    let vault = tempfile::tempdir().unwrap();
    let mut journal = Journal::new(vault.path());
    assert_eq!(journal.cursor("contacts").unwrap(), None);
    let cursor = json!({"sync_token": "abc"});
    journal
        .append(
            "contacts",
            &[record(Op::Delete, Some("a"), "2026-11-12T18:30:00Z", None)],
            Some(&cursor),
        )
        .unwrap();
    assert_eq!(journal.cursor("contacts").unwrap(), Some(cursor));
}

#[test]
fn invalid_input_writes_nothing() {
    let vault = tempfile::tempdir().unwrap();
    let mut journal = Journal::new(vault.path());
    let bad = [
        record(Op::Delete, Some("a"), "2026-11-12T18:30:00Z", None),
        record(Op::Upsert, None, "2026-11-12T18:30:00Z", Some(json!({}))),
    ];
    let err = journal.append("contacts", &bad, None).unwrap_err();
    assert!(
        matches!(&err, Error::Refused(m) if m.contains("Record 2")),
        "{err}"
    );
    assert!(!vault.path().join("journal/contacts").exists());

    let err = journal.append("../escape", &bad[..1], None).unwrap_err();
    assert!(matches!(err, Error::Refused(_)));
    assert!(!vault.path().join("escape").exists());
}

#[test]
fn reading_a_source_with_nothing_yet() {
    let vault = tempfile::tempdir().unwrap();
    let journal = Journal::new(vault.path());
    assert!(read_all(&journal, "contacts").is_empty());
}
