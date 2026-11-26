//! What does the `unsafe` buy? `Shelf` against `SafeShelf`, same API, same work.
//! Run: cargo run --release --example bench
//!
//! A small hand-rolled timer, so the crate needs no dependencies: many samples per case, cases interleaved, and the
//! median is reported together with the fastest and slowest sample. For decisions that matter, use a harness such as criterion.

use shelf::safe::SafeShelf;
use shelf::Shelf;
use std::hint::black_box;
use std::mem::size_of;
use std::time::Instant;

const N: usize = 1024;
const SAMPLES: usize = 31;
const REPEATS: usize = 2_000;

fn ns_per_element(mut work: impl FnMut() -> u64) -> f64 {
    let start = Instant::now();
    for _ in 0..REPEATS {
        black_box(work());
    }
    start.elapsed().as_nanos() as f64 / (REPEATS * N) as f64
}

macro_rules! fill_and_sum {
    ($ty:ty) => {
        || {
            let shelf = <$ty>::new();
            for i in 0..N as u64 {
                let _ = shelf.push(black_box(i));
            }
            let shelf = black_box(&shelf);
            (0..N).filter_map(|i| shelf.get(i)).sum()
        }
    };
}

fn main() {
    println!("size in bytes, N = {N}      unsafe   safe");
    println!("  u64    (no spare value)  {:>6} {:>6}", size_of::<Shelf<u64, N>>(), size_of::<SafeShelf<u64, N>>());
    println!(
        "  String (has a niche)     {:>6} {:>6}",
        size_of::<Shelf<String, N>>(),
        size_of::<SafeShelf<String, N>>()
    );

    let filled = Shelf::<u64, N>::new();
    let safe_filled = SafeShelf::<u64, N>::new();
    for i in 0..N as u64 {
        let _ = filled.push(i);
        let _ = safe_filled.push(i);
    }

    let names = ["fill+sum  unsafe", "fill+sum  safe  ", "sum only  unsafe", "sum only  safe  "];
    let mut samples = [const { Vec::new() }; 4];
    for _ in 0..SAMPLES {
        samples[0].push(ns_per_element(fill_and_sum!(Shelf<u64, N>)));
        samples[1].push(ns_per_element(fill_and_sum!(SafeShelf<u64, N>)));
        samples[2].push(ns_per_element(|| (0..N).filter_map(|i| black_box(&filled).get(i)).sum()));
        samples[3].push(ns_per_element(|| (0..N).filter_map(|i| black_box(&safe_filled).get(i)).sum()));
    }
    println!("ns per element, {SAMPLES} samples of {REPEATS} x {N}:");
    for (name, s) in names.iter().zip(&mut samples) {
        s.sort_by(f64::total_cmp);
        println!("  {name}  median {:.3}  min {:.3}  max {:.3}", s[SAMPLES / 2], s[0], s[SAMPLES - 1]);
    }
}
