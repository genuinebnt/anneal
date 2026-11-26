//! RECIPE 146 (concurrency): a concurrent HashMap by lock striping: N shards, each `RwLock<HashMap>`.
//! LEVEL: intermediate
//! Use in: caches, session tables, in-memory indexes, per-key counters shared by many threads.   Run: cargo run --release --example sharded_map
#![allow(dead_code)]
//!
//! WHAT: `ShardedMap<K, V>` splits one big map into N independent maps. A key's hash picks its shard, and only that
//! shard's lock is taken, so operations on different shards never contend. One `Mutex<HashMap>` serialises everything.
//! HOW: `get` takes a shard read lock (many readers in parallel), `insert/remove/update` take the write lock.
//! `update_or_insert` does "entry API" style read-modify-write atomically under ONE write lock, which a separate
//! `get` then `insert` could not (two threads would both see "absent" and one increment would be lost).
//! TRADE-OFFS: `len` locks shards one by one, so it is only a point-in-time estimate under concurrent writes; there is
//! no iteration with a stable view; more shards = less contention but more memory. A hot single key still serialises.
//! FALSE SHARING: neighbouring `RwLock`s in a `Vec` can share a cache line, so a lock write on shard i invalidates
//! shard i+1's line. Each shard is wrapped in `#[repr(align(64))]` (see recipe 85) to give every lock its own line.
//! Numbers printed below are machine-dependent and are NOT asserted; correctness is.
//! COMMON USES: in-process caches, connection/session registries, metrics registries keyed by name, deduplication sets.
//! The dashmap crate (https://github.com/xacrimon/dashmap) is this idea, production-grade; for read-mostly data see
//! recipe 147 (snapshots) which avoids per-read locking; for very hot maps consider lock-free/concurrent tries.
//! Read: https://doc.rust-lang.org/std/sync/struct.RwLock.html and https://github.com/xacrimon/dashmap
use std::collections::HashMap;
use std::collections::hash_map::RandomState;
use std::hash::{BuildHasher, Hash};
use std::sync::{Mutex, RwLock};
use std::thread;
use std::time::Instant;

#[repr(align(64))]
struct Shard<K, V>(RwLock<HashMap<K, V>>);

pub struct ShardedMap<K, V> {
    shards: Vec<Shard<K, V>>,
    hasher: RandomState,
}

impl<K: Hash + Eq, V> ShardedMap<K, V> {
    pub fn new(shards: usize) -> Self {
        assert!(shards >= 1);
        Self {
            shards: (0..shards)
                .map(|_| Shard(RwLock::new(HashMap::new())))
                .collect(),
            hasher: RandomState::new(),
        }
    }
    fn shard(&self, k: &K) -> &RwLock<HashMap<K, V>> {
        &self.shards[(self.hasher.hash_one(k) as usize) % self.shards.len()].0
    }
    // A panic while holding a lock poisons it; the map data is still structurally valid, so we keep going.
    pub fn get(&self, k: &K) -> Option<V>
    where
        V: Clone,
    {
        self.shard(k)
            .read()
            .unwrap_or_else(|e| e.into_inner())
            .get(k)
            .cloned()
    }
    pub fn insert(&self, k: K, v: V) -> Option<V> {
        self.shard(&k)
            .write()
            .unwrap_or_else(|e| e.into_inner())
            .insert(k, v)
    }
    pub fn remove(&self, k: &K) -> Option<V> {
        self.shard(k)
            .write()
            .unwrap_or_else(|e| e.into_inner())
            .remove(k)
    }
    /// Mutate in place under the shard's write lock; None if the key is absent.
    pub fn update<R>(&self, k: &K, f: impl FnOnce(&mut V) -> R) -> Option<R> {
        self.shard(k)
            .write()
            .unwrap_or_else(|e| e.into_inner())
            .get_mut(k)
            .map(f)
    }
    /// Atomic "insert default if absent, then update": the entry API for a concurrent map.
    pub fn update_or_insert<R>(
        &self,
        k: K,
        default: impl FnOnce() -> V,
        f: impl FnOnce(&mut V) -> R,
    ) -> R {
        let mut g = self.shard(&k).write().unwrap_or_else(|e| e.into_inner());
        f(g.entry(k).or_insert_with(default))
    }
    pub fn len(&self) -> usize {
        self.shards
            .iter()
            .map(|s| s.0.read().unwrap_or_else(|e| e.into_inner()).len())
            .sum()
    }
    pub fn into_map(self) -> HashMap<K, V> {
        self.shards
            .into_iter()
            .flat_map(|s| s.0.into_inner().unwrap_or_else(|e| e.into_inner()))
            .collect()
    }
}

/// Minimal interface so the benchmark can drive both implementations.
trait Kv: Sync {
    fn read(&self, k: u64) -> Option<u64>;
    fn bump(&self, k: u64);
}
impl Kv for Mutex<HashMap<u64, u64>> {
    fn read(&self, k: u64) -> Option<u64> {
        self.lock().unwrap().get(&k).copied()
    }
    fn bump(&self, k: u64) {
        *self.lock().unwrap().entry(k).or_insert(0) += 1;
    }
}
impl Kv for ShardedMap<u64, u64> {
    fn read(&self, k: u64) -> Option<u64> {
        self.get(&k)
    }
    fn bump(&self, k: u64) {
        self.update_or_insert(k, || 0, |v| *v += 1);
    }
}

/// Million operations per second for 8 threads; `read_pct` percent of ops are reads.
fn bench(m: &dyn Kv, read_pct: u64) -> f64 {
    const THREADS: u64 = 8;
    const OPS: u64 = 250_000;
    for k in 0..4096 {
        m.bump(k);
    }
    let t = Instant::now();
    thread::scope(|s| {
        for id in 0..THREADS {
            s.spawn(move || {
                let mut x = 0x9E37_79B9_7F4A_7C15u64 ^ (id + 1); // xorshift64 RNG
                let mut hits = 0u64;
                for _ in 0..OPS {
                    x ^= x << 13;
                    x ^= x >> 7;
                    x ^= x << 17;
                    let key = (x >> 8) % 4096;
                    if x % 100 < read_pct {
                        hits += m.read(key).is_some() as u64;
                    } else {
                        m.bump(key);
                    }
                }
                std::hint::black_box(hits);
            });
        }
    });
    (THREADS * OPS) as f64 / t.elapsed().as_secs_f64() / 1e6
}

fn correctness() {
    // 1. disjoint keys per thread: result must equal the sequential computation
    let map = ShardedMap::new(16);
    thread::scope(|s| {
        for t in 0..8u64 {
            let map = &map;
            s.spawn(move || {
                for i in 0..5_000u64 {
                    map.insert(t * 10_000 + i, i);
                    map.update(&(t * 10_000 + i), |v| *v *= 2);
                }
                for i in (0..5_000u64).step_by(2) {
                    assert_eq!(map.remove(&(t * 10_000 + i)), Some(i * 2));
                }
            });
        }
    });
    let mut expected = HashMap::new();
    for t in 0..8u64 {
        for i in (1..5_000u64).step_by(2) {
            expected.insert(t * 10_000 + i, i * 2);
        }
    }
    assert_eq!(map.len(), expected.len());
    assert_eq!(map.get(&1), Some(2));
    assert_eq!(map.get(&2), None);
    assert_eq!(map.update(&999_999, |_| ()), None);
    assert_eq!(map.into_map(), expected);

    // 2. contended counters: 8 threads hammer 16 keys; no increment may be lost
    let counters = ShardedMap::new(4);
    thread::scope(|s| {
        for t in 0..8u64 {
            let counters = &counters;
            s.spawn(move || {
                for i in 0..50_000u64 {
                    counters.update_or_insert((i + t) % 16, || 0u64, |v| *v += 1);
                }
            });
        }
    });
    let mut expected: HashMap<u64, u64> = HashMap::new();
    for t in 0..8u64 {
        for i in 0..50_000u64 {
            *expected.entry((i + t) % 16).or_insert(0) += 1;
        }
    }
    assert_eq!(counters.len(), 16);
    assert_eq!(counters.into_map(), expected);
    println!(
        "correctness ok: disjoint inserts/removes and 400k contended increments match the sequential result"
    );
}

fn main() {
    correctness();
    println!("throughput, 8 threads, Mops/s (machine-dependent, not asserted)");
    for (name, read_pct) in [
        ("read-heavy 95% get", 95u64),
        ("write-heavy 90% update", 10),
    ] {
        let base = Mutex::new(HashMap::new());
        print!("{name:24} Mutex<HashMap>: {:6.1}", bench(&base, read_pct));
        for n in [1, 4, 16, 64] {
            let m = ShardedMap::new(n);
            print!(" | {n:2} shards: {:6.1}", bench(&m, read_pct));
        }
        println!();
    }
    println!("all assertions passed");
}
