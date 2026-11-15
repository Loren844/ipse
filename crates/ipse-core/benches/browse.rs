//! Browsing a memory of 20 000 people as the interface does: the first page of the
//! list, a search as one types (each prefix of a name), and one person in full.
#![allow(missing_docs, clippy::unwrap_used, clippy::print_stdout)]

use std::time::{Duration, Instant};

use ipse_core::rights::Gateway;
use ipse_core::vault::{History, Language, Settings, Vault};

const PEOPLE: usize = 20_000;
const FIRST: [&str; 8] = ["Marie", "Paul", "Zoé", "Hugo", "Léa", "Nina", "Tom", "Inès"];
const LAST: [&str; 8] = [
    "Durand", "Martin", "Petit", "Roux", "Faure", "Blanc", "Garnier", "Morel",
];

fn median(mut times: Vec<Duration>) -> Duration {
    times.sort();
    times[times.len() / 2]
}

fn main() {
    let dir = tempfile::tempdir().unwrap();
    Vault::init(
        dir.path(),
        "Jules Moreau",
        Settings::new(Language::Fr, "Europe/Paris").unwrap(),
    )
    .unwrap();
    for i in 0..PEOPLE {
        let name = format!("{} {} {i}", FIRST[i % 8], LAST[(i / 8) % 8]);
        std::fs::write(
            dir.path().join(format!("people/p-{i}.md")),
            format!(
                "---\nid: 01jcx8m2p4q6r8t0v2w4{i:06}\ntype: person\ncategory: people\ncreated: 2026-01-10T09:00:00+01:00\nupdated: 2026-01-10T09:00:00+01:00\nname: {name}\nphones:\n  - \"+336{i:08}\"\n---\n\n## Faits\n\n- Aime le jazz ^f-ab12\n\n## Relations\n\n- ami : [[p-{}]]\n\n## Notes\n\nRencontré au club en 2019.\n",
                (i + 1) % PEOPLE
            ),
        )
        .unwrap();
    }
    History::open(dir.path())
        .unwrap()
        .record_outside_edits()
        .unwrap();
    let mut gateway = Gateway::open(dir.path()).unwrap();

    let owner = gateway.fresh_owner().unwrap();
    let mut list = Vec::new();
    for _ in 0..21 {
        let start = Instant::now();
        let page = owner.people(None, 100, 0);
        list.push(start.elapsed());
        assert_eq!(page.total, PEOPLE + 1);
    }
    let mut typed = Vec::new();
    for _ in 0..5 {
        for query in [
            "m",
            "ma",
            "mar",
            "mari",
            "marie",
            "marie d",
            "marie du",
            "marie dur",
        ] {
            let start = Instant::now();
            let found = owner.people(Some(query), 100, 0);
            typed.push(start.elapsed());
            assert!(!found.items.is_empty(), "{query}");
        }
    }
    let mut one = Vec::new();
    for i in 0..21 {
        let start = Instant::now();
        let person = owner
            .details(&format!("01jcx8m2p4q6r8t0v2w4{:06}", i * 900))
            .unwrap();
        one.push(start.elapsed());
        assert_eq!(person.relations.len(), 1);
    }
    println!("{PEOPLE} people, median:");
    println!("first page of the list    {:>9.2?}", median(list));
    println!(
        "search as one types       {:>9.2?}  (worst {:.2?})",
        median(typed.clone()),
        typed.iter().max().unwrap()
    );
    println!("one person, links found   {:>9.2?}", median(one));
}
