//! Benchmarks for `slug::to_file_stem`, which runs once per entity on every import.
#![allow(missing_docs)]

use std::hint::black_box;

use criterion::{Criterion, Throughput, criterion_group, criterion_main};
use ipse_core::slug::to_file_stem;

fn bench_to_file_stem(c: &mut Criterion) {
    let mut group = c.benchmark_group("to_file_stem");

    for (label, input) in [
        ("ascii_short", "Marie Dupont"),
        ("accents", "Élodie Gaëlle de l'Œuvre, Straße 12"),
        (
            "long_mixed",
            "Réunion trimestrielle — équipe produit & données ✨ 2026 Q4 bilan",
        ),
    ] {
        group.throughput(Throughput::Bytes(input.len() as u64));
        group.bench_function(label, |b| b.iter(|| to_file_stem(black_box(input))));
    }

    group.finish();
}

criterion_group!(benches, bench_to_file_stem);
criterion_main!(benches);
