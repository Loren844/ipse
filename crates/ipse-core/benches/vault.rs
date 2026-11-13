//! Benchmarks for reading memory files, which indexing does for every file of the vault.
#![allow(missing_docs, clippy::unwrap_used)]

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
