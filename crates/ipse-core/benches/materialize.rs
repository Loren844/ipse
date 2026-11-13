//! Benchmark of an import's last step: 5 000 contacts and 10 000 events from the
//! journal into a new vault, each as one change in the history.
#![allow(missing_docs, clippy::unwrap_used)]

use criterion::{BatchSize, Criterion, Throughput, criterion_group, criterion_main};
use ipse_core::journal::Journal;
use ipse_core::materialize::{calendar_to_events, contacts_to_people};
use ipse_core::vault::{Language, Settings, Source, Vault};
use ipse_protocol::{Op, Record};
use serde_json::json;

const PEOPLE: usize = 5_000;
const EVENTS: usize = 10_000;

fn records() -> (Vec<Record>, Vec<Record>) {
    let at = "2026-11-12T18:30:00Z".parse().unwrap();
    let record = |kind: &str, id: String, data| Record {
        kind: kind.into(),
        op: Op::Upsert,
        external_id: Some(id),
        observed_at: at,
        data: Some(data),
    };
    let people = (0..PEOPLE)
        .map(|i| {
            record(
                "contact.card",
                format!("card-{i}"),
                json!({
                    "full_name": format!("Personne Numéro {i}"),
                    "nicknames": [format!("Surnom {i}")],
                    "emails": [format!("personne{i}@example.org")],
                    "phones": [format!("06{i:08}")],
                    "birthday": format!("1990-04-{:02}", i % 28 + 1),
                    "organization": "Boulangerie du Coin",
                }),
            )
        })
        .collect();
    let events = (0..EVENTS)
        .map(|i| {
            let (month, day, hour) = (i % 12 + 1, i % 28 + 1, 8 + i % 10);
            record(
                "calendar.event",
                format!("event-{i}@example.org"),
                json!({
                    "uid": format!("event-{i}@example.org"),
                    "title": format!("Réunion numéro {i}"),
                    "start": format!("2026-{month:02}-{day:02}T{hour:02}:00:00+01:00"),
                    "end": format!("2026-{month:02}-{day:02}T{:02}:00:00+01:00", hour + 1),
                    "location": format!("Salle {}", i % 40),
                    "description": "Ordre du jour : budget, questions diverses.",
                    "organizer": {"name": "Organisatrice", "email": format!("personne{}@example.org", i % PEOPLE)},
                    "attendees": [
                        {"name": "Invité", "email": format!("personne{}@example.org", (i * 7) % PEOPLE)},
                        {"name": "Externe", "email": "externe@example.com"},
                    ],
                }),
            )
        })
        .collect();
    (people, events)
}

fn bench_materialize(c: &mut Criterion) {
    let (people, events) = records();
    let mut group = c.benchmark_group("materialize");
    group.sample_size(10);
    group.throughput(Throughput::Elements((PEOPLE + EVENTS) as u64));
    group.bench_function("5000_people_10000_events", |b| {
        b.iter_batched(
            || {
                let dir = tempfile::tempdir().unwrap();
                let vault = Vault::init(
                    dir.path(),
                    "Camille Durand",
                    Settings::new(Language::Fr, "Europe/Paris").unwrap(),
                )
                .unwrap();
                let mut journal = Journal::new(vault.root());
                journal.append("contacts", &people, None).unwrap();
                journal.append("calendar", &events, None).unwrap();
                (dir, vault, journal)
            },
            |(dir, vault, journal)| {
                let history = vault.history().unwrap();
                let mut change = history.change(Source::Connector("contacts".into()));
                let people = contacts_to_people(&vault, &journal, "contacts", &mut change).unwrap();
                change.commit("Import contacts").unwrap();
                let mut change = history.change(Source::Connector("calendar".into()));
                let now = "2026-11-13T12:00:00Z".parse().unwrap();
                let events =
                    calendar_to_events(&vault, &journal, "calendar", &mut change, now).unwrap();
                change.commit("Import calendar").unwrap();
                assert_eq!((people.added, events.added), (PEOPLE as u64, EVENTS as u64));
                drop(dir);
            },
            BatchSize::PerIteration,
        );
    });
    group.finish();
}

criterion_group!(benches, bench_materialize);
criterion_main!(benches);
