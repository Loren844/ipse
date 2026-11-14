//! Benchmarks for reading memory files, which indexing does for every file of the vault.
#![allow(missing_docs, clippy::unwrap_used)]

use std::fmt::Write as _;
use std::hint::black_box;

use criterion::{Criterion, Throughput, criterion_group, criterion_main};
use ipse_core::vault::MemoryFile;

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

fn bench_parse(c: &mut Criterion) {
    let files: Vec<String> = (0..10_000).map(person).collect();
    let mut group = c.benchmark_group("memory_file");

    group.throughput(Throughput::Elements(1));
    group.bench_function("parse_one_person", |b| {
        b.iter(|| MemoryFile::parse("p.md", black_box(&files[0])).unwrap());
    });

    group.throughput(Throughput::Elements(files.len() as u64));
    group.sample_size(20);
    group.bench_function("parse_10000_people", |b| {
        b.iter(|| {
            for text in &files {
                black_box(MemoryFile::parse("p.md", black_box(text)).unwrap());
            }
        });
    });

    // A long body: 50 facts, half without an id, and 20 relations.
    let mut long = String::from("\n## Faits\n\n");
    for i in 0..50 {
        let id = if i % 2 == 0 {
            format!(" ^f-{i:04}")
        } else {
            String::new()
        };
        let _ = writeln!(long, "- Fait numéro {i}, avec quelques mots de plus.{id}");
    }
    long.push_str("\n## Relations\n\n");
    for i in 0..20 {
        let word = ["mère", "ami", "travaille chez", "parrain"][i % 4];
        let _ = writeln!(long, "- {word} : [[personne-{i}|Personne {i}]]");
    }
    long.push_str("\n## Notes\n\nQuelques notes.\n");
    group.throughput(Throughput::Elements(1));
    group.bench_function("body_50_facts_20_relations", |b| {
        b.iter(|| ipse_core::vault::body::parse(black_box(&long)));
    });
    group.bench_function("relation_kinds_20", |b| {
        let body = ipse_core::vault::body::parse(&long);
        b.iter(|| {
            black_box(&body)
                .relations
                .iter()
                .filter(|r| r.kind().is_some())
                .count()
        });
    });
    group.bench_function("fact_ids_25_missing", |b| {
        b.iter(|| ipse_core::vault::body::with_fact_ids(black_box(&long)).unwrap());
    });

    let file = MemoryFile::parse("p.md", &files[0]).unwrap();
    group.throughput(Throughput::Elements(1));
    group.sample_size(100);
    group.bench_function("render_unchanged", |b| {
        b.iter(|| black_box(&file).render().unwrap());
    });

    group.finish();
}

criterion_group!(benches, bench_parse);
criterion_main!(benches);
