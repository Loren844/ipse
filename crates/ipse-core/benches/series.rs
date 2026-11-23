//! Repeating events (issue #175): two years of a busy agenda, 7 300 events (ten a
//! day). Half repeat every week by title, weekday and time, a fifth come from
//! calendar rules, the rest happen once; every series is found in one pass.
#![allow(missing_docs, clippy::unwrap_used, clippy::print_stdout)]

use std::time::{Duration, Instant};

use ipse_core::search::SearchIndex;
use ipse_core::vault::{Language, Settings, Vault};

const DAYS: usize = 730;
const PER_DAY: usize = 10;

fn main() {
    let dir = tempfile::tempdir().unwrap();
    Vault::init(
        dir.path(),
        "Jules Moreau",
        Settings::new(Language::Fr, "Europe/Paris").unwrap(),
    )
    .unwrap();
    let first = jiff::civil::date(2025, 1, 1);
    let mut n = 0;
    for d in 0..DAYS {
        let day = first
            .checked_add(jiff::Span::new().days(i64::try_from(d).unwrap()))
            .unwrap();
        let folder = dir.path().join(format!("events/{}", day.year()));
        std::fs::create_dir_all(&folder).unwrap();
        for e in 0..PER_DAY {
            let (title, sources) = match e {
                0..5 => (format!("Habitude {e} du jour {}", d % 7), String::new()),
                5 | 6 => (
                    format!("Réunion {e}"),
                    format!(
                        "sources:\n  - \"calendar:rule-{e}-{}@example.org#{n}\"\n",
                        d % 7
                    ),
                ),
                _ => (format!("Moment {n}"), String::new()),
            };
            std::fs::write(
                folder.join(format!("e-{n}.md")),
                format!(
                    "---\nid: 01jcx8m2p4q6r8t0v2w5{n:06}\ntype: event\ncategory: social\ncreated: 2025-01-10T09:00:00+01:00\nupdated: 2025-01-10T09:00:00+01:00\n{sources}title: {title}\nstart: {day}T{:02}:00:00+01:00\n---\n",
                    8 + e,
                ),
            )
            .unwrap();
            n += 1;
        }
    }
    let index = SearchIndex::build(dir.path()).unwrap();
    let mut times: Vec<Duration> = Vec::new();
    for _ in 0..21 {
        let start = Instant::now();
        let series = index.series(|_| true);
        times.push(start.elapsed());
        assert_eq!(
            series.len(),
            DAYS * 7,
            "every habit and every calendar occurrence"
        );
    }
    times.sort();
    println!("series of {n} events: median {:.2?} (21 runs)", times[10]);
}
