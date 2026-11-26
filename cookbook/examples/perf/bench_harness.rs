//! RECIPE 56 (perf): measuring properly: warm-up, many samples, median, `black_box`, release mode.
//! LEVEL: intermediate
//! Use in: stage 8 and every "is this faster?" question.   Run: cargo run --release --example bench_harness
//! COMMON USES: every performance question: warm-up, median, black_box, release mode.
#![allow(dead_code)]
//!
//! Rules: (1) ALWAYS benchmark with --release; (2) run the code several times and report the MEDIAN (not one run);
//! (3) wrap inputs/outputs in `black_box` so the optimiser cannot delete the work; (4) change one thing at a time;
//! (5) measure before optimising. For statistical rigour use criterion https://github.com/bheisler/criterion.rs
//! and divan https://github.com/nvzqz/divan ; to see WHERE time goes use flamegraphs (cargo flamegraph).
//! Read: https://nnethercote.github.io/perf-book/benchmarking.html
use std::collections::HashSet;
use std::hint::black_box;
use std::time::{Duration, Instant};

fn bench<R>(name: &str, mut f: impl FnMut() -> R) {
    for _ in 0..3 {
        black_box(f()); // warm-up: caches, branch predictors, lazy init
    }
    let mut samples: Vec<Duration> = (0..15)
        .map(|_| {
            let t = Instant::now();
            black_box(f());
            t.elapsed()
        })
        .collect();
    samples.sort();
    println!(
        "{name:<28} median {:>10.2?}  min {:>10.2?}  max {:>10.2?}",
        samples[samples.len() / 2],
        samples[0],
        samples[samples.len() - 1]
    );
}

fn main() {
    if cfg!(debug_assertions) {
        println!("WARNING: debug build: numbers are meaningless. Use --release.\n");
    }
    // Question: how should I test membership of 2000 queries in 2000 ids?
    let ids: Vec<u32> = (0..2000).map(|i| i * 7).collect();
    let queries: Vec<u32> = (0..2000).map(|i| i * 3).collect();

    bench("Vec::contains (O(n) each)", || {
        queries.iter().filter(|q| ids.contains(q)).count()
    });

    let set: HashSet<u32> = ids.iter().copied().collect();
    bench("HashSet lookups (O(1) each)", || {
        queries.iter().filter(|q| set.contains(q)).count()
    });

    let mut sorted = ids.clone();
    sorted.sort_unstable();
    bench("sorted + binary_search", || {
        queries
            .iter()
            .filter(|q| sorted.binary_search(q).is_ok())
            .count()
    });

    bench("build HashSet (setup cost)", || {
        ids.iter().copied().collect::<HashSet<u32>>().len()
    });

    // Sanity: all strategies must agree before you compare speed
    let a = queries.iter().filter(|q| ids.contains(q)).count();
    let b = queries.iter().filter(|q| set.contains(q)).count();
    let c = queries
        .iter()
        .filter(|q| sorted.binary_search(q).is_ok())
        .count();
    assert!(a == b && b == c);
    println!(
        "\nall agree: {a} matches. Lesson: the winner depends on n and on whether setup is amortised."
    );
}
