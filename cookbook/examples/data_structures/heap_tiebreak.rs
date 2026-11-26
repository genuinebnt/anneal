//! RECIPE 05 (data-structures): BinaryHeap as a min-heap with a deterministic tie-break.
//! LEVEL: simple
//! Use in: stage 7 (and later priority scheduling).   Run: cargo run --example heap_tiebreak
//! COMMON USES: schedulers and top-k trackers: BinaryHeap + Reverse + a sequence number gives deterministic priority queues.
//!
//! Pattern: `BinaryHeap` is a MAX-heap; wrap keys in `Reverse` for min-first. Tuples compare field by
//! field, so `(priority, sequence, name)` gives "lowest priority number first, then first-come-first-served".
//! Never rely on `HashMap`/`HashSet` iteration order for output: it differs between runs.
//! Real code: std BinaryHeap -> https://github.com/rust-lang/rust/blob/master/library/alloc/src/collections/binary_heap/mod.rs
#![allow(dead_code)]
use std::cmp::Reverse;
use std::collections::BinaryHeap;

fn main() {
    let jobs = [
        ("report", 2u8),
        ("invoice", 1),
        ("photo", 2),
        ("memo", 1),
        ("poster", 3),
    ];
    let mut heap = BinaryHeap::new();
    for (seq, (name, priority)) in jobs.iter().enumerate() {
        heap.push(Reverse((*priority, seq as u64, name.to_string())));
    }
    while let Some(Reverse((priority, seq, name))) = heap.pop() {
        println!("priority {priority} (arrived #{seq}): {name}");
    }
}
