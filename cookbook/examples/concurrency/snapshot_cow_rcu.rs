//! RECIPE 147 (concurrency): read-mostly shared state with copy-on-write snapshots (RCU style).
//! LEVEL: advanced
//! Use in: hot-reloaded config, routing tables, feature flags, certificate stores, ACLs read on every request.   Run: cargo run --release --example snapshot_cow_rcu
#![allow(dead_code)]
//!
//! WHAT: `Snapshot<T>` = `RwLock<Arc<T>>`. A reader takes the read lock only long enough to clone the `Arc` (a refcount
//! bump), releases the lock, then works on its private, immutable version for as long as it likes. A writer builds a
//! complete NEW `T` off to the side and swaps the pointer under the write lock. Readers holding the old version keep it
//! alive via the Arc; it is freed when the last one drops it. No reader ever sees a half-updated value.
//! WHY: guarding a multi-field struct with per-field atomics gives torn reads (a was updated, b not yet). The demo
//! below runs both: the snapshot version must show ZERO torn reads; the racy per-field-atomic version typically shows many
//! (timing-dependent, so only printed). The racy version is not UB; it is a logic race.
//! RCU ("read-copy-update") is the same idea in the Linux kernel: readers are wait-free, writers copy, publish with a
//! pointer swap, and reclaim the old version after a grace period. Here `Arc` refcounting does the reclamation.
//! `arc-swap` (https://github.com/vorner/arc-swap) replaces the RwLock with a lock-free pointer swap so even the
//! Arc clone never blocks; same API shape (`load`, `store`).
//! `update` is a read-modify-write under the write lock (so concurrent updates are not lost); `store` just replaces.
//! PERSISTENT DATA STRUCTURES: copying the whole value per write is O(n). Persistent structures share unchanged parts
//! between versions, so a new version costs O(change). Below, `PStack` is an immutable linked list: pushing creates one
//! new node pointing at the old list via `Arc`, and old versions stay valid and unchanged.
//! COMMON USES: config reload on SIGHUP, service discovery tables, compiled regex/rule sets swapped atomically, Rust
//! crates like `im` and `rpds` provide full persistent collections for this pattern.
//! Read: https://docs.rs/arc-swap/latest/arc_swap/ and https://marabos.nl/atomics/ and https://en.wikipedia.org/wiki/Read-copy-update
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering::*};
use std::sync::{Arc, RwLock};
use std::thread;

pub struct Snapshot<T> {
    cur: RwLock<Arc<T>>,
    epoch: AtomicU64,
}

impl<T> Snapshot<T> {
    pub fn new(v: T) -> Self {
        Self {
            cur: RwLock::new(Arc::new(v)),
            epoch: AtomicU64::new(0),
        }
    }
    /// Cheap: lock, clone the Arc, unlock. The returned version is immutable and stays valid forever.
    pub fn load(&self) -> Arc<T> {
        self.cur.read().unwrap_or_else(|e| e.into_inner()).clone()
    }
    /// Publish a fully built new version; returns the new epoch (number of swaps so far).
    pub fn store(&self, v: T) -> u64 {
        let new = Arc::new(v); // allocate and build outside the lock
        let mut g = self.cur.write().unwrap_or_else(|e| e.into_inner());
        *g = new;
        self.epoch.fetch_add(1, Release) + 1
    }
    /// Read-modify-write: builds the new version from the current one while holding the write lock.
    pub fn update(&self, f: impl FnOnce(&T) -> T) -> u64 {
        let mut g = self.cur.write().unwrap_or_else(|e| e.into_inner());
        *g = Arc::new(f(&g));
        self.epoch.fetch_add(1, Release) + 1
    }
    pub fn epoch(&self) -> u64 {
        self.epoch.load(Acquire)
    }
}

/// Invariant: a + b == 100 in every published version.
#[derive(Clone, Debug)]
struct Config {
    a: u64,
    b: u64,
    generation: u64,
}
impl Config {
    fn make(g: u64) -> Config {
        let a = g % 101;
        Config {
            a,
            b: 100 - a,
            generation: g,
        }
    }
}

/// The deliberately broken design: per-field atomics, two separate stores.
struct RacyConfig {
    a: AtomicU64,
    b: AtomicU64,
}

fn snapshot_demo() {
    const SWAPS: u64 = 20_000;
    let snap = Snapshot::new(Config::make(0));
    let stop = AtomicBool::new(false);
    let (snap, stop) = (&snap, &stop);
    let reads: Vec<u64> = thread::scope(|s| {
        let readers: Vec<_> = (0..8)
            .map(|_| {
                s.spawn(move || {
                    let (mut n, mut last_gen) = (0u64, 0u64);
                    while !stop.load(Acquire) {
                        let c = snap.load();
                        assert_eq!(c.a + c.b, 100, "torn snapshot: {c:?}");
                        assert!(c.generation >= last_gen, "versions went backwards");
                        last_gen = c.generation;
                        n += 1;
                    }
                    n
                })
            })
            .collect();
        for g in 1..=SWAPS {
            snap.store(Config::make(g));
        }
        stop.store(true, Release);
        readers.into_iter().map(|h| h.join().unwrap()).collect()
    });
    assert_eq!(snap.epoch(), SWAPS);
    assert_eq!(snap.load().generation, SWAPS);
    // an old version stays valid and unchanged while newer ones are published
    let old = snap.load();
    snap.update(|c| Config::make(c.generation + 1));
    assert_eq!((old.generation, snap.load().generation), (SWAPS, SWAPS + 1));
    println!(
        "snapshot: {} reads over {SWAPS} swaps, torn reads = 0",
        reads.iter().sum::<u64>()
    );
}

fn racy_demo() -> u64 {
    const WRITES: u64 = 300_000;
    let cfg = RacyConfig {
        a: AtomicU64::new(0),
        b: AtomicU64::new(100),
    };
    let stop = AtomicBool::new(false);
    let (cfg, stop) = (&cfg, &stop);
    let torn: u64 = thread::scope(|s| {
        let readers: Vec<_> = (0..8)
            .map(|_| {
                s.spawn(move || {
                    let mut torn = 0u64;
                    while !stop.load(Acquire) {
                        let a = cfg.a.load(Relaxed);
                        let b = cfg.b.load(Relaxed);
                        torn += (a + b != 100) as u64;
                    }
                    torn
                })
            })
            .collect();
        for g in 1..=WRITES {
            let a = g % 101;
            cfg.a.store(a, Relaxed); // a reader can run between these two stores...
            cfg.b.store(100 - a, Relaxed); // ...and see the new a with the old b
        }
        stop.store(true, Release);
        readers.into_iter().map(|h| h.join().unwrap()).sum()
    });
    println!(
        "racy per-field atomics: torn reads = {torn} (timing-dependent, usually > 0; not asserted)"
    );
    torn
}

/// Persistent (immutable) stack: versions share their tails.
struct PNode<T> {
    val: T,
    next: Option<Arc<PNode<T>>>,
}
struct PStack<T> {
    head: Option<Arc<PNode<T>>>,
}
impl<T> Clone for PStack<T> {
    fn clone(&self) -> Self {
        Self {
            head: self.head.clone(),
        }
    }
}
impl<T> PStack<T> {
    fn new() -> Self {
        Self { head: None }
    }
    /// O(1): one new node; the whole old list is shared, not copied.
    fn push(&self, val: T) -> Self {
        Self {
            head: Some(Arc::new(PNode {
                val,
                next: self.head.clone(),
            })),
        }
    }
    fn pop(&self) -> Option<(&T, Self)> {
        self.head.as_ref().map(|n| {
            (
                &n.val,
                Self {
                    head: n.next.clone(),
                },
            )
        })
    }
    fn iter(&self) -> impl Iterator<Item = &T> {
        let mut cur = self.head.as_deref();
        std::iter::from_fn(move || {
            let n = cur?;
            cur = n.next.as_deref();
            Some(&n.val)
        })
    }
}
impl<T> Drop for PStack<T> {
    // iterative drop: avoids stack overflow from recursive Arc drops on long lists
    fn drop(&mut self) {
        let mut cur = self.head.take();
        while let Some(n) = cur {
            match Arc::try_unwrap(n) {
                Ok(mut node) => cur = node.next.take(),
                Err(_) => break, // still shared by another version: that version will free the rest
            }
        }
    }
}

fn persistent_demo() {
    let v1 = PStack::new().push(1).push(2).push(3);
    let v2 = v1.push(4); // shares all of v1
    let (top, v3) = v1.pop().unwrap(); // v3 shares v1's tail too
    assert_eq!(*top, 3);
    assert_eq!(v1.iter().copied().collect::<Vec<_>>(), [3, 2, 1]);
    assert_eq!(v2.iter().copied().collect::<Vec<_>>(), [4, 3, 2, 1]);
    assert_eq!(v3.iter().copied().collect::<Vec<_>>(), [2, 1]);
    let tail_of_v2 = v2.head.as_ref().unwrap().next.as_ref().unwrap();
    assert!(
        Arc::ptr_eq(tail_of_v2, v1.head.as_ref().unwrap()),
        "v2's tail IS v1's head node"
    );
    assert_eq!(Arc::strong_count(v1.head.as_ref().unwrap()), 2); // v1 itself + v2's `next`
    // use it as the payload of a Snapshot: writers push, readers keep consistent old versions
    let shared = Snapshot::new(PStack::<u32>::new());
    let before = shared.load();
    for i in 0..1000 {
        shared.update(|s| s.push(i));
    }
    assert_eq!(before.iter().count(), 0);
    assert_eq!(shared.load().iter().count(), 1000);
    drop(PStack::<u32>::new().push(1)); // exercise Drop
    let long = (0..200_000).fold(PStack::new(), |s, i| s.push(i));
    drop(long); // would overflow the stack with a recursive Drop
    println!(
        "persistent stack: versions share structure (ptr_eq verified), old versions unchanged"
    );
}

fn main() {
    for round in 0..3 {
        print!("round {round}: ");
        snapshot_demo();
    }
    racy_demo();
    persistent_demo();
    println!("all assertions passed");
}
