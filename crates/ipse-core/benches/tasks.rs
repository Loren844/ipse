//! Making `extract` tasks on the import path: 20 000 people with notes, the first time
//! (1 000 tasks), then again with nothing new.
#![allow(missing_docs, clippy::unwrap_used, clippy::print_stdout)]

use std::time::Instant;

use ipse_core::tasks::Tasks;
use ipse_core::vault::{Language, Settings, Vault};

const PEOPLE: usize = 20_000;

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
}
