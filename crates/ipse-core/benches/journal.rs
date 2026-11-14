//! The journal's throughput: appending 1 million small records in batches of 1 000, each
//! flushed to disk as the connector protocol requires, then reading them back. Prints
//! records per second and megabytes per second.
#![allow(
    missing_docs,
    clippy::unwrap_used,
    clippy::print_stdout,
    clippy::cast_precision_loss
)]

use std::time::Instant;

use ipse_core::journal::Journal;
use ipse_protocol::{MAX_BATCH_RECORDS, Op, Record};
use serde_json::json;

const RECORDS: usize = 1_000_000;

fn main() {
    let dir = tempfile::tempdir().unwrap();
    let observed_at = jiff::Timestamp::now();
    let mut journal = Journal::new(dir.path());

    let start = Instant::now();
    let mut batch = Vec::with_capacity(MAX_BATCH_RECORDS);
    for i in 0..RECORDS {
        batch.push(Record {
            kind: "media.playing".into(),
            op: Op::Upsert,
            external_id: Some(format!("track-{i}")),
            observed_at,
            data: Some(json!({"title": format!("Morceau {i}"), "artist": "Nina Simone", "seconds": i % 300})),
        });
        if batch.len() == MAX_BATCH_RECORDS {
            journal.append("music", &batch, None).unwrap();
            batch.clear();
        }
    }
    journal.save_index();
    let append = start.elapsed();

    let bytes: u64 = walk(&dir.path().join("journal"));
    let start = Instant::now();
    let read = journal.read("music", None).unwrap().count();
    let reading = start.elapsed();
    assert_eq!(read, RECORDS);

    let mb = bytes as f64 / 1_000_000.0;
    println!("{RECORDS} records, {mb:.0} MB on disk");
    println!(
        "append (batches of {MAX_BATCH_RECORDS}, each flushed)  {:>9.0} records/s  {:>6.1} MB/s  ({append:.2?})",
        RECORDS as f64 / append.as_secs_f64(),
        mb / append.as_secs_f64()
    );
    println!(
        "read                                  {:>9.0} records/s  {:>6.1} MB/s  ({reading:.2?})",
        RECORDS as f64 / reading.as_secs_f64(),
        mb / reading.as_secs_f64()
    );

    // A full re-sync of the same items: every one is unchanged and skipped.
    let start = Instant::now();
    for chunk in (0..RECORDS).collect::<Vec<_>>().chunks(MAX_BATCH_RECORDS) {
        let batch: Vec<Record> = chunk
            .iter()
            .map(|&i| Record {
                kind: "media.playing".into(),
                op: Op::Upsert,
                external_id: Some(format!("track-{i}")),
                observed_at,
                data: Some(json!({"title": format!("Morceau {i}"), "artist": "Nina Simone", "seconds": i % 300})),
            })
            .collect();
        assert!(journal.append("music", &batch, None).unwrap().is_empty());
    }
    let resync = start.elapsed();
    println!(
        "re-sync, nothing changed              {:>9.0} records/s  ({resync:.2?})",
        RECORDS as f64 / resync.as_secs_f64()
    );
}

fn walk(dir: &std::path::Path) -> u64 {
    std::fs::read_dir(dir)
        .unwrap()
        .map(|e| {
            let e = e.unwrap();
            if e.file_type().unwrap().is_dir() {
                walk(&e.path())
            } else {
                e.metadata().unwrap().len()
            }
        })
        .sum()
}
