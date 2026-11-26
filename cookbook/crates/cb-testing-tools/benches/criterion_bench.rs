//! Criterion benchmarks. Run: `cargo bench -p cb-testing-tools` (full) or
//! `cargo bench -p cb-testing-tools -- --quick` (fast, less precise). `cargo bench --no-run` only builds.
//! `cargo test --benches` runs each benchmark ONCE as a smoke test (criterion "test mode").
use std::hint::black_box;

use cb_testing_tools::duration::{format_duration, parse_duration};
use cb_testing_tools::expr::{format_expr, parse_expr};
use cb_testing_tools::lru::LruMap;
use criterion::{BenchmarkId, Criterion, Throughput, criterion_group, criterion_main};

/// "1 + 2 * x1 - (3 / y2) + ..." with `terms` terms: input size scales linearly.
fn sample_expr(terms: usize) -> String {
    (0..terms)
        .map(|i| {
            if i % 3 == 0 {
                format!("(x{i} - {i})")
            } else {
                format!("{i} * y{i}")
            }
        })
        .collect::<Vec<_>>()
        .join(" + ")
}

fn bench_parse(c: &mut Criterion) {
    let mut g = c.benchmark_group("expr");
    for terms in [10usize, 100, 1000] {
        let src = sample_expr(terms);
        // Throughput::Elements makes criterion also report terms/second.
        g.throughput(Throughput::Elements(terms as u64));
        g.bench_with_input(BenchmarkId::new("parse", terms), &src, |b, s| {
            b.iter(|| parse_expr(black_box(s)).unwrap())
        });
        let tree = parse_expr(&src).unwrap();
        g.bench_with_input(BenchmarkId::new("format", terms), &tree, |b, t| {
            b.iter(|| format_expr(black_box(t)))
        });
    }
    g.finish();
}

fn bench_lru(c: &mut Criterion) {
    let mut g = c.benchmark_group("lru");
    for cap in [16usize, 256] {
        g.throughput(Throughput::Elements(1000));
        g.bench_function(BenchmarkId::from_parameter(cap), |b| {
            b.iter(|| {
                let mut m = LruMap::new(cap);
                for i in 0..1000u32 {
                    m.put(i % (cap as u32 * 2), i);
                    black_box(m.get(&(i % 7)));
                }
                m.len()
            })
        });
    }
    g.finish();
}

fn bench_duration(c: &mut Criterion) {
    c.bench_function("duration/roundtrip", |b| {
        b.iter(|| format_duration(parse_duration(black_box("3d4h5m6s789ms")).unwrap()))
    });
}

criterion_group!(benches, bench_parse, bench_lru, bench_duration);
criterion_main!(benches);
