//! Looking for possible duplicates on the import path: 20 000 people, 200 of whom
//! share an email or a phone with another, through maps, never every pair.
#![allow(missing_docs, clippy::unwrap_used, clippy::print_stdout)]

use std::time::Instant;

use ipse_core::duplicates;
use ipse_core::vault::{Language, Settings, Vault};

const PEOPLE: usize = 20_000;
/// One person in this many shares a phone with the previous one.
const EVERY: usize = 100;

fn main() {
    let dir = tempfile::tempdir().unwrap();
    Vault::init(
        dir.path(),
        "Jules Moreau",
        Settings::new(Language::Fr, "Europe/Paris").unwrap(),
    )
    .unwrap();
    for i in 0..PEOPLE {
        let phone = if i % EVERY == 1 { i - 1 } else { i };
        std::fs::write(
            dir.path().join(format!("people/personne-{i}.md")),
            format!(
                "---\nid: 01jcx8m2p4q6r8t0v2w4{i:06}\ntype: person\ncategory: people\ncreated: 2026-01-10T09:00:00+01:00\nupdated: 2026-01-10T09:00:00+01:00\nname: Personne {i}\nemails:\n  - personne{i}@example.org\nphones:\n  - \"+336{phone:08}\"\n---\n\n## Faits\n\n- Aime le jazz ^f-ab12\n"
            ),
        )
        .unwrap();
    }
    let start = Instant::now();
    let found = duplicates::people(dir.path()).unwrap();
    println!(
        "detect      {PEOPLE} people  {:>4} pairs  {:>9.2?}",
        found.len(),
        start.elapsed()
    );
    assert_eq!(found.len(), PEOPLE / EVERY);
    let vault = Vault::open(dir.path()).unwrap();
    let start = Instant::now();
    let made = duplicates::propose(&vault).unwrap();
    println!(
        "propose     {PEOPLE} people  {made:>4} new    {:>9.2?}",
        start.elapsed()
    );
    let start = Instant::now();
    let again = duplicates::propose(&vault).unwrap();
    println!(
        "again       {PEOPLE} people  {again:>4} new    {:>9.2?}",
        start.elapsed()
    );
}
