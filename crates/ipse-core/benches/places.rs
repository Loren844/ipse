//! The map (issue #135): two years of a busy agenda, 7 300 events (ten a day) at 300
//! places, each with people, summed up per place with the people seen there most.
#![allow(missing_docs, clippy::unwrap_used, clippy::print_stdout)]

use std::time::{Duration, Instant};

use ipse_core::search::SearchIndex;
use ipse_core::vault::{Language, Settings, Vault};

const DAYS: usize = 730;
const PER_DAY: usize = 10;
const PEOPLE: usize = 500;
const PLACES: usize = 300;

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
    for i in 0..PLACES {
        std::fs::write(
            dir.path().join(format!("places/l-{i}.md")),
            format!("---\nid: 01jcx8m2p4q6r8t0v2w6{i:06}\ntype: place\ncategory: location\ncreated: 2025-01-10T09:00:00+01:00\nupdated: 2025-01-10T09:00:00+01:00\nname: Lieu {i}\ngeo: [48.{i:04}, 2.{i:04}]\n---\n"),
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
            std::fs::write(
                folder.join(format!("e-{n}.md")),
                format!(
                    "---\nid: 01jcx8m2p4q6r8t0v2w5{n:06}\ntype: event\ncategory: location\ncreated: 2025-01-10T09:00:00+01:00\nupdated: 2025-01-10T09:00:00+01:00\ntitle: Moment {n}\nstart: {day}T{:02}:00:00+01:00\nplace: \"[[l-{}]]\"\nparticipants:\n  - \"[[p-{}]]\"\n---\n",
                    8 + e,
                    (n * 7) % PLACES,
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
        let places = index.places_visited(|_| true, |stem| stem.starts_with("p-"));
        times.push(start.elapsed());
        assert_eq!(places.len(), PLACES);
        assert_eq!(places.iter().map(|p| p.events).sum::<u32>() as usize, n);
    }
    times.sort();
    println!("places of {n} events: median {:.2?} (21 runs)", times[10]);
}
