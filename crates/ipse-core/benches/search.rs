//! Benchmarks for searching the vault: reading it, telling whether it changed, and
//! answering a query. An MCP server does the second on every request.
#![allow(missing_docs, clippy::unwrap_used)]

use std::hint::black_box;
use std::path::Path;

use criterion::{Criterion, Throughput, criterion_group, criterion_main};
use ipse_core::search::SearchIndex;

const PEOPLE: usize = 5000;

/// A typical person file: common keys, a few entity keys, an unknown key, some facts.
fn person(i: usize) -> String {
    format!(
        "---
id: 01jcx8m2p4q6r8t0v2w4y{:05}
type: person
category: people
created: 2026-11-12T19:30:00+01:00
updated: 2026-11-12T19:30:00+01:00
sources:
  - contacts:card-{i}
name: Personne Numéro {i}
aliases:
  - Surnom {i}
emails:
  - personne{i}@example.org
phones:
  - \"06{:08}\"
birthday: 1990-04-{:02}
favourite_colour: green
---

## Faits

- Travaille à la boulangerie du quartier. ^f-a{:03}
- Préfère être appelée le soir.

## Relations

- collègue : [[personne-{}]]
",
        i % 100_000,
        i,
        i % 28 + 1,
        i % 1000,
        i + 1
    )
}

/// A vault folder with `PEOPLE` person files.
fn vault() -> tempfile::TempDir {
    let dir = tempfile::tempdir().unwrap();
    let people = dir.path().join("people");
    std::fs::create_dir(&people).unwrap();
    for i in 0..PEOPLE {
        std::fs::write(people.join(format!("personne-{i}.md")), person(i)).unwrap();
    }
    dir
}

fn all(_: &ipse_core::vault::Category) -> bool {
    true
}

fn bench_search(c: &mut Criterion) {
    let dir = vault();
    let root: &Path = dir.path();
    let index = SearchIndex::load(root).unwrap();
    assert_eq!(index.len(), PEOPLE);

    let mut group = c.benchmark_group("search");
    group.sample_size(20);
    group.throughput(Throughput::Elements(PEOPLE as u64));
    group.bench_function("load_5000_people", |b| {
        b.iter(|| SearchIndex::load(black_box(root)).unwrap());
    });
    group.bench_function("unchanged_5000_people", |b| {
        b.iter(|| assert!(!index.changed(black_box(root)).unwrap()));
    });
    group.sample_size(100);
    group.bench_function("query_5000_people", |b| {
        b.iter(|| index.search(black_box("surnom 4242"), 10, all));
    });
    group.finish();
}

criterion_group!(benches, bench_search);
criterion_main!(benches);
