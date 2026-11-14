//! The connector host's throughput: a connector written with the Rust helper sends
//! 100 000 records, which the core checks and writes to the journal; then the same
//! records again, which the core skips as unchanged. Prints records per second.
//!
//! The same executable is the connector when started with `--fake-connector`.
#![allow(
    missing_docs,
    clippy::unwrap_used,
    clippy::print_stdout,
    clippy::cast_precision_loss
)]

use std::time::{Duration, Instant};

use ipse_core::connector::{Launch, Outcome, run_session};
use ipse_core::journal::Journal;
use ipse_protocol::connector::Session;
use ipse_protocol::{DoneSummary, Op, Record};
use serde_json::json;

const RECORDS: usize = 100_000;

fn main() {
    if std::env::args().any(|a| a == "--fake-connector") {
        connector();
        return;
    }
    let dir = tempfile::tempdir().unwrap();
    let mut journal = Journal::new(dir.path());
    let exe = std::env::current_exe().unwrap();
    let mut launch = Launch::new(
        vec![
            exe.to_string_lossy().into_owned(),
            "--fake-connector".into(),
        ],
        "fake",
        "fr",
        "Europe/Paris",
    );
    launch.idle_timeout = Duration::from_mins(1);

    for (label, expected) in [("first import", RECORDS), ("same records again", 0)] {
        let start = Instant::now();
        let report = run_session(&mut journal, &launch).unwrap();
        let took = start.elapsed();
        assert!(
            matches!(report.outcome, Outcome::Done(_)),
            "{:?}",
            report.outcome
        );
        assert_eq!(report.written, expected as u64);
        println!(
            "{label:<20} {:>9.0} records/s  ({took:.2?}, {} written)",
            RECORDS as f64 / took.as_secs_f64(),
            report.written
        );
    }
}

fn connector() {
    let (mut session, _hello) = Session::open(
        std::io::stdin().lock(),
        std::io::stdout().lock(),
        "fake",
        "0",
    )
    .unwrap();
    session.wait_start().unwrap();
    let observed_at = jiff::Timestamp::now();
    let records = (0..RECORDS).map(|i| Record {
        kind: "contact.card".into(),
        op: Op::Upsert,
        external_id: Some(format!("card-{i}")),
        observed_at,
        data: Some(json!({"name": format!("Personne {i}"), "phones": ["+33600000000"]})),
    });
    let sent = session.send(records, None).unwrap();
    session
        .done(DoneSummary {
            upserts: sent.accepted,
            ..DoneSummary::default()
        })
        .unwrap();
    session.finish().unwrap();
}
