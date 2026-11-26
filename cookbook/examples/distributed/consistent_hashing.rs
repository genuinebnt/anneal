//! RECIPE 25 (distributed): consistent hashing ring with virtual nodes.
//! LEVEL: intermediate
//! Use in: partitioning work/data across workers (distributed roadmap).   Run: cargo run --release --example consistent_hashing
//! COMMON USES: sharding keys across caches/DB nodes (Cassandra, memcached clients) so adding a node moves ~1/N of keys.
//!
//! Pattern: hash nodes (many virtual points each) and keys onto the same ring; a key belongs to the first
//! node clockwise. Adding a node only steals the keys in its arcs (about 1/N of them), unlike `hash % N`,
//! which reshuffles almost everything.
//! Read: https://en.wikipedia.org/wiki/Consistent_hashing
#![allow(dead_code)]
use std::collections::BTreeMap;
use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};

fn h<T: Hash>(x: &T) -> u64 {
    let mut s = DefaultHasher::new();
    x.hash(&mut s);
    s.finish()
}

struct Ring {
    points: BTreeMap<u64, String>,
    vnodes: u32,
}

impl Ring {
    fn new(vnodes: u32) -> Self {
        Ring {
            points: BTreeMap::new(),
            vnodes,
        }
    }
    fn add(&mut self, node: &str) {
        for v in 0..self.vnodes {
            self.points.insert(h(&(node, v)), node.to_string());
        }
    }
    fn owner(&self, key: &str) -> &str {
        let hv = h(&key);
        self.points
            .range(hv..)
            .next()
            .or_else(|| self.points.iter().next()) // wrap around the ring
            .map(|(_, n)| n.as_str())
            .unwrap()
    }
}

fn main() {
    let mut ring = Ring::new(100);
    for n in ["worker-a", "worker-b", "worker-c"] {
        ring.add(n);
    }
    let keys: Vec<String> = (0..10_000).map(|i| format!("task-{i}")).collect();
    let before: Vec<String> = keys.iter().map(|k| ring.owner(k).to_string()).collect();
    for n in ["worker-a", "worker-b", "worker-c"] {
        let c = before.iter().filter(|o| *o == n).count();
        println!("{n}: {c} keys");
    }
    ring.add("worker-d");
    let moved = keys
        .iter()
        .zip(&before)
        .filter(|(k, old)| ring.owner(k) != old.as_str())
        .count();
    println!(
        "after adding worker-d, {moved}/10000 keys moved ({:.1}%, ideal 25%)",
        moved as f64 / 100.0
    );
    assert!(moved < 4_000, "naive `hash % N` would move ~75%");
}
