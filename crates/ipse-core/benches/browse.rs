//! Browsing a memory of 20 000 people as the interface does: the first page of the
//! list, a search as one types (each prefix of a name), and one person in full; then
//! the whole memory (issue #128): how many of each kind, one kind listed, and what links
//! to a person; then the graph around a person (issue #130), and around one who takes
//! part in 2 000 events; then the whole memory as one graph (issue #174); then everything
//! around that person, for the view around someone (issue #244).
#![allow(
    missing_docs,
    clippy::unwrap_used,
    clippy::print_stdout,
    clippy::too_many_lines // one memory, measured step by step
)]

use std::time::{Duration, Instant};

use ipse_core::rights::Gateway;
use ipse_core::vault::{History, Language, Settings, Vault};

const PEOPLE: usize = 20_000;
const EVENTS: usize = 2_000;
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
    std::fs::create_dir_all(dir.path().join("events/2026")).unwrap();
    for e in 0..EVENTS {
        std::fs::write(
            dir.path().join(format!("events/2026/e-{e}.md")),
            format!(
                "---\nid: 01jcx8m2p4q6r8t0v2w5{e:06}\ntype: event\ncategory: agenda\ncreated: 2026-01-10T09:00:00+01:00\nupdated: 2026-01-10T09:00:00+01:00\ntitle: Café {e}\nstart: 2026-{:02}-{:02}\nparticipants:\n  - \"[[p-0]]\"\n  - \"[[p-{}]]\"\n---\n",
                e % 12 + 1,
                e % 28 + 1,
                e * 10 + 3
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

    let mut kinds = Vec::new();
    let mut places = Vec::new();
    let mut back = Vec::new();
    for i in 0..21 {
        let start = Instant::now();
        let counted = owner.kinds();
        kinds.push(start.elapsed());
        assert_eq!(counted[0].1, PEOPLE + 1);
        let start = Instant::now();
        let listed = owner.entities(ipse_core::vault::FileType::Place, None, 100, 0);
        places.push(start.elapsed());
        assert_eq!(listed.total, 0);
        let start = Instant::now();
        let links = owner.backlinks(&format!("01jcx8m2p4q6r8t0v2w4{:06}", i * 900 + 1));
        back.push(start.elapsed());
        assert_eq!(links.len(), 1, "the one before links to it");
    }
    println!("every kind counted        {:>9.2?}", median(kinds));
    println!("one kind listed (empty)   {:>9.2?}", median(places));
    println!("what links to a person    {:>9.2?}", median(back));

    let mut around = Vec::new();
    let mut hub = Vec::new();
    for i in 0..21 {
        let start = Instant::now();
        let graph = owner
            .graph(
                &format!("01jcx8m2p4q6r8t0v2w4{:06}", i * 900 + 5),
                2,
                &[],
                None,
                300,
            )
            .unwrap();
        around.push(start.elapsed());
        assert!(graph.nodes.len() >= 5, "two friends each side");
        let start = Instant::now();
        let graph = owner
            .graph("01jcx8m2p4q6r8t0v2w4000000", 2, &[], None, 300)
            .unwrap();
        hub.push(start.elapsed());
        assert_eq!(graph.nodes.len(), 300);
        assert!(graph.more > 0);
    }
    println!(
        "graph around a person     {:>9.2?}  (2 steps)",
        median(around)
    );
    println!(
        "graph, 2 000 events       {:>9.2?}  (2 steps, 300 files)",
        median(hub)
    );

    let mut mine = Vec::new();
    let mut one = Vec::new();
    let always = (jiff::Timestamp::MIN, jiff::Timestamp::MAX);
    for _ in 0..11 {
        let start = Instant::now();
        let graph = owner.around(None, always).unwrap();
        mine.push(start.elapsed());
        assert!(graph.mine);
        assert!(!graph.neighbours.is_empty());
        let start = Instant::now();
        let graph = owner
            .around(Some("01jcx8m2p4q6r8t0v2w4000000"), always)
            .unwrap();
        one.push(start.elapsed());
        assert!(!graph.neighbours.is_empty());
    }
    println!(
        "graph around the owner    {:>9.2?}  ({} files, 2 000 events)",
        median(mine),
        PEOPLE + 1 + EVENTS
    );
    println!("graph around a person     {:>9.2?}  (1 step)", median(one));

    let mut busy = Vec::new();
    let mut quiet = Vec::new();
    for _ in 0..11 {
        let start = Instant::now();
        let found = owner.surroundings("01jcx8m2p4q6r8t0v2w4000000").unwrap();
        busy.push(start.elapsed());
        assert_eq!(found.events.len(), EVENTS);
        assert!(!found.people.is_empty());
        let start = Instant::now();
        let found = owner.surroundings("01jcx8m2p4q6r8t0v2w4000013").unwrap();
        quiet.push(start.elapsed());
        assert_eq!(found.events.len(), 1);
    }
    println!(
        "around someone, 2 000 ev. {:>9.2?}  (every section)",
        median(busy)
    );
    println!("around someone, 1 event   {:>9.2?}", median(quiet));
}
