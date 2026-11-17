//! A year of location history imported as a person does it (issue #106): the export of
//! an Android phone, with 5 stays a day, a path between them and the phone's raw
//! positions (about 400 000 points), read by the connector in its own process, written
//! to the journal, then turned into places and visits. Then the same file again, which
//! changes nothing.
#![allow(
    missing_docs,
    clippy::unwrap_used,
    clippy::print_stdout,
    clippy::cast_precision_loss // small counts turned into made-up coordinates
)]

use std::fmt::Write as _;
use std::process::Command;
use std::time::Instant;

const DAYS: usize = 365;
/// Raw positions a day: one every 2 minutes, 16 hours a day.
const RAW_PER_DAY: usize = 480;
/// Points of the path between two stays.
const PATH_POINTS: usize = 120;
/// Places the person goes to, besides home and work.
const PLACES: usize = 40;

fn export() -> (String, usize) {
    let mut segments = String::new();
    let mut raw = String::new();
    let mut points = 0;
    let first = jiff::civil::date(2025, 1, 1);
    for d in 0..DAYS {
        let day = first
            .checked_add(jiff::Span::new().days(i64::try_from(d).unwrap()))
            .unwrap();
        // Home 0–8, work 9–17 on weekdays, two places in the evening, home again.
        let stays: [(u32, u32, usize, &str); 5] = [
            (0, 8, 0, "HOME"),
            (9, 12, 1, "WORK"),
            (13, 17, 1, "WORK"),
            (18, 20, 2 + (d * 7) % PLACES, "UNKNOWN"),
            (21, 23, 2 + (d * 3 + 1) % PLACES, "UNKNOWN"),
        ];
        for (from, to, place, semantic) in stays {
            let (lat, lon) = (48.80 + place as f64 * 0.003, 2.30 + place as f64 * 0.002);
            if !segments.is_empty() {
                segments.push(',');
            }
            let _ = write!(
                segments,
                r#"{{"startTime":"{day}T{from:02}:00:00.000+01:00","endTime":"{day}T{to:02}:00:00.000+01:00","visit":{{"probability":0.9,"topCandidate":{{"placeId":"ChIJbench{place}","semanticType":"{semantic}","placeLocation":{{"latLng":"{lat:.4}°, {lon:.4}°"}}}}}}}}"#
            );
            let _ = write!(
                segments,
                r#",{{"startTime":"{day}T{to:02}:00:00.000+01:00","endTime":"{day}T{to:02}:59:00.000+01:00","timelinePath":["#
            );
            for p in 0..PATH_POINTS {
                if p > 0 {
                    segments.push(',');
                }
                let minute = p % 60;
                let _ = write!(
                    segments,
                    r#"{{"point":"{:.5}°, {:.5}°","time":"{day}T{to:02}:{minute:02}:{:02}.000+01:00"}}"#,
                    lat + p as f64 * 0.0001,
                    lon + p as f64 * 0.0001,
                    p / 60 * 30
                );
            }
            segments.push_str("]}");
            points += PATH_POINTS;
        }
        for r in 0..RAW_PER_DAY {
            if !raw.is_empty() {
                raw.push(',');
            }
            let minutes = 6 * 60 + r * 2;
            let _ = write!(
                raw,
                r#"{{"position":{{"LatLng":"{:.6}°, {:.6}°","accuracyMeters":{},"source":"WIFI","timestamp":"{day}T{:02}:{:02}:00.000+01:00"}}}}"#,
                48.85 + r as f64 * 0.00001,
                2.35 + r as f64 * 0.00001,
                5 + r % 20,
                minutes / 60,
                minutes % 60
            );
        }
        points += RAW_PER_DAY;
    }
    (
        format!(
            r#"{{"semanticSegments":[{segments}],"rawSignals":[{raw}],"userLocationProfile":{{"frequentPlaces":[{{"placeId":"ChIJbench0","placeLocation":"48.8000°, 2.3000°","label":"HOME"}}]}}}}"#
        ),
        points,
    )
}

fn main() {
    let dir = tempfile::tempdir().unwrap();
    let vault = dir.path().join("memoire");
    let file = dir.path().join("Timeline.json");
    let (text, points) = export();
    std::fs::write(&file, &text).unwrap();
    let ipse = env!("CARGO_BIN_EXE_ipse");
    let init = Command::new(ipse)
        .args([
            "init",
            vault.to_str().unwrap(),
            "--name",
            "Jules Moreau",
            "--time-zone",
            "Europe/Paris",
        ])
        .output()
        .unwrap();
    assert!(init.status.success());
    println!(
        "a year of location history: {DAYS} days, {} stays, {points} points, {:.0} MB",
        DAYS * 5,
        text.len() as f64 / 1e6
    );
    for label in ["first import", "same file again"] {
        let start = Instant::now();
        let out = Command::new(ipse)
            .args([
                "import",
                file.to_str().unwrap(),
                "--vault",
                vault.to_str().unwrap(),
            ])
            .output()
            .unwrap();
        let took = start.elapsed();
        assert!(
            out.status.success(),
            "{}",
            String::from_utf8_lossy(&out.stderr)
        );
        let said = String::from_utf8_lossy(&out.stdout);
        println!(
            "{label:<16} {took:>9.2?}  {}",
            said.lines().last().unwrap_or_default()
        );
    }
}
