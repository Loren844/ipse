//! A proposal touching 100 people files, among 5 000: how long submitting, showing its
//! changes, accepting it (one change of the history) and undoing it take; then the
//! review inbox with 100 waiting proposals.
#![allow(missing_docs, clippy::unwrap_used, clippy::print_stdout)]

use std::time::{Duration, Instant};

use ipse_core::proposals::{Accepted, Author, Change, Proposals, Status};
use ipse_core::vault::{History, Language, Settings, Vault};
use serde_json::json;

const PEOPLE: usize = 5_000;
const TOUCHED: usize = 100;
const ROUNDS: usize = 7;
/// Waiting proposals in the inbox.
const INBOX: usize = 100;

fn person(i: usize) -> String {
    format!(
        "---\nid: 01jcx8m2p4q6r8t0v2w4{i:06}\ntype: person\ncategory: people\ncreated: 2026-01-10T09:00:00+01:00\nupdated: 2026-01-10T09:00:00+01:00\nname: Personne {i}\nphones:\n  - \"+336{i:08}\"\n---\n\n## Faits\n\n- Aime le jazz ^f-ab12\n- Habite à Lyon ^f-cd34\n\n## Notes\n\nRencontrée au club.\n"
    )
}

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
        std::fs::write(
            dir.path().join(format!("people/personne-{i}.md")),
            person(i),
        )
        .unwrap();
    }
    History::open(dir.path())
        .unwrap()
        .record_outside_edits()
        .unwrap();
    let vault = Vault::open(dir.path()).unwrap();
    let proposals = Proposals::new(&vault);

    let (mut submit, mut preview, mut accept, mut undo) = (vec![], vec![], vec![], vec![]);
    for round in 0..ROUNDS {
        let changes: Vec<Change> = (0..TOUCHED)
            .flat_map(|i| {
                let file = format!("people/personne-{}.md", i * (PEOPLE / TOUCHED));
                [
                    Change::AddFact {
                        file: file.clone(),
                        text: format!("Fait numéro {round}."),
                    },
                    Change::SetKey {
                        file,
                        key: "aliases".into(),
                        from: json!([]),
                        to: json!([format!("Surnom {round}")]),
                    },
                ]
            })
            .collect();
        let start = Instant::now();
        let proposal = proposals
            .submit(Author::Client("bench".into()), "Bench.", None, changes)
            .unwrap();
        submit.push(start.elapsed());
        let start = Instant::now();
        let shown = proposals.preview(&proposal).unwrap();
        preview.push(start.elapsed());
        assert_eq!(shown.files.len(), TOUCHED);
        let start = Instant::now();
        assert!(matches!(
            proposals.accept(&proposal.id).unwrap(),
            Accepted::Applied(Some(_))
        ));
        accept.push(start.elapsed());
        let start = Instant::now();
        proposals.undo(&proposal.id).unwrap();
        undo.push(start.elapsed());
    }
    let described = described(&proposals);
    let inbox = inbox(&proposals);

    println!("{TOUCHED} files touched (200 changes) among {PEOPLE}, median of {ROUNDS}:");
    println!("submit   {:>9.2?}", median(submit));
    println!("preview  {:>9.2?}", median(preview));
    println!(
        "accept   {:>9.2?}  (one change of the history)",
        median(accept)
    );
    println!("undo     {:>9.2?}", median(undo));
    println!(
        "described {:>8.2?}  (a decided one, as written)",
        median(described)
    );
    println!(
        "inbox    {:>9.2?}  ({INBOX} waiting, each checked against its file)",
        median(inbox)
    );
}

/// Showing again the last decided proposal, as it was written.
fn described(proposals: &Proposals<'_>) -> Vec<Duration> {
    let decided = proposals.list().unwrap().remove(0);
    (0..ROUNDS)
        .map(|_| {
            let start = Instant::now();
            assert_eq!(proposals.described(&decided).files.len(), TOUCHED);
            start.elapsed()
        })
        .collect()
}

/// The inbox: every waiting proposal checked against the files, as `ipse ui` lists them.
fn inbox(proposals: &Proposals<'_>) -> Vec<Duration> {
    for i in 0..INBOX {
        proposals
            .submit(
                Author::Client("bench".into()),
                "Bench.",
                None,
                vec![Change::AddFact {
                    file: format!("people/personne-{}.md", i * (PEOPLE / INBOX)),
                    text: "Un fait de plus.".into(),
                }],
            )
            .unwrap();
    }
    (0..ROUNDS)
        .map(|_| {
            let start = Instant::now();
            let applies: usize = proposals
                .list()
                .unwrap()
                .iter()
                .filter(|p| p.status == Status::Waiting)
                .map(|p| usize::from(proposals.preview(p).unwrap().stale.is_none()))
                .sum();
            assert_eq!(applies, INBOX);
            start.elapsed()
        })
        .collect()
}
