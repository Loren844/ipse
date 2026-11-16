//! Making `extract` tasks on the import path: 20 000 people with notes, the first time
//! (1 000 tasks), then again with nothing new. Then planning `summary` tasks, as each
//! `tasks` call of an AI does: 5 000 events over two years, the first time, then with
//! nothing new.
#![allow(missing_docs, clippy::unwrap_used, clippy::print_stdout)]

use std::time::Instant;

use ipse_core::search::SearchIndex;
use ipse_core::tasks::Tasks;
use ipse_core::vault::{Language, Settings, Vault};
use jiff::ToSpan;
use jiff::civil::date;

const PEOPLE: usize = 20_000;
const EVENTS: usize = 5_000;

fn main() {
    let dir = tempfile::tempdir().unwrap();
    Vault::init(
        dir.path(),
        "Jules Moreau",
        Settings::new(Language::Fr, "Europe/Paris").unwrap(),
    )
    .unwrap();
    let mut paths = Vec::with_capacity(PEOPLE);
    for i in 0..PEOPLE {
        let path = dir.path().join(format!("people/personne-{i}.md"));
        std::fs::write(
            &path,
            format!(
                "---\nid: 01jcx8m2p4q6r8t0v2w4{i:06}\ntype: person\ncategory: people\ncreated: 2026-01-10T09:00:00+01:00\nupdated: 2026-01-10T09:00:00+01:00\nname: Personne {i}\nphones:\n  - \"+336{i:08}\"\n---\n\n## Faits\n\n- Aime le jazz ^f-ab12\n\n## Notes\n\nRencontrée au club d'escalade en 2019, travaille à l'hôpital. Deux enfants, Léo et Zoé. Numéro {i}.\n"
            ),
        )
        .unwrap();
        paths.push(path);
    }
    let vault = Vault::open(dir.path()).unwrap();
    let tasks = Tasks::new(&vault);
    for label in ["first import", "same notes again"] {
        let start = Instant::now();
        let made = tasks.extract_from(&paths).unwrap();
        println!(
            "{label:<18} {PEOPLE} files  {made:>5} tasks  {:>9.2?}",
            start.elapsed()
        );
    }

    // Events every few hours over two years, linking to people.
    let first = date(2025, 1, 1);
    for i in 0..EVENTS {
        let day = first
            .checked_add(i64::try_from((i * 3) / 20).unwrap().days())
            .unwrap();
        let path = dir
            .path()
            .join(format!("events/{}/{day}-evenement-{i}.md", day.year()));
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(
            &path,
            format!(
                "---\nid: 01jcx8m2p4q6r8t0v2w5{i:06}\ntype: event\ncategory: agenda\ncreated: 2026-01-10T09:00:00+01:00\nupdated: 2026-01-10T09:00:00+01:00\ntitle: Événement {i}\nstart: {day}T{:02}:00:00+01:00\nparticipants:\n  - \"[[personne-{i}]]\"\n---\n",
                8 + i % 12
            ),
        )
        .unwrap();
    }
    let index = SearchIndex::load(dir.path()).unwrap();
    let today = date(2026, 9, 1);
    for label in ["summaries, first", "summaries, again"] {
        let start = Instant::now();
        let made = tasks.plan_summaries(&index, today).unwrap();
        println!(
            "{label:<18} {EVENTS} events  {made:>5} tasks  {:>9.2?}",
            start.elapsed()
        );
    }
}
