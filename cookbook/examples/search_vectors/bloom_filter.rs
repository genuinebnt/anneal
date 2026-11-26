//! RECIPE 24 (search): Bloom filter: "definitely not present" vs "probably present".
//! LEVEL: intermediate
//! Use in: LSM trees, dedup of events/messages, cache guards.   Run: cargo run --release --example bloom_filter
//! COMMON USES: LSM-tree SSTables (skip files that cannot contain a key), dedup caches, cache-penetration guards.
//!
//! Pattern: k hash functions set k bits per item. A lookup that finds any bit unset is a certain NO; all bits
//! set means MAYBE. No false negatives; false-positive rate is tuned by bits-per-item and k. We derive k
//! hashes from two via double hashing.
//! Read: https://en.wikipedia.org/wiki/Bloom_filter
#![allow(dead_code)]
use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};

struct Bloom {
    bits: Vec<u64>,
    m: usize,
    k: u32,
}

fn h<T: Hash>(x: &T, salt: u64) -> u64 {
    let mut s = DefaultHasher::new(); // fixed keys: deterministic across runs
    salt.hash(&mut s);
    x.hash(&mut s);
    s.finish()
}

impl Bloom {
    fn new(m: usize, k: u32) -> Self {
        Bloom {
            bits: vec![0; m.div_ceil(64)],
            m,
            k,
        }
    }
    fn idx<T: Hash>(&self, x: &T, i: u32) -> usize {
        (h(x, 1).wrapping_add((i as u64).wrapping_mul(h(x, 2) | 1)) % self.m as u64) as usize
    }
    fn insert<T: Hash>(&mut self, x: &T) {
        for i in 0..self.k {
            let b = self.idx(x, i);
            self.bits[b / 64] |= 1 << (b % 64);
        }
    }
    fn maybe_contains<T: Hash>(&self, x: &T) -> bool {
        (0..self.k).all(|i| {
            let b = self.idx(x, i);
            self.bits[b / 64] & (1 << (b % 64)) != 0
        })
    }
}

fn main() {
    let n = 1_000;
    let mut bloom = Bloom::new(n * 10, 7); // 10 bits per item, 7 hashes
    for i in 0..n {
        bloom.insert(&format!("task-{i}"));
    }
    assert!(
        (0..n).all(|i| bloom.maybe_contains(&format!("task-{i}"))),
        "false negative!"
    );
    let trials = 10_000;
    let fp = (0..trials)
        .filter(|i| bloom.maybe_contains(&format!("other-{i}")))
        .count();
    let rate = fp as f64 / trials as f64;
    println!(
        "false positives: {fp}/{trials} = {:.2}% (theory ~0.8%)",
        rate * 100.0
    );
    assert!(rate < 0.03);
}
