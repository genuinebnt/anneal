//! RECIPE 900 (concurrency): concurrent hash map designs: lock striping, per-bucket RwLock, seqlock readers, incremental resize.
//! LEVEL: advanced
//! Use in: choosing a shared cache/registry/session-table design, explaining why `Mutex<HashMap>` stops scaling, interviews on dashmap/ConcurrentHashMap.   Run: cargo run --release --example concurrent_hashmap_designs
//! COMMON USES: in-process caches, connection/session registries, memoisation tables, metrics registries; Java's `ConcurrentHashMap`, Rust's `dashmap` (striped RwLocks), Linux `rhashtable` (incremental resize) and Go's `sync.Map` solve the same problem with different trade-offs.
//! Course: CMU 15-445 (hash tables lecture) for the single-threaded part; Herlihy & Shavit, "The Art of Multiprocessor Programming" ch. 13 (concurrent hashing) for the rest.
#![allow(dead_code)]
//!
//! WHAT: five designs behind one `ConcMap` trait, verified the same way:
//!   1. `Global`   : `Mutex<HashMap>`. One lock = the baseline. Correct, but every operation serialises.
//!   2. `Striped`  : N independent `Mutex<HashMap>` shards chosen by hash. Contention drops ~N-fold when keys spread.
//!   3. `RwBuckets`: fixed bucket array, each bucket its own `RwLock<Vec<(k,v)>>`. Readers of one bucket do not block each other.
//!   4. `SeqMap`   : writers take a per-stripe mutex and bump a version counter (odd = write in progress); readers take NO lock:
//!                   read version, read slots, re-read version, retry if it changed (a seqlock). Slots are atomics so a racing read is
//!                   defined behaviour (a torn *snapshot* is possible, a torn *word* is not) and the version check discards torn snapshots.
//!   5. `GrowMap`  : chained table that doubles WITHOUT a stop-the-world rehash: the old table is drained a few buckets per operation
//!                   (the idea behind Redis's dict rehash and Linux rhashtable). We measure the worst per-operation pause.
//! NOT built (described): split-ordered lists (Shalev & Shavit). Keep ONE lock-free sorted linked list of all items, ordered by the
//! bit-REVERSED hash; buckets are just shortcut pointers (dummy nodes) into that list. Doubling the table then only adds new
//! shortcuts; no item ever moves, which is what makes lock-free resizing possible.
//! WHY hash then mask: `h & (n-1)` needs n a power of two; we mix keys with splitmix64 first because sequential ints would
//! otherwise map to neighbouring buckets (and, with striping, to the same stripe pattern).
//! VERIFICATION: (a) 8-thread stress with exact totals (disjoint inserts, contended `add`, removes); (b) a Wing-Gong style
//! LINEARIZABILITY checker: record (invoke, return, op, result) with a global ticket counter for random 3-thread histories on a
//! tiny key space, then search for a sequential order that respects real-time order and the map's sequential spec; (c) the checker
//! itself is tested on a hand-made non-linearizable history so we know it can say "no".
//! Read: Herlihy & Shavit, The Art of Multiprocessor Programming (ch. 13). Shalev & Shavit, "Split-Ordered Lists: Lock-Free Extensible Hash Tables" (JACM 2006, title only, not re-read).
//! Read: Wing & Gong, "Testing and Verifying Concurrent Objects" (1993, title only). Boehm, "Can Seqlocks Get Along with Programming Language Memory Models?" (title only).
//! Read: https://docs.rs/dashmap/latest/dashmap/ and https://www.1024cores.net/
//!
//! TRY NEXT:
//!   - [S] In `Striped`, change the shard index from `h >> 32` to `h & mask` of the SAME hash used inside the shard's HashMap and
//!         predict what happens to per-shard collisions, then measure the `bench` numbers (hint: hashbrown also uses hash bits).
//!   - [S] Raise `SeqMap`'s write ratio in `bench` from 5% to 50%. Predict the crossover where `RwBuckets` beats it; print retries counted in `SeqStripe::get`.
//!   - [M] Make `GrowMap` concurrent by giving each *old* bucket its own `Mutex` and migrating lazily on touch; keep `max_moved_per_op` low and re-run `check_linearizable`.
//!   - [M] Port `RwBuckets` to hold `Arc<V>` values so readers clone a pointer instead of a value; what breaks for `add`? Compare with `snapshot_cow_rcu`.
//!   - [A] PROJECT: use the best map here as the index of a sharded LRU or session cache, and reuse the 3-thread history checker from this file together
//!         with `sharded_map` and `model_checker_for_concurrent_code` to check an `insert_if_absent` that you add to the trait.

use std::collections::{HashMap, HashSet};
use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering::*, fence};
use std::sync::{Mutex, RwLock};
use std::thread;
use std::time::Instant;

// ---------------------------------------------------------------- utilities
struct Rng(u64);
impl Rng {
    fn new(seed: u64) -> Self {
        Rng(seed.max(1))
    }
    fn next(&mut self) -> u64 {
        let mut x = self.0;
        x ^= x << 13;
        x ^= x >> 7;
        x ^= x << 17;
        self.0 = x;
        x
    }
    fn below(&mut self, n: u64) -> u64 {
        self.next() % n
    }
}

fn mix(mut z: u64) -> u64 {
    z = z.wrapping_add(0x9E37_79B9_7F4A_7C15);
    z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
    z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
    z ^ (z >> 31)
}

trait ConcMap: Sync {
    fn name(&self) -> &'static str;
    fn insert(&self, k: u64, v: u64) -> Option<u64>;
    fn get(&self, k: u64) -> Option<u64>;
    fn remove(&self, k: u64) -> Option<u64>;
    /// Atomic read-modify-write: value += d (absent counts as 0); returns the new value.
    fn add(&self, k: u64, d: u64) -> u64;
    fn len(&self) -> usize;
}

// ---------------------------------------------------------------- 1. global lock
struct Global(Mutex<HashMap<u64, u64>>);
impl Global {
    fn new() -> Self {
        Global(Mutex::new(HashMap::new()))
    }
}
impl ConcMap for Global {
    fn name(&self) -> &'static str {
        "global Mutex"
    }
    fn insert(&self, k: u64, v: u64) -> Option<u64> {
        self.0.lock().unwrap().insert(k, v)
    }
    fn get(&self, k: u64) -> Option<u64> {
        self.0.lock().unwrap().get(&k).copied()
    }
    fn remove(&self, k: u64) -> Option<u64> {
        self.0.lock().unwrap().remove(&k)
    }
    fn add(&self, k: u64, d: u64) -> u64 {
        let mut g = self.0.lock().unwrap();
        let e = g.entry(k).or_insert(0);
        *e += d;
        *e
    }
    fn len(&self) -> usize {
        self.0.lock().unwrap().len()
    }
}

// ---------------------------------------------------------------- 2. lock striping
struct Striped {
    shards: Vec<Mutex<HashMap<u64, u64>>>,
}
impl Striped {
    fn new(n: usize) -> Self {
        Striped {
            shards: (0..n).map(|_| Mutex::new(HashMap::new())).collect(),
        }
    }
    // Use HIGH hash bits for the shard so the shard's own HashMap (which hashes again) is unaffected.
    fn shard(&self, k: u64) -> &Mutex<HashMap<u64, u64>> {
        &self.shards[(mix(k) >> 40) as usize % self.shards.len()]
    }
}
impl ConcMap for Striped {
    fn name(&self) -> &'static str {
        "striped x32"
    }
    fn insert(&self, k: u64, v: u64) -> Option<u64> {
        self.shard(k).lock().unwrap().insert(k, v)
    }
    fn get(&self, k: u64) -> Option<u64> {
        self.shard(k).lock().unwrap().get(&k).copied()
    }
    fn remove(&self, k: u64) -> Option<u64> {
        self.shard(k).lock().unwrap().remove(&k)
    }
    fn add(&self, k: u64, d: u64) -> u64 {
        let mut g = self.shard(k).lock().unwrap();
        let e = g.entry(k).or_insert(0);
        *e += d;
        *e
    }
    fn len(&self) -> usize {
        self.shards.iter().map(|s| s.lock().unwrap().len()).sum()
    }
}

// ---------------------------------------------------------------- 3. per-bucket RwLock
struct RwBuckets {
    buckets: Vec<RwLock<Vec<(u64, u64)>>>,
    count: AtomicUsize,
}
impl RwBuckets {
    fn new(n: usize) -> Self {
        assert!(n.is_power_of_two());
        RwBuckets {
            buckets: (0..n).map(|_| RwLock::new(Vec::new())).collect(),
            count: AtomicUsize::new(0),
        }
    }
    fn bucket(&self, k: u64) -> &RwLock<Vec<(u64, u64)>> {
        &self.buckets[mix(k) as usize & (self.buckets.len() - 1)]
    }
}
impl ConcMap for RwBuckets {
    fn name(&self) -> &'static str {
        "RwLock buckets"
    }
    fn insert(&self, k: u64, v: u64) -> Option<u64> {
        let mut b = self.bucket(k).write().unwrap();
        if let Some(e) = b.iter_mut().find(|e| e.0 == k) {
            return Some(std::mem::replace(&mut e.1, v));
        }
        b.push((k, v));
        self.count.fetch_add(1, Relaxed);
        None
    }
    fn get(&self, k: u64) -> Option<u64> {
        self.bucket(k)
            .read()
            .unwrap()
            .iter()
            .find(|e| e.0 == k)
            .map(|e| e.1)
    }
    fn remove(&self, k: u64) -> Option<u64> {
        let mut b = self.bucket(k).write().unwrap();
        let i = b.iter().position(|e| e.0 == k)?;
        self.count.fetch_sub(1, Relaxed);
        Some(b.swap_remove(i).1)
    }
    fn add(&self, k: u64, d: u64) -> u64 {
        let mut b = self.bucket(k).write().unwrap();
        if let Some(e) = b.iter_mut().find(|e| e.0 == k) {
            e.1 += d;
            return e.1;
        }
        b.push((k, d));
        self.count.fetch_add(1, Relaxed);
        d
    }
    fn len(&self) -> usize {
        self.count.load(Relaxed)
    }
}

// ---------------------------------------------------------------- 4. seqlock readers
const EMPTY: u64 = 0;
const TOMB: u64 = u64::MAX;

struct SeqStripe {
    seq: AtomicU64,
    writer: Mutex<()>,
    keys: Vec<AtomicU64>, // stored key = user key + 1 (0 is EMPTY); TOMB marks a deleted slot
    vals: Vec<AtomicU64>,
    retries: AtomicU64,
}
impl SeqStripe {
    fn new(cap: usize) -> Self {
        assert!(cap.is_power_of_two());
        SeqStripe {
            seq: AtomicU64::new(0),
            writer: Mutex::new(()),
            keys: (0..cap).map(|_| AtomicU64::new(EMPTY)).collect(),
            vals: (0..cap).map(|_| AtomicU64::new(0)).collect(),
            retries: AtomicU64::new(0),
        }
    }
    fn mask(&self) -> usize {
        self.keys.len() - 1
    }
    /// Racy lookup: may observe a half-finished write; the caller validates with the version counter.
    fn find_racy(&self, sk: u64, h: u64) -> Option<u64> {
        let mut i = h as usize & self.mask();
        for _ in 0..self.keys.len() {
            let k = self.keys[i].load(Relaxed);
            if k == sk {
                return Some(self.vals[i].load(Relaxed));
            }
            if k == EMPTY {
                return None;
            }
            i = (i + 1) & self.mask();
        }
        None
    }
    fn get(&self, sk: u64, h: u64) -> Option<u64> {
        loop {
            let s1 = self.seq.load(Acquire);
            if s1 & 1 == 1 {
                self.retries.fetch_add(1, Relaxed);
                std::hint::spin_loop();
                continue;
            }
            let r = self.find_racy(sk, h);
            fence(Acquire); // order the slot reads before the validating re-read
            if self.seq.load(Relaxed) == s1 {
                return r;
            }
            self.retries.fetch_add(1, Relaxed);
        }
    }
    /// Under the writer lock. Returns (index of existing key, first reusable slot).
    fn locate(&self, sk: u64, h: u64) -> (Option<usize>, Option<usize>) {
        let mut i = h as usize & self.mask();
        let mut free = None;
        for _ in 0..self.keys.len() {
            let k = self.keys[i].load(Relaxed);
            if k == sk {
                return (Some(i), free);
            }
            if k == TOMB && free.is_none() {
                free = Some(i);
            }
            if k == EMPTY {
                return (None, free.or(Some(i)));
            }
            i = (i + 1) & self.mask();
        }
        (None, free)
    }
    fn write<R>(&self, f: impl FnOnce() -> R) -> R {
        let s = self.seq.load(Relaxed);
        self.seq.store(s + 1, Relaxed); // odd: readers will retry
        fence(Release); // the odd value must be visible before any slot write
        let r = f();
        self.seq.store(s + 2, Release); // even again: publishes the slot writes
        r
    }
}
struct SeqMap {
    stripes: Vec<SeqStripe>,
}
impl SeqMap {
    fn new(stripes: usize, cap: usize) -> Self {
        SeqMap {
            stripes: (0..stripes).map(|_| SeqStripe::new(cap)).collect(),
        }
    }
    fn at(&self, k: u64) -> (&SeqStripe, u64, u64) {
        let h = mix(k);
        (
            &self.stripes[(h >> 48) as usize % self.stripes.len()],
            k + 1,
            h,
        )
    }
    fn retries(&self) -> u64 {
        self.stripes.iter().map(|s| s.retries.load(Relaxed)).sum()
    }
}
impl ConcMap for SeqMap {
    fn name(&self) -> &'static str {
        "seqlock readers"
    }
    fn insert(&self, k: u64, v: u64) -> Option<u64> {
        let (s, sk, h) = self.at(k);
        let _g = s.writer.lock().unwrap();
        let (found, free) = s.locate(sk, h);
        s.write(|| match found {
            Some(i) => {
                let old = s.vals[i].swap(v, Relaxed);
                Some(old)
            }
            None => {
                let i = free.expect("stripe full: this demo table does not resize");
                s.vals[i].store(v, Relaxed);
                s.keys[i].store(sk, Relaxed);
                None
            }
        })
    }
    fn get(&self, k: u64) -> Option<u64> {
        let (s, sk, h) = self.at(k);
        s.get(sk, h)
    }
    fn remove(&self, k: u64) -> Option<u64> {
        let (s, sk, h) = self.at(k);
        let _g = s.writer.lock().unwrap();
        let (found, _) = s.locate(sk, h);
        let i = found?;
        Some(s.write(|| {
            let old = s.vals[i].load(Relaxed);
            s.keys[i].store(TOMB, Relaxed);
            old
        }))
    }
    fn add(&self, k: u64, d: u64) -> u64 {
        let (s, sk, h) = self.at(k);
        let _g = s.writer.lock().unwrap();
        let (found, free) = s.locate(sk, h);
        s.write(|| match found {
            Some(i) => s.vals[i].fetch_add(d, Relaxed) + d,
            None => {
                let i = free.expect("stripe full");
                s.vals[i].store(d, Relaxed);
                s.keys[i].store(sk, Relaxed);
                d
            }
        })
    }
    fn len(&self) -> usize {
        let mut n = 0;
        for s in &self.stripes {
            let _g = s.writer.lock().unwrap();
            n += s
                .keys
                .iter()
                .filter(|k| !matches!(k.load(Relaxed), EMPTY | TOMB))
                .count();
        }
        n
    }
}

// ---------------------------------------------------------------- 5. incremental resize
const STEP: usize = 4; // old buckets migrated per operation
struct GrowInner {
    new: Vec<Vec<(u64, u64)>>,
    old: Vec<Vec<(u64, u64)>>, // empty unless a migration is running
    migrated: usize,           // old[..migrated] have been drained
    len: usize,
    max_moved_per_op: usize,
    resizes: usize,
}
impl GrowInner {
    fn idx(h: u64, n: usize) -> usize {
        h as usize & (n - 1)
    }
    /// Drain a few old buckets into the new table. Bounded work per call = bounded pause.
    fn step(&mut self) {
        if self.old.is_empty() {
            return;
        }
        let end = (self.migrated + STEP).min(self.old.len());
        let mut moved = 0;
        for b in self.migrated..end {
            for (k, v) in std::mem::take(&mut self.old[b]) {
                let i = Self::idx(mix(k), self.new.len());
                self.new[i].push((k, v));
                moved += 1;
            }
        }
        self.migrated = end;
        if end == self.old.len() {
            self.old = Vec::new();
            self.migrated = 0;
        }
        self.max_moved_per_op = self.max_moved_per_op.max(moved);
    }
    fn maybe_start_growth(&mut self) {
        if self.old.is_empty() && self.len > self.new.len() {
            let n = self.new.len() * 2;
            self.old = std::mem::replace(&mut self.new, vec![Vec::new(); n]);
            self.migrated = 0;
            self.resizes += 1;
        }
    }
    /// An old bucket not yet migrated still owns its keys.
    fn old_bucket(&mut self, k: u64) -> Option<&mut Vec<(u64, u64)>> {
        if self.old.is_empty() {
            return None;
        }
        let i = Self::idx(mix(k), self.old.len());
        if i >= self.migrated {
            Some(&mut self.old[i])
        } else {
            None
        }
    }
    fn take_from_old(&mut self, k: u64) -> Option<u64> {
        let b = self.old_bucket(k)?;
        let p = b.iter().position(|e| e.0 == k)?;
        Some(b.swap_remove(p).1)
    }
    fn find_mut(&mut self, k: u64) -> Option<&mut u64> {
        let i = Self::idx(mix(k), self.new.len());
        if let Some(p) = self.new[i].iter().position(|e| e.0 == k) {
            return Some(&mut self.new[i][p].1);
        }
        self.old_bucket(k)?
            .iter_mut()
            .find(|e| e.0 == k)
            .map(|e| &mut e.1)
    }
}
struct GrowMap(Mutex<GrowInner>);
impl GrowMap {
    fn new() -> Self {
        GrowMap(Mutex::new(GrowInner {
            new: vec![Vec::new(); 4],
            old: Vec::new(),
            migrated: 0,
            len: 0,
            max_moved_per_op: 0,
            resizes: 0,
        }))
    }
}
impl ConcMap for GrowMap {
    fn name(&self) -> &'static str {
        "incremental grow"
    }
    fn insert(&self, k: u64, v: u64) -> Option<u64> {
        let mut g = self.0.lock().unwrap();
        g.step();
        let prev = g.take_from_old(k);
        let i = GrowInner::idx(mix(k), g.new.len());
        let prev = prev.or_else(|| {
            let p = g.new[i].iter().position(|e| e.0 == k)?;
            Some(g.new[i].swap_remove(p).1)
        });
        g.new[i].push((k, v));
        if prev.is_none() {
            g.len += 1;
            g.maybe_start_growth();
        }
        prev
    }
    fn get(&self, k: u64) -> Option<u64> {
        let mut g = self.0.lock().unwrap();
        g.step();
        g.find_mut(k).map(|v| *v)
    }
    fn remove(&self, k: u64) -> Option<u64> {
        let mut g = self.0.lock().unwrap();
        g.step();
        let mut r = g.take_from_old(k);
        if r.is_none() {
            let i = GrowInner::idx(mix(k), g.new.len());
            if let Some(p) = g.new[i].iter().position(|e| e.0 == k) {
                r = Some(g.new[i].swap_remove(p).1);
            }
        }
        if r.is_some() {
            g.len -= 1;
        }
        r
    }
    fn add(&self, k: u64, d: u64) -> u64 {
        let mut g = self.0.lock().unwrap();
        g.step();
        if let Some(v) = g.find_mut(k) {
            *v += d;
            return *v;
        }
        let i = GrowInner::idx(mix(k), g.new.len());
        g.new[i].push((k, d));
        g.len += 1;
        g.maybe_start_growth();
        d
    }
    fn len(&self) -> usize {
        self.0.lock().unwrap().len
    }
}

// ---------------------------------------------------------------- stress tests (exact totals)
fn stress(m: &dyn ConcMap) {
    const T: u64 = 8;
    const N: u64 = 2000;
    // (a) disjoint inserts
    thread::scope(|s| {
        for t in 0..T {
            s.spawn(move || {
                for i in 0..N {
                    let k = t * N + i;
                    assert_eq!(m.insert(k, k * 3), None);
                    assert_eq!(m.get(k), Some(k * 3), "read-your-writes");
                }
            });
        }
    });
    assert_eq!(
        m.len(),
        (T * N) as usize,
        "{}: lost or duplicated inserts",
        m.name()
    );
    // (b) contended read-modify-write on 16 hot keys (keys disjoint from (a)): exact totals prove `add` is atomic
    thread::scope(|s| {
        for _ in 0..T {
            s.spawn(|| {
                for i in 0..4096u64 {
                    m.add(1_000_000 + i % 16, 1);
                }
            });
        }
    });
    for k in 0..16 {
        assert_eq!(
            m.get(1_000_000 + k),
            Some(T * 4096 / 16),
            "{}: lost update",
            m.name()
        );
    }
    // (c) concurrent removes of the odd keys, readers verifying even keys stay intact
    thread::scope(|s| {
        for t in 0..T {
            s.spawn(move || {
                for i in 0..N {
                    let k = t * N + i;
                    if k % 2 == 1 {
                        assert_eq!(m.remove(k), Some(k * 3));
                    } else {
                        assert_eq!(m.get(k), Some(k * 3));
                    }
                }
            });
        }
    });
    assert_eq!(m.len(), (T * N / 2) as usize + 16);
    assert_eq!(m.get(1), None);
}

// ---------------------------------------------------------------- linearizability checker
#[derive(Clone, Copy, Debug, PartialEq)]
enum OpKind {
    Insert(u64, u64),
    Get(u64),
    Remove(u64),
}
#[derive(Clone, Copy, Debug)]
struct Event {
    inv: u64,
    ret: u64,
    op: OpKind,
    res: Option<u64>,
}
const KEYS: usize = 2;

fn apply_spec(state: [Option<u64>; KEYS], op: OpKind) -> ([Option<u64>; KEYS], Option<u64>) {
    let mut s = state;
    match op {
        OpKind::Insert(k, v) => {
            let old = s[k as usize];
            s[k as usize] = Some(v);
            (s, old)
        }
        OpKind::Get(k) => (s, s[k as usize]),
        OpKind::Remove(k) => {
            let old = s[k as usize];
            s[k as usize] = None;
            (s, old)
        }
    }
}

/// Wing-Gong search with memoisation on (set of linearized ops, map state).
fn linearizable(h: &[Event]) -> bool {
    fn go(
        h: &[Event],
        done: u32,
        st: [Option<u64>; KEYS],
        seen: &mut HashSet<(u32, [Option<u64>; KEYS])>,
    ) -> bool {
        if done == (1u32 << h.len()) - 1 {
            return true;
        }
        if !seen.insert((done, st)) {
            return false; // already explored this configuration and it failed
        }
        for i in 0..h.len() {
            if done >> i & 1 == 1 {
                continue;
            }
            // op i is a candidate only if no other pending op *finished before it started*
            let minimal = (0..h.len()).all(|j| j == i || done >> j & 1 == 1 || h[j].ret > h[i].inv);
            if !minimal {
                continue;
            }
            let (ns, r) = apply_spec(st, h[i].op);
            if r == h[i].res && go(h, done | 1 << i, ns, seen) {
                return true;
            }
        }
        false
    }
    go(h, 0, [None; KEYS], &mut HashSet::new())
}

/// Record a random concurrent history: 3 threads x `per` ops over KEYS keys, timestamps from a shared ticket counter.
fn record_history(make: &dyn Fn() -> Box<dyn ConcMap>, seed: u64, per: usize) -> Vec<Event> {
    let m = make();
    let clock = AtomicU64::new(0);
    let mut all = Vec::new();
    thread::scope(|s| {
        let hs: Vec<_> = (0..3u64)
            .map(|t| {
                let (m, clock) = (&*m, &clock);
                s.spawn(move || {
                    let mut rng = Rng::new(seed * 31 + t + 1);
                    let mut ev = Vec::new();
                    for i in 0..per as u64 {
                        let k = rng.below(KEYS as u64);
                        let op = match rng.below(3) {
                            0 => OpKind::Insert(k, t * 100 + i + 1),
                            1 => OpKind::Get(k),
                            _ => OpKind::Remove(k),
                        };
                        let inv = clock.fetch_add(1, SeqCst);
                        let res = match op {
                            OpKind::Insert(k, v) => m.insert(k, v),
                            OpKind::Get(k) => m.get(k),
                            OpKind::Remove(k) => m.remove(k),
                        };
                        let ret = clock.fetch_add(1, SeqCst);
                        ev.push(Event { inv, ret, op, res });
                    }
                    ev
                })
            })
            .collect();
        for h in hs {
            all.extend(h.join().unwrap());
        }
    });
    all
}

/// A deliberately broken map: insert reads the old value and writes in two separate critical sections.
struct Racy(Mutex<HashMap<u64, u64>>);
impl ConcMap for Racy {
    fn name(&self) -> &'static str {
        "racy (broken)"
    }
    fn insert(&self, k: u64, v: u64) -> Option<u64> {
        let old = self.0.lock().unwrap().get(&k).copied(); // lock released here: check-then-act race
        thread::yield_now();
        self.0.lock().unwrap().insert(k, v);
        old
    }
    fn get(&self, k: u64) -> Option<u64> {
        self.0.lock().unwrap().get(&k).copied()
    }
    fn remove(&self, k: u64) -> Option<u64> {
        self.0.lock().unwrap().remove(&k)
    }
    fn add(&self, k: u64, d: u64) -> u64 {
        let mut g = self.0.lock().unwrap();
        *g.entry(k).or_insert(0) += d;
        g[&k]
    }
    fn len(&self) -> usize {
        self.0.lock().unwrap().len()
    }
}

fn check_checker() {
    // T1: insert(0,1)->None completes; THEN T2: get(0)->None starts. Impossible in any linear order.
    let bad = [
        Event {
            inv: 0,
            ret: 1,
            op: OpKind::Insert(0, 1),
            res: None,
        },
        Event {
            inv: 2,
            ret: 3,
            op: OpKind::Get(0),
            res: None,
        },
    ];
    assert!(!linearizable(&bad));
    // Overlapping version is fine: the get may linearize first.
    let ok = [
        Event {
            inv: 0,
            ret: 3,
            op: OpKind::Insert(0, 1),
            res: None,
        },
        Event {
            inv: 1,
            ret: 2,
            op: OpKind::Get(0),
            res: None,
        },
    ];
    assert!(linearizable(&ok));
    // A value that was never written is never fine.
    let phantom = [Event {
        inv: 0,
        ret: 1,
        op: OpKind::Get(1),
        res: Some(9),
    }];
    assert!(!linearizable(&phantom));
}

// ---------------------------------------------------------------- benchmark (printed only)
fn bench(m: &dyn ConcMap, write_pct: u64) -> f64 {
    for k in 0..4096 {
        m.insert(k, k);
    }
    let t0 = Instant::now();
    const OPS: u64 = 150_000;
    thread::scope(|s| {
        for t in 0..8u64 {
            s.spawn(move || {
                let mut r = Rng::new(t + 7);
                for _ in 0..OPS {
                    let k = r.below(4096);
                    let dice = r.below(100);
                    if dice < write_pct / 2 {
                        m.insert(k, k);
                    } else if dice < write_pct {
                        m.remove(k);
                    } else {
                        std::hint::black_box(m.get(k));
                    }
                }
            });
        }
    });
    (8 * OPS) as f64 / t0.elapsed().as_secs_f64() / 1e6
}

fn main() {
    println!("== stress: 8 threads, exact totals ==");
    let makers: Vec<(&str, Box<dyn Fn() -> Box<dyn ConcMap>>)> = vec![
        ("global", Box::new(|| Box::new(Global::new()))),
        ("striped", Box::new(|| Box::new(Striped::new(32)))),
        ("rwbuckets", Box::new(|| Box::new(RwBuckets::new(1024)))),
        ("seqlock", Box::new(|| Box::new(SeqMap::new(16, 8192)))),
        ("grow", Box::new(|| Box::new(GrowMap::new()))),
    ];
    for (n, mk) in &makers {
        let m = mk();
        stress(&*m);
        println!("  {n:<10} ok ({})", m.name());
    }

    println!("== incremental resize: worst single-operation pause ==");
    let g = GrowMap::new();
    for k in 0..50_000u64 {
        g.insert(k, k);
    }
    let (resizes, worst, len) = {
        let i = g.0.lock().unwrap();
        (i.resizes, i.max_moved_per_op, i.len)
    };
    // A stop-the-world rehash at the last doubling would move ~len/2 entries inside ONE insert.
    println!(
        "  {resizes} doublings, worst entries moved by one op = {worst}; stop-the-world would move ~{}",
        len / 2
    );
    assert!(
        worst * 50 < len / 2,
        "migration pause should be tiny vs a full rehash"
    );
    for k in 0..50_000u64 {
        assert_eq!(g.get(k), Some(k)); // nothing lost across the migrations
    }

    println!("== linearizability: 3 threads x 4 ops, random histories ==");
    check_checker();
    for (n, mk) in makers.iter().filter(|(n, _)| *n != "grow" || true) {
        let mut ok = 0;
        for seed in 0..150 {
            let h = record_history(mk.as_ref(), seed, 4);
            assert!(linearizable(&h), "{n}: non-linearizable history {h:#?}");
            ok += 1;
        }
        println!("  {n:<10} {ok}/150 histories linearizable");
    }
    let racy: Box<dyn Fn() -> Box<dyn ConcMap>> =
        Box::new(|| Box::new(Racy(Mutex::new(HashMap::new()))));
    let caught = (0..400)
        .filter(|&s| !linearizable(&record_history(racy.as_ref(), s, 4)))
        .count();
    println!("  racy insert: checker flagged {caught}/400 histories (probabilistic, not asserted)");

    println!("== throughput, 8 threads, million ops/s (printed only; machine dependent) ==");
    println!("  {:<18}{:>10}{:>10}", "design", "5% write", "50% write");
    for (_, mk) in &makers {
        let a = bench(&*mk(), 5);
        let b = bench(&*mk(), 50);
        println!("  {:<18}{:>10.1}{:>10.1}", mk().name(), a, b);
    }
    let sm = SeqMap::new(16, 8192);
    bench(&sm, 5);
    println!(
        "  seqlock reader retries during a 5% write run: {}",
        sm.retries()
    );
    println!("all concurrent map checks passed");
}
