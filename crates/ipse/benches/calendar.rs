//! Benchmark of the calendar connector on a 10 000-event file, run as the core runs it:
//! a separate process reading the file and sending its records.
#![allow(missing_docs, clippy::unwrap_used)]

use std::fmt::Write as _;
use std::io::Write as _;
use std::path::Path;
use std::process::{Command, Stdio};

use criterion::{Criterion, Throughput, criterion_group, criterion_main};

const EVENTS: usize = 10_000;
/// One event in 50 repeats every week.
const REPEATING_EVERY: usize = 50;

/// A Google-like export: mostly single events, some weekly ones.
fn calendar() -> String {
    let mut out = String::from(
        "BEGIN:VCALENDAR\r\nVERSION:2.0\r\nPRODID:-//Ipse//Bench//EN\r\nX-WR-TIMEZONE:Europe/Paris\r\n",
    );
    for i in 0..EVENTS {
        let month = i % 12 + 1;
        let day = i % 28 + 1;
        let hour = 8 + i % 10;
        let _ = write!(
            out,
            "BEGIN:VEVENT\r\nUID:event-{i}@example.org\r\nDTSTAMP:20261113T101500Z\r\n\
             DTSTART;TZID=Europe/Paris:2026{month:02}{day:02}T{hour:02}0000\r\n\
             DTEND;TZID=Europe/Paris:2026{month:02}{day:02}T{:02}0000\r\n\
             SUMMARY:Réunion numéro {i}\r\nLOCATION:Salle {}\\, bâtiment B\r\n\
             DESCRIPTION:Ordre du jour : point {i}\\, budget\\, questions diverses.\r\n\
             ORGANIZER;CN=Personne {}:mailto:personne{}@example.org\r\n\
             ATTENDEE;CN=Personne {};PARTSTAT=ACCEPTED:mailto:personne{}@example.org\r\n",
            hour + 1,
            i % 40,
            i % 300,
            i % 300,
            (i + 1) % 300,
            (i + 1) % 300,
        );
        if i % REPEATING_EVERY == 0 {
            out.push_str("RRULE:FREQ=WEEKLY\r\n");
        }
        out.push_str(
            "BEGIN:VALARM\r\nACTION:DISPLAY\r\nTRIGGER:-PT15M\r\nEND:VALARM\r\nEND:VEVENT\r\n",
        );
    }
    out.push_str("END:VCALENDAR\r\n");
    out
}

fn import(path: &Path) {
    let mut input = format!(
        "{{\"type\":\"hello\",\"protocol\":1,\"instance\":\"calendar\",\"mode\":\"import\",\"language\":\"en\",\"time_zone\":\"Europe/Paris\",\"cursor\":null,\"files\":[{}]}}\n{{\"type\":\"start\",\"full\":false}}\n",
        serde_json::to_string(path).unwrap()
    );
    for seq in 1..=200 {
        let _ = writeln!(input, "{{\"type\":\"ack\",\"seq\":{seq}}}");
    }
    input.push_str("{\"type\":\"bye\"}\n");
    let mut child = Command::new(env!("CARGO_BIN_EXE_ipse"))
        .args(["connector", "calendar"])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .unwrap();
    child
        .stdin
        .take()
        .unwrap()
        .write_all(input.as_bytes())
        .unwrap();
    let out = child.wait_with_output().unwrap();
    assert!(out.status.success());
    assert!(out.stdout.ends_with(b"\n"));
}

fn bench_calendar(c: &mut Criterion) {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("agenda.ics");
    std::fs::write(&path, calendar()).unwrap();

    let mut group = c.benchmark_group("calendar");
    group.sample_size(20);
    group.throughput(Throughput::Elements(EVENTS as u64));
    group.bench_function("import_10000_events", |b| b.iter(|| import(&path)));
    group.finish();
}

criterion_group!(benches, bench_calendar);
criterion_main!(benches);
