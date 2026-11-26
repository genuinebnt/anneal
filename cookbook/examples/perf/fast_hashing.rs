//! RECIPE 59 (perf): the default HashMap hasher (SipHash) vs a fast non-cryptographic one, and when NOT to swap it.
//! LEVEL: intermediate
//! Use in: maps keyed by small ints/ids in hot loops (graph algorithms, interning).   Run: cargo run --release --example fast_hashing
//! COMMON USES: graph algorithms, interning tables, compilers (rustc uses Fx hashing); not for attacker-controlled keys.
#![allow(dead_code)]
//!
//! std's default hasher (SipHash 1-3, randomly keyed per map) is deliberately DoS-resistant but not the fastest.
//! For trusted keys, a simple multiply-rotate hasher (the "Fx" family used inside rustc) is much faster.
//! NEVER use it for attacker-controlled keys (hash flooding). Production crates: rustc-hash, ahash, fxhash.
//! Often the best fix is no hashing at all: dense `usize` ids into a Vec (see index_graph).
//! Read: https://nnethercote.github.io/perf-book/hashing.html and https://github.com/rust-lang/rustc-hash
use std::collections::HashMap;
use std::hash::{BuildHasherDefault, Hasher};
use std::hint::black_box;
use std::time::Instant;

#[derive(Default)]
struct Fx(u64);
const K: u64 = 0x517c_c1b7_2722_0a95;
impl Fx {
    #[inline]
    fn add(&mut self, w: u64) {
        self.0 = (self.0.rotate_left(5) ^ w).wrapping_mul(K);
    }
}
impl Hasher for Fx {
    fn write(&mut self, bytes: &[u8]) {
        for &b in bytes {
            self.add(b as u64);
        }
    }
    fn write_u32(&mut self, i: u32) {
        self.add(i as u64);
    }
    fn write_u64(&mut self, i: u64) {
        self.add(i);
    }
    fn write_usize(&mut self, i: usize) {
        self.add(i as u64);
    }
    fn finish(&self) -> u64 {
        self.0
    }
}
type FxMap<K, V> = HashMap<K, V, BuildHasherDefault<Fx>>;

fn workload<M: Extend<(u32, u32)> + Default>(n: u32) -> M {
    let mut m = M::default();
    m.extend((0..n).map(|i| (i.wrapping_mul(2_654_435_761), i)));
    m
}

fn main() {
    let n = 1_000_000u32;
    let keys: Vec<u32> = (0..n).map(|i| i.wrapping_mul(2_654_435_761)).collect();

    let t = Instant::now();
    let std_map: HashMap<u32, u32> = workload(n);
    let hits_std: u32 = keys.iter().filter(|k| std_map.contains_key(*k)).count() as u32;
    let std_time = t.elapsed();

    let t = Instant::now();
    let fx_map: FxMap<u32, u32> = workload(n);
    let hits_fx: u32 = keys.iter().filter(|k| fx_map.contains_key(*k)).count() as u32;
    let fx_time = t.elapsed();

    black_box((&std_map, &fx_map));
    assert_eq!(hits_std, hits_fx);
    println!("{n} inserts + {n} lookups: SipHash {std_time:.2?} | Fx {fx_time:.2?}");

    let t = Instant::now();
    let mut presized: FxMap<u32, u32> =
        FxMap::with_capacity_and_hasher(n as usize, Default::default());
    presized.extend(keys.iter().map(|&k| (k, 0)));
    println!(
        "Fx + with_capacity inserts only: {:.2?} (avoids rehash growth)",
        t.elapsed()
    );

    let t = Instant::now();
    let mut dense = vec![0u32; n as usize];
    for i in 0..n as usize {
        dense[i] = i as u32;
    }
    black_box(&dense);
    println!(
        "dense Vec<u32> indexed by id: {:.2?} (no hashing at all)",
        t.elapsed()
    );
}
