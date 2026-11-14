//! The journal over time: recovery, unchanged items, sealed months, time series and
//! reading a period (`docs/spec/journal.md`, sections 4, 5 and 7).

// Test helpers may panic: a failed expectation is a failed test.
#![allow(clippy::expect_used, clippy::unwrap_used)]

use std::path::{Path, PathBuf};

use ipse_core::journal::{Journal, JournalRecord};
use ipse_protocol::{Op, Record};
use jiff::Timestamp;
use serde_json::{Value, json};

fn upsert(id: &str, observed_at: &str, data: Value) -> Record {
    Record {
        kind: "contact.card".into(),
        op: Op::Upsert,
        external_id: Some(id.into()),
        observed_at: observed_at.parse().unwrap(),
        data: Some(data),
    }
}

fn all(journal: &Journal, source: &str) -> Vec<JournalRecord> {
    journal
        .read(source, None)
        .unwrap()
        .collect::<Result<Vec<_>, _>>()
        .unwrap()
}

/// The names of the files under `dir`, sorted.
fn file_names(dir: &Path) -> Vec<String> {
    let mut out = Vec::new();
    let mut stack = vec![dir.to_owned()];
    while let Some(dir) = stack.pop() {
        for entry in std::fs::read_dir(&dir).unwrap() {
            let path = entry.unwrap().path();
            if path.is_dir() {
                stack.push(path);
            } else {
                out.push(path.file_name().unwrap().to_string_lossy().into_owned());
            }
        }
    }
    out.sort();
    out
}

/// A month before this one, in UTC, as `YYYY-MM`, and a time in it.
fn last_month() -> (String, String) {
    let now = Timestamp::now().to_zoned(jiff::tz::TimeZone::UTC);
    let first = now.date().first_of_month().yesterday().unwrap();
    let month = format!("{:04}-{:02}", first.year(), first.month());
    (month.clone(), format!("{month}-15T12:00:00Z"))
}

fn now_text() -> String {
    Timestamp::now().to_string()
}

#[test]
fn an_unchanged_item_is_not_written_again() {
    let vault = tempfile::tempdir().unwrap();
    let now = now_text();
    let mut journal = Journal::new(vault.path());
    let first = journal
        .append(
            "contacts",
            &[
                upsert("a", &now, json!({"name": "Ana"})),
                upsert("b", &now, json!({"name": "Bo"})),
            ],
            None,
        )
        .unwrap();
    assert_eq!(first.len(), 2);

    let again = journal
        .append(
            "contacts",
            &[
                upsert("a", &now, json!({"name": "Ana"})),
                upsert("b", &now, json!({"name": "Bob"})),
            ],
            None,
        )
        .unwrap();
    assert_eq!(again.len(), 1, "only the changed one");
    assert_eq!(again[0].external_id.as_deref(), Some("b"));
    drop(journal);

    // The table of latest records is saved, and used again.
    assert!(
        vault
            .path()
            .join(".ipse/index/journal/contacts.bin")
            .is_file()
    );
    let mut journal = Journal::new(vault.path());
    let unchanged = journal
        .append(
            "contacts",
            &[upsert("b", &now, json!({"name": "Bob"}))],
            None,
        )
        .unwrap();
    assert!(unchanged.is_empty());

    // Removed, then the same data again: written, the item is back.
    let delete = Record {
        kind: "contact.card".into(),
        op: Op::Delete,
        external_id: Some("a".into()),
        observed_at: now.parse().unwrap(),
        data: None,
    };
    assert_eq!(
        journal.append("contacts", &[delete], None).unwrap().len(),
        1
    );
    let back = journal
        .append(
            "contacts",
            &[upsert("a", &now, json!({"name": "Ana"}))],
            None,
        )
        .unwrap();
    assert_eq!(back.len(), 1);
    drop(journal);

    // Without its table, or with one behind the journal, it is read again from it.
    std::fs::remove_dir_all(vault.path().join(".ipse/index/journal")).unwrap();
    let mut journal = Journal::new(vault.path());
    assert!(
        journal
            .append(
                "contacts",
                &[upsert("a", &now, json!({"name": "Ana"}))],
                None
            )
            .unwrap()
            .is_empty()
    );
    drop(journal);
    let table = vault.path().join(".ipse/index/journal/contacts.bin");
    let stale = std::fs::read(&table).unwrap();
    let mut other = Journal::new(vault.path());
    other
        .append(
            "contacts",
            &[upsert("a", &now, json!({"name": "Ana Silva"}))],
            None,
        )
        .unwrap();
    drop(other);
    std::fs::write(&table, &stale).unwrap();
    let mut journal = Journal::new(vault.path());
    assert!(
        journal
            .append(
                "contacts",
                &[upsert("a", &now, json!({"name": "Ana Silva"}))],
                None
            )
            .unwrap()
            .is_empty(),
        "the stale table is not trusted"
    );
    assert_eq!(all(&journal, "contacts").len(), 6);
}

#[test]
fn a_torn_last_line_is_cut_before_the_next_write() {
    let vault = tempfile::tempdir().unwrap();
    let now = now_text();
    let mut journal = Journal::new(vault.path());
    journal
        .append("contacts", &[upsert("a", &now, json!({"n": 1}))], None)
        .unwrap();
    drop(journal);
    let segment: PathBuf = std::fs::read_dir(vault.path().join("journal/contacts"))
        .unwrap()
        .flat_map(|y| std::fs::read_dir(y.unwrap().path()).unwrap())
        .map(|e| e.unwrap().path())
        .find(|p| p.extension().is_some_and(|e| e == "jsonl"))
        .unwrap();
    let mut text = std::fs::read_to_string(&segment).unwrap();
    text.push_str("{\"v\":1,\"id\":\"01jc");
    std::fs::write(&segment, &text).unwrap();

    let journal = Journal::new(vault.path());
    let mut reader = journal.read("contacts", None).unwrap();
    assert_eq!(reader.by_ref().count(), 1);
    assert_eq!(
        reader.skipped_malformed(),
        1,
        "read as a broken line, not an error"
    );
    drop(reader);

    let mut journal = Journal::new(vault.path());
    journal
        .append("contacts", &[upsert("b", &now, json!({"n": 2}))], None)
        .unwrap();
    assert_eq!(journal.recovered(), 1);
    let mut reader = journal.read("contacts", None).unwrap();
    assert_eq!(reader.by_ref().count(), 2);
    assert_eq!(reader.skipped_malformed(), 0, "the torn line is gone");
}

#[test]
fn a_new_month_seals_the_past_ones_and_both_are_read() {
    let vault = tempfile::tempdir().unwrap();
    let (month, then) = last_month();
    let mut journal = Journal::new(vault.path());
    journal
        .append(
            "contacts",
            &[
                upsert("a", &then, json!({"n": 1})),
                upsert("b", &then, json!({"n": 2})),
            ],
            None,
        )
        .unwrap();
    drop(journal);
    assert!(file_names(&vault.path().join("journal")).contains(&format!("{month}.jsonl")));

    let mut journal = Journal::new(vault.path());
    journal
        .append(
            "contacts",
            &[upsert("c", &now_text(), json!({"n": 3}))],
            None,
        )
        .unwrap();
    let names = file_names(&vault.path().join("journal"));
    assert!(names.contains(&format!("{month}.jsonl.zst")), "{names:?}");
    assert!(
        names.contains(&format!("{month}.jsonl.zst.sha256")),
        "{names:?}"
    );
    assert!(
        !names.contains(&format!("{month}.jsonl")),
        "the open file is gone"
    );

    let ids: Vec<String> = all(&journal, "contacts")
        .into_iter()
        .filter_map(|r| r.external_id)
        .collect();
    assert_eq!(ids, ["a", "b", "c"], "in the order they were written");

    // A late record of the sealed month goes to a new open file, read after the
    // sealed one, then sealed with it next time.
    journal
        .append("contacts", &[upsert("d", &then, json!({"n": 4}))], None)
        .unwrap();
    let ids: Vec<String> = all(&journal, "contacts")
        .into_iter()
        .filter_map(|r| r.external_id)
        .collect();
    assert_eq!(ids, ["a", "b", "d", "c"], "by month, then as written");
    drop(journal);
    let mut journal = Journal::new(vault.path());
    journal
        .append(
            "contacts",
            &[upsert("e", &now_text(), json!({"n": 5}))],
            None,
        )
        .unwrap();
    let names = file_names(&vault.path().join("journal"));
    assert!(!names.contains(&format!("{month}.jsonl")), "{names:?}");
    assert_eq!(all(&journal, "contacts").len(), 5);
}

#[test]
fn a_damaged_sealed_month_is_reported_and_kept() {
    let vault = tempfile::tempdir().unwrap();
    let (month, then) = last_month();
    let mut journal = Journal::new(vault.path());
    journal
        .append("contacts", &[upsert("a", &then, json!({"n": 1}))], None)
        .unwrap();
    drop(journal);
    let mut journal = Journal::new(vault.path());
    journal
        .append(
            "contacts",
            &[upsert("b", &now_text(), json!({"n": 2}))],
            None,
        )
        .unwrap();
    drop(journal);

    let year = &month[..4];
    let sealed = vault
        .path()
        .join(format!("journal/contacts/{year}/{month}.jsonl.zst"));
    let mut bytes = std::fs::read(&sealed).unwrap();
    bytes[0] ^= 0xff;
    std::fs::write(&sealed, &bytes).unwrap();

    let journal = Journal::new(vault.path());
    let mut reader = journal.read("contacts", None).unwrap();
    let ids: Vec<String> = reader
        .by_ref()
        .map(|r| r.unwrap().external_id.unwrap())
        .collect();
    assert_eq!(ids, ["b"], "the damaged month is skipped, the rest is read");
    assert_eq!(reader.damaged().len(), 1);
    drop(reader);
    drop(journal);

    // A late record of that month stays open: the damaged file is never overwritten.
    let mut journal = Journal::new(vault.path());
    journal
        .append("contacts", &[upsert("c", &then, json!({"n": 3}))], None)
        .unwrap();
    drop(journal);
    let mut journal = Journal::new(vault.path());
    journal
        .append(
            "contacts",
            &[upsert("d", &now_text(), json!({"n": 4}))],
            None,
        )
        .unwrap();
    assert_eq!(std::fs::read(&sealed).unwrap(), bytes);
    assert!(
        vault
            .path()
            .join(format!("journal/contacts/{year}/{month}.jsonl"))
            .is_file()
    );

    let damaged = ipse_core::journal::damaged_segments(vault.path()).unwrap();
    assert_eq!(damaged.len(), 1);
    assert!(damaged[0].ends_with(format!("{month}.jsonl.zst")));
}

#[test]
fn a_time_series_batch_round_trips() {
    let vault = tempfile::tempdir().unwrap();
    let mut journal = Journal::new(vault.path());
    let data = json!({
        "t": [1_794_466_800, 1_794_466_860, 1_794_466_920],
        "lat": [45.7578, 45.7581, 45.7585],
        "lon": [4.832, 4.8324, 4.8331],
        "acc": [12, 9, 10],
    });
    let batch = Record {
        kind: "location.points".into(),
        op: Op::Observe,
        external_id: None,
        observed_at: "2026-11-12T07:00:00Z".parse().unwrap(),
        data: Some(data.clone()),
    };
    journal
        .append("location", &[batch.clone(), batch], None)
        .unwrap();
    let back = all(&journal, "location");
    assert_eq!(back.len(), 2, "observations are never skipped as unchanged");
    assert_eq!(back[0].data.as_ref(), Some(&data));
    assert_eq!(back[0].op, Op::Observe);
    assert!(back[0].hash.is_none());
}

#[test]
fn reading_a_period() {
    let vault = tempfile::tempdir().unwrap();
    let mut journal = Journal::new(vault.path());
    let records: Vec<Record> = [
        "2026-09-30T23:59:59Z",
        "2026-10-01T00:00:00Z",
        "2026-10-15T08:00:00Z",
        "2026-11-01T00:00:00Z",
    ]
    .iter()
    .enumerate()
    .map(|(i, at)| upsert(&format!("r{i}"), at, json!({"i": i})))
    .collect();
    journal.append("contacts", &records, None).unwrap();
    let october: Vec<String> = journal
        .read_between(
            "contacts",
            None,
            "2026-10-01T00:00:00Z".parse().unwrap(),
            "2026-11-01T00:00:00Z".parse().unwrap(),
        )
        .unwrap()
        .map(|r| r.unwrap().external_id.unwrap())
        .collect();
    assert_eq!(october, ["r1", "r2"], "from included, to excluded");
}
