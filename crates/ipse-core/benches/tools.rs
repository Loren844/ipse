//! Each MCP tool's read, through the query layer, on a vault of 20 000 files: 10 000
//! people and 10 000 events, plus a year of music. Prints p50 and p99 of 200 calls.
//!
//! Each call includes what `ipse mcp` does before every tool: reloading the connected
//! AIs, checking the access code and checking whether the vault changed.
#![allow(
    missing_docs,
    clippy::unwrap_used,
    clippy::print_stdout,
    clippy::cast_precision_loss
)]

use std::fmt::Write as _;
use std::path::Path;
use std::time::{Duration, Instant};

use ipse_core::rights::{Access, Clients, Gateway};
use ipse_core::vault::{DateTime, EventTime, Language, Settings, Vault};

const PEOPLE: usize = 10_000;
const EVENTS: usize = 10_000;
const CALLS: usize = 200;

fn id(prefix: char, i: usize) -> String {
    format!("01{prefix}cx8m2p4q6r8t0v2w4{i:06}")
}

fn write_vault(root: &Path) -> String {
    let tz = jiff::tz::TimeZone::get("Europe/Paris").unwrap();
    Vault::init(
        root,
        "Camille Durand",
        Settings::new(Language::Fr, "Europe/Paris").unwrap(),
    )
    .unwrap();
    let people = root.join("people");
    for i in 0..PEOPLE {
        let text = format!(
            "---\nid: {}\ntype: person\ncategory: people\ncreated: 2026-11-12T19:30:00+01:00\nupdated: 2026-11-12T19:30:00+01:00\nname: Personne Numéro {i}\naliases:\n  - Surnom {i}\nemails:\n  - personne{i}@example.org\n---\n\n## Faits\n\n- Travaille à la boulangerie du quartier.\n- Voisine de [[personne-numero-{}]].\n",
            id('p', i),
            (i + 1) % PEOPLE
        );
        std::fs::write(people.join(format!("personne-numero-{i}.md")), text).unwrap();
    }
    let today = jiff::Timestamp::now().to_zoned(tz).date();
    for i in 0..EVENTS {
        let day = today
            .checked_add(jiff::ToSpan::days(i64::try_from(i % 400).unwrap() - 300))
            .unwrap();
        let dir = root.join("events").join(day.year().to_string());
        std::fs::create_dir_all(&dir).unwrap();
        let text = format!(
            "---\nid: {}\ntype: event\ncategory: agenda\ncreated: 2026-11-12T19:30:00+01:00\nupdated: 2026-11-12T19:30:00+01:00\ntitle: Réunion numéro {i}\nstart: {day}T{:02}:00:00+01:00\nend: {day}T{:02}:00:00+01:00\nplace: Salle {}\nparticipants: [\"[[personne-numero-{}]]\", \"[[personne-numero-{}]]\", Invité externe]\n---\n\n## Notes\n\nOrdre du jour : budget.\n",
            id('e', i),
            8 + i % 10,
            9 + i % 10,
            i % 40,
            i % PEOPLE,
            (i * 7) % PEOPLE
        );
        std::fs::write(dir.join(format!("{day}-reunion-numero-{i}.md")), text).unwrap();
    }
    let mut rows = String::new();
    for i in 0..300 {
        let last = today
            .checked_add(jiff::ToSpan::days(-i64::from(i)))
            .unwrap();
        let _ = writeln!(
            rows,
            "| Artiste {i} | {last} | {last} | {} | device |",
            i + 1
        );
    }
    let year = today.year();
    let music = root.join("states/music");
    std::fs::create_dir_all(&music).unwrap();
    std::fs::write(
        music.join(format!("{year}.md")),
        format!(
            "---\nid: {}\ntype: state-log\ncategory: media\ncreated: 2026-01-01T00:00:00+01:00\nupdated: 2026-11-12T00:00:00+01:00\ntopic: music\nyear: {year}\n---\n\n| Subject | First seen | Last seen | Count | Source |\n| --- | --- | --- | --- | --- |\n{rows}",
            id('s', 0)
        ),
    )
    .unwrap();
    let mut clients = Clients::load(root).unwrap();
    let (_, token) = clients
        .add("Claude", DateTime::now(&jiff::tz::TimeZone::UTC))
        .unwrap();
    token.expose().to_owned()
}

fn measure(gateway: &mut Gateway, token: &str, name: &str, call: impl Fn(&Access<'_>) -> usize) {
    let mut times: Vec<Duration> = Vec::with_capacity(CALLS);
    let mut results = 0;
    for _ in 0..CALLS {
        let start = Instant::now();
        let access = gateway.fresh_access(Some(token)).unwrap();
        results = call(&access);
        times.push(start.elapsed());
    }
    times.sort();
    let p = |percent: usize| times[(times.len() - 1) * percent / 100];
    println!(
        "{name:<32} p50 {:>9.3?}  p99 {:>9.3?}  ({results} results)",
        p(50),
        p(99)
    );
}

fn main() {
    // `cargo bench` passes `--bench`; `cargo test --benches` passes nothing else useful.
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    let token = write_vault(root);
    let mut gateway = Gateway::open(root).unwrap();
    let today = jiff::Timestamp::now()
        .to_zoned(jiff::tz::TimeZone::get("Europe/Paris").unwrap())
        .date();
    let day = |n: i64| EventTime::Day(today.checked_add(jiff::ToSpan::days(n)).unwrap());
    let person = id('p', 42);
    let event = id('e', 42);

    println!("{} files, {CALLS} calls each", PEOPLE + EVENTS + 2);
    let index = ipse_core::search::SearchIndex::load(root).unwrap();
    let mut full: Vec<Duration> = (0..20)
        .map(|_| {
            let start = Instant::now();
            assert!(!index.changed(root).unwrap());
            start.elapsed()
        })
        .collect();
    full.sort();
    println!(
        "{:<32} p50 {:>9.3?}  (at most every 2 s)",
        "full check of every file",
        full[full.len() / 2]
    );
    measure(&mut gateway, &token, "check only (before each tool)", |_| 0);
    measure(&mut gateway, &token, "about_me", |a| {
        usize::from(a.about_me().is_some())
    });
    measure(&mut gateway, &token, "search \"personne 4242\"", |a| {
        a.search("personne 4242", 10).hits.len()
    });
    measure(&mut gateway, &token, "search \"boulangerie\" (10)", |a| {
        a.search("boulangerie", 10).hits.len()
    });
    measure(&mut gateway, &token, "get by id (person)", |a| {
        usize::from(a.get(&person).is_some())
    });
    measure(&mut gateway, &token, "get by id (event)", |a| {
        usize::from(a.get(&event).is_some())
    });
    measure(&mut gateway, &token, "get by name", |a| {
        a.get_by_name("Personne Numéro 42").len()
    });
    measure(&mut gateway, &token, "events tomorrow", |a| {
        a.events(&day(1), &day(1), None, 200).events.len()
    });
    measure(&mut gateway, &token, "events of a year (200 max)", |a| {
        a.events(&day(-365), &day(0), None, 200).events.len()
    });
    measure(&mut gateway, &token, "events with someone, a year", |a| {
        a.events(&day(-365), &day(90), Some(&person), 200)
            .events
            .len()
    });
    measure(&mut gateway, &token, "now", |a| a.now(&[]).states.len());
    measure(&mut gateway, &token, "states_history, a year", |a| {
        a.states_history("music", &day(-365), &day(0), 200)
            .states
            .len()
    });
}
