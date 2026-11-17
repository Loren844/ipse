//! The timeline view (issue #133): two years of a busy agenda, 7 300 events (ten a
//! day), each with people, some trips and celebrations, summed up per year and month
//! with the people seen most.
#![allow(missing_docs, clippy::unwrap_used, clippy::print_stdout)]

use std::time::{Duration, Instant};

use ipse_core::search::SearchIndex;
use ipse_core::vault::{Language, Settings, Vault};

const DAYS: usize = 730;
const PER_DAY: usize = 10;
const PEOPLE: usize = 500;

fn main() {
    let dir = tempfile::tempdir().unwrap();
    Vault::init(
        dir.path(),
        "Jules Moreau",
        Settings::new(Language::Fr, "Europe/Paris").unwrap(),
    )
    .unwrap();
    for i in 0..PEOPLE {
        std::fs::write(
            dir.path().join(format!("people/p-{i}.md")),
            format!("---\nid: 01jcx8m2p4q6r8t0v2w4{i:06}\ntype: person\ncategory: people\ncreated: 2025-01-10T09:00:00+01:00\nupdated: 2025-01-10T09:00:00+01:00\nname: Personne {i}\n---\n"),
        )
        .unwrap();
    }
    let first = jiff::civil::date(2025, 1, 1);
    let mut n = 0;
    for d in 0..DAYS {
        let day = first
            .checked_add(jiff::Span::new().days(i64::try_from(d).unwrap()))
            .unwrap();
        let folder = dir.path().join(format!("events/{}", day.year()));
        std::fs::create_dir_all(&folder).unwrap();
        for e in 0..PER_DAY {
            let kind = match n % 40 {
                0 => "kind: trip\n",
                1 => "kind: celebration\n",
                2..=9 => "kind: meal\n",
                _ => "",
            };
            std::fs::write(
                folder.join(format!("e-{n}.md")),
                format!(
                    "---\nid: 01jcx8m2p4q6r8t0v2w5{n:06}\ntype: event\ncategory: agenda\ncreated: 2025-01-10T09:00:00+01:00\nupdated: 2025-01-10T09:00:00+01:00\ntitle: Moment {n}\nstart: {day}T{:02}:00:00+01:00\n{kind}participants:\n  - \"[[p-{}]]\"\n  - \"[[p-{}]]\"\n---\n",
                    8 + e,
                    n % 7,
                    (n * 13) % PEOPLE
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
        let timeline = index.timeline(|_| true, |stem| index.person_path(stem, |_| true).is_some());
        times.push(start.elapsed());
        assert_eq!(timeline.years.len(), 2);
        assert_eq!(
            timeline.years.iter().map(|y| y.events).sum::<u32>() as usize,
            n
        );
    }
    times.sort();
    println!(
        "timeline of {n} events over two years: median {:.2?} (21 runs)",
        times[10]
    );
}
