//! Criterion benchmark structure, companion to recipe 1117 (examples/criterion_proptest_insta_testing.rs).
//! Run: cargo bench -p cb-crate-tour --bench crate_tour_bench -- --quick
//! Numbers are machine dependent and nothing here asserts on timing.
#![allow(dead_code)]
#[path = "../examples/criterion_proptest_insta_testing.rs"]
mod subject;

use criterion::{BatchSize, BenchmarkId, Criterion, Throughput, criterion_group, criterion_main};
use std::hint::black_box;

fn rle(c: &mut Criterion) {
    let mut group = c.benchmark_group("rle_encode");
    for len in [1_000usize, 100_000] {
        // The input SHAPE matters as much as its size: benchmark the best and worst case separately.
        for (shape, input) in [
            ("one_run", "a".repeat(len)),
            ("alternating", "ab".repeat(len / 2)),
        ] {
            group.throughput(Throughput::Bytes(len as u64)); // report bytes per second, comparable across sizes
            group.bench_with_input(BenchmarkId::new(shape, len), &input, |b, s| {
                b.iter(|| subject::rle_encode(black_box(s))) // black_box: stop the optimiser from precomputing
            });
        }
    }
    group.finish();
}

fn ring(c: &mut Criterion) {
    // iter_batched: the setup closure is NOT measured. Use it when the routine consumes or mutates its input.
    c.bench_function("ring_drain_1024", |b| {
        b.iter_batched(
            || {
                let mut r = subject::Ring::new(1024);
                (0..2048).for_each(|i| r.push(i));
                r
            },
            |mut r| {
                while let Some(v) = r.pop() {
                    black_box(v);
                }
                r // returned so that dropping it is not measured
            },
            BatchSize::SmallInput,
        )
    });
}

criterion_group!(benches, rle, ring);
criterion_main!(benches);
