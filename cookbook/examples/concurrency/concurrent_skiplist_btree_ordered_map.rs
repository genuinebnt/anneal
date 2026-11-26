//! RECIPE 901 (concurrency): concurrent ordered map: the lazy skip list (Herlihy, Lev, Luchangco, Shavit) with range scans.
//! LEVEL: advanced
//! Use in: sorted in-memory indexes (LSM memtables), time-ordered event buffers, leaderboards, anything needing `range()` under concurrency.   Run: cargo run --release --example concurrent_skiplist_btree_ordered_map
//! COMMON USES: LevelDB/RocksDB memtables are skip lists; Java's `ConcurrentSkipListMap`; Redis sorted sets use a skip list for rank/range; crossbeam-skiplist is the lock-free Rust version.
//! Course: CMU 15-445 (index concurrency lectures); Herlihy & Shavit, "The Art of Multiprocessor Programming" ch. 14.
#![allow(dead_code)]
//!
//! WHY NOT A CONCURRENT B-TREE HERE: a B+tree needs latch crabbing across variable-size nodes and splits; a skip list's pointer
//! updates are local (a node is linked into `h` lists one pointer at a time), which makes a *lock-light* design small enough to
//! verify. This is the LAZY skip list: `find` takes NO locks; `insert`/`remove` lock only the few predecessor nodes, validate, and apply.
//! THE ALGORITHM (per node: `key`, `top` level, `next[level]`, `marked`, `fully_linked`, a tiny lock):
//!   find(key)   : walk from the head, top level to bottom, remembering `preds[l]` and `succs[l]`. Lock-free, may see stale pointers.
//!   insert      : find; if present and not marked, update its value under its lock. Else lock preds[0..=top] bottom-up, VALIDATE that every
//!                 pred and succ is unmarked and `pred.next[l] == succ`, retry from scratch if not; link bottom-up; set `fully_linked`.
//!   remove      : find victim; lock it; if it is not already `marked`, set `marked` (this is the LINEARIZATION POINT: the key is now logically
//!                 gone); lock preds, validate `pred.next[l] == victim` and pred unmarked; unlink; unlock.
//!   get         : find; present iff `fully_linked && !marked` (also lock-free). Contains is linearized at the read of `marked`.
//! The `marked` flag is why validation can be so cheap: a node is never repaired once marked, so seeing `!marked` under the lock is a proof.
//! MEMORY: nodes live in a pre-allocated arena (`Vec<Node>`), links are `AtomicUsize` INDICES, and a removed node is never reused. That
//! removes use-after-free, ABA and the need for hazard pointers/epochs (see `epoch_hazard_pointer_reclamation`) at the cost of a bounded
//! lifetime insert budget. Real implementations (crossbeam-skiplist) pay for epoch-based reclamation instead.
//! RANGE SCANS walk the bottom level skipping marked/not-yet-linked nodes. They are WEAKLY CONSISTENT, not a snapshot: every key that
//! was present for the whole scan appears, nothing that was never inserted appears, order is strict. Snapshot scans need versions (MVCC).
//! VERIFICATION: (1) 8 threads on DISJOINT key classes each compared op-by-op against its own `BTreeMap` model, then the union and
//! sub-ranges compared with the merged model, plus a structural check (levels sorted, upper levels subsets of the bottom);
//! (2) overlapping keys: random 3-thread x 4-op histories on 2 keys checked for linearizability with a Wing-Gong search;
//! (3) scans during churn: stable keys always present and output strictly sorted.
//! Read: Herlihy, Lev, Luchangco, Shavit, "A Simple Optimistic Skiplist Algorithm" (SIROCCO 2007, title only, not re-read here).
//! Read: Pugh, "Skip Lists: A Probabilistic Alternative to Balanced Trees" (CACM 1990, title only). https://docs.rs/crossbeam-skiplist/latest/crossbeam_skiplist/
//!
//! TRY NEXT:
//!   - [S] In `remove`, swap the order "mark victim" and "lock victim" (mark first, lock after). Predict which invariant of the scan test breaks, then run `histories` to see.
//!   - [S] Change the level probability from 1/2 to 1/4 in `random_level`; predict the effect on average levels and `find` steps (count steps with an AtomicU64), then measure.
//!   - [M] Add `pop_first()` (remove the smallest live key) and use it from 4 threads as a priority queue; assert each key is popped exactly once.
//!   - [M] Replace the never-reuse arena with a free list guarded by an epoch counter from `read_copy_update_and_hazard_patterns`; what makes recycling a node unsafe, and what must you wait for?
//!   - [A] PROJECT: put this map under a write-ahead log and a memtable flush threshold to build the memtable half of an LSM tree (compare `concurrent_hashmap_designs` for the point-lookup path).

use std::collections::{BTreeMap, HashSet};
use std::sync::atomic::{AtomicBool, AtomicU64, AtomicUsize, Ordering::*};
use std::sync::{Mutex, MutexGuard};
use std::thread;
use std::time::Instant;

const MAXH: usize = 12;
const HEAD: usize = 0;
const TAIL: usize = 1;
const NEG: u64 = 0; // head key; user keys are stored as k+1
const POS: u64 = u64::MAX; // tail key

struct Node {
    key: AtomicU64,
    val: AtomicU64,
    top: AtomicUsize,
    next: [AtomicUsize; MAXH],
    marked: AtomicBool,
    fully_linked: AtomicBool,
    lock: Mutex<()>,
}
impl Node {
    fn blank() -> Node {
        Node {
            key: AtomicU64::new(0),
            val: AtomicU64::new(0),
            top: AtomicUsize::new(0),
            next: std::array::from_fn(|_| AtomicUsize::new(TAIL)),
            marked: AtomicBool::new(false),
            fully_linked: AtomicBool::new(false),
            lock: Mutex::new(()),
        }
    }
}

struct SkipMap {
    nodes: Vec<Node>,
    used: AtomicUsize,
    seed: AtomicU64,
}

impl SkipMap {
    fn with_capacity(cap: usize) -> Self {
        let nodes: Vec<Node> = (0..cap + 2).map(|_| Node::blank()).collect();
        nodes[HEAD].key.store(NEG, Relaxed);
        nodes[HEAD].top.store(MAXH - 1, Relaxed);
        nodes[HEAD].fully_linked.store(true, Relaxed);
        nodes[TAIL].key.store(POS, Relaxed);
        nodes[TAIL].top.store(MAXH - 1, Relaxed);
        nodes[TAIL].fully_linked.store(true, Relaxed);
        SkipMap {
            nodes,
            used: AtomicUsize::new(2),
            seed: AtomicU64::new(0x1234_5678_9abc_def1),
        }
    }
    fn key(&self, i: usize) -> u64 {
        self.nodes[i].key.load(Acquire)
    }
    fn next(&self, i: usize, l: usize) -> usize {
        self.nodes[i].next[l].load(Acquire)
    }
    fn random_level(&self) -> usize {
        let s = self.seed.fetch_add(0x9E37_79B9_7F4A_7C15, Relaxed);
        let mut z = s;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        z ^= z >> 31;
        (z.trailing_ones() as usize).min(MAXH - 1) // P(level >= l) = 2^-l
    }

    /// Lock-free search. Fills preds/succs for every level; returns the highest level at which `key` was found.
    fn find(
        &self,
        key: u64,
        preds: &mut [usize; MAXH],
        succs: &mut [usize; MAXH],
    ) -> Option<usize> {
        let mut found = None;
        let mut pred = HEAD;
        for l in (0..MAXH).rev() {
            let mut cur = self.next(pred, l);
            while self.key(cur) < key {
                pred = cur;
                cur = self.next(pred, l);
            }
            if found.is_none() && self.key(cur) == key {
                found = Some(l);
            }
            preds[l] = pred;
            succs[l] = cur;
        }
        found
    }

    /// Lock distinct preds bottom-up (same global order everywhere => no deadlock).
    fn lock_preds(&self, preds: &[usize; MAXH], top: usize) -> Vec<MutexGuard<'_, ()>> {
        let mut g = Vec::new();
        let mut last = usize::MAX;
        for l in 0..=top {
            if preds[l] != last {
                g.push(self.nodes[preds[l]].lock.lock().unwrap());
                last = preds[l];
            }
        }
        g
    }

    fn insert(&self, k: u64, v: u64) -> Option<u64> {
        let key = k + 1;
        let top = self.random_level();
        let (mut preds, mut succs) = ([0; MAXH], [0; MAXH]);
        loop {
            if let Some(lf) = self.find(key, &mut preds, &mut succs) {
                let n = &self.nodes[succs[lf]];
                if !n.marked.load(Acquire) {
                    while !n.fully_linked.load(Acquire) {
                        std::hint::spin_loop();
                    }
                    // update in place under the node lock so it cannot race with a concurrent remove
                    let _g = n.lock.lock().unwrap();
                    if n.marked.load(Acquire) {
                        continue;
                    }
                    return Some(n.val.swap(v, AcqRel));
                }
                continue; // being removed: wait for it to disappear, then retry
            }
            let _guards = self.lock_preds(&preds, top);
            let valid = (0..=top).all(|l| {
                let (p, s) = (&self.nodes[preds[l]], &self.nodes[succs[l]]);
                !p.marked.load(Acquire)
                    && !s.marked.load(Acquire)
                    && p.next[l].load(Acquire) == succs[l]
            });
            if !valid {
                continue; // guards drop here; retry
            }
            let idx = self.used.fetch_add(1, Relaxed);
            assert!(
                idx < self.nodes.len(),
                "arena exhausted: this demo never recycles nodes"
            );
            let n = &self.nodes[idx];
            n.key.store(key, Relaxed);
            n.val.store(v, Relaxed);
            n.top.store(top, Relaxed);
            for l in 0..=top {
                n.next[l].store(succs[l], Relaxed);
            }
            for l in 0..=top {
                self.nodes[preds[l]].next[l].store(idx, Release); // publish bottom-up
            }
            n.fully_linked.store(true, Release);
            return None;
        }
    }

    fn remove(&self, k: u64) -> Option<u64> {
        let key = k + 1;
        let (mut preds, mut succs) = ([0; MAXH], [0; MAXH]);
        let mut victim: Option<(usize, usize, MutexGuard<'_, ()>)> = None; // (index, top, held lock) once WE marked it
        loop {
            let lf = self.find(key, &mut preds, &mut succs);
            if victim.is_none() {
                let lf = lf?;
                let idx = succs[lf];
                let n = &self.nodes[idx];
                let ok = n.fully_linked.load(Acquire)
                    && n.top.load(Acquire) == lf
                    && !n.marked.load(Acquire);
                if !ok {
                    return None; // not fully linked, or already being removed by someone else
                }
                let g = n.lock.lock().unwrap();
                if n.marked.load(Acquire) {
                    return None; // lost the race to another remover
                }
                n.marked.store(true, Release); // linearization point of a successful remove
                victim = Some((idx, lf, g));
            }
            let (idx, top, _vg) = victim.as_ref().map(|(i, t, g)| (*i, *t, g)).unwrap();
            let _guards = self.lock_preds(&preds, top);
            let valid = (0..=top).all(|l| {
                !self.nodes[preds[l]].marked.load(Acquire)
                    && self.nodes[preds[l]].next[l].load(Acquire) == idx
            });
            if !valid {
                continue; // keep the victim locked, search again
            }
            let n = &self.nodes[idx];
            let old = n.val.load(Acquire);
            for l in 0..=top {
                self.nodes[preds[l]].next[l].store(n.next[l].load(Acquire), Release);
            }
            return Some(old);
        }
    }

    fn get(&self, k: u64) -> Option<u64> {
        let (mut preds, mut succs) = ([0; MAXH], [0; MAXH]);
        let lf = self.find(k + 1, &mut preds, &mut succs)?;
        let n = &self.nodes[succs[lf]];
        (n.fully_linked.load(Acquire) && !n.marked.load(Acquire)).then(|| n.val.load(Acquire))
    }

    /// Weakly consistent scan of [lo, hi): strictly sorted, skips logically deleted nodes.
    fn range(&self, lo: u64, hi: u64) -> Vec<(u64, u64)> {
        let (mut preds, mut succs) = ([0; MAXH], [0; MAXH]);
        self.find(lo + 1, &mut preds, &mut succs);
        let mut cur = succs[0];
        let mut out = Vec::new();
        while cur != TAIL && self.key(cur) - 1 < hi {
            let n = &self.nodes[cur];
            if n.fully_linked.load(Acquire) && !n.marked.load(Acquire) {
                out.push((self.key(cur) - 1, n.val.load(Acquire)));
            }
            cur = self.next(cur, 0);
        }
        out
    }

    /// Quiescent-only structural check.
    fn check_structure(&self) -> usize {
        let mut bottom = Vec::new();
        let mut cur = self.next(HEAD, 0);
        while cur != TAIL {
            assert!(
                !self.nodes[cur].marked.load(Acquire),
                "marked node still linked at the bottom"
            );
            bottom.push(self.key(cur));
            cur = self.next(cur, 0);
        }
        assert!(
            bottom.windows(2).all(|w| w[0] < w[1]),
            "bottom level not strictly sorted"
        );
        let set: HashSet<u64> = bottom.iter().copied().collect();
        for l in 1..MAXH {
            let mut last = 0;
            let mut cur = self.next(HEAD, l);
            while cur != TAIL {
                let k = self.key(cur);
                assert!(k > last, "level {l} not sorted");
                assert!(
                    set.contains(&k),
                    "level {l} holds a key missing from the bottom level"
                );
                last = k;
                cur = self.next(cur, l);
            }
        }
        bottom.len()
    }
}

struct Rng(u64);
impl Rng {
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

// (1) disjoint key classes vs a BTreeMap model per thread
fn disjoint_workload() {
    const T: u64 = 8;
    let m = SkipMap::with_capacity(400_000);
    let models: Vec<BTreeMap<u64, u64>> = thread::scope(|s| {
        let hs: Vec<_> = (0..T)
            .map(|t| {
                let m = &m;
                s.spawn(move || {
                    let mut rng = Rng(t * 7919 + 13);
                    let mut model = BTreeMap::new();
                    for step in 0..20_000u64 {
                        let k = rng.below(1500) * T + t; // class t only
                        match rng.below(3) {
                            0 => assert_eq!(m.insert(k, step), model.insert(k, step)),
                            1 => assert_eq!(m.remove(k), model.remove(&k)),
                            _ => assert_eq!(m.get(k), model.get(&k).copied()),
                        }
                    }
                    model
                })
            })
            .collect();
        hs.into_iter().map(|h| h.join().unwrap()).collect()
    });
    let merged: BTreeMap<u64, u64> = models.into_iter().flatten().collect();
    let full = m.range(0, u64::MAX - 2);
    assert_eq!(
        full,
        merged.iter().map(|(a, b)| (*a, *b)).collect::<Vec<_>>()
    );
    for (lo, hi) in [(0, 100), (500, 5000), (11_000, 12_000), (3, 4)] {
        let want: Vec<_> = merged.range(lo..hi).map(|(a, b)| (*a, *b)).collect();
        assert_eq!(m.range(lo, hi), want, "range [{lo},{hi})");
    }
    let n = m.check_structure();
    assert_eq!(n, merged.len());
    println!("  disjoint: {n} live keys match the merged BTreeMap model; structure valid");
}

// (3) range scans during churn
fn scans_during_churn() {
    let m = SkipMap::with_capacity(400_000);
    let stable: Vec<u64> = (0..50).map(|i| i * 1000).collect();
    for &k in &stable {
        m.insert(k, k);
    }
    let stop = AtomicBool::new(false);
    let scans = AtomicUsize::new(0);
    thread::scope(|s| {
        for t in 0..4u64 {
            let (m, stop) = (&m, &stop);
            s.spawn(move || {
                let mut r = Rng(t + 99);
                for _ in 0..40_000 {
                    let k = r.below(50_000);
                    if k % 1000 == 0 {
                        continue; // never touch the stable keys
                    }
                    if r.below(2) == 0 {
                        m.insert(k, k);
                    } else {
                        m.remove(k);
                    }
                }
                let _ = stop;
            });
        }
        for _ in 0..3 {
            s.spawn(|| {
                while !stop.load(Relaxed) {
                    let out = m.range(0, 50_000);
                    assert!(
                        out.windows(2).all(|w| w[0].0 < w[1].0),
                        "scan not strictly sorted"
                    );
                    for &k in &stable {
                        assert!(
                            out.iter().any(|e| e.0 == k),
                            "stable key {k} missing from a scan"
                        );
                    }
                    scans.fetch_add(1, Relaxed);
                    if scans.load(Relaxed) > 200 {
                        break;
                    }
                }
            });
        }
        // writers finish on their own; scanners stop after 200 scans
    });
    stop.store(true, Relaxed);
    m.check_structure();
    println!(
        "  churn: {} concurrent scans saw every stable key, in order",
        scans.load(Relaxed)
    );
}

// (2) linearizability of overlapping keys
#[derive(Clone, Copy, Debug)]
enum Op {
    Insert(u64, u64),
    Get(u64),
    Remove(u64),
}
#[derive(Clone, Copy, Debug)]
struct Ev {
    inv: u64,
    ret: u64,
    op: Op,
    res: Option<u64>,
}
fn spec(mut s: [Option<u64>; 2], op: Op) -> ([Option<u64>; 2], Option<u64>) {
    match op {
        Op::Insert(k, v) => {
            let o = s[k as usize];
            s[k as usize] = Some(v);
            (s, o)
        }
        Op::Get(k) => (s, s[k as usize]),
        Op::Remove(k) => {
            let o = s[k as usize];
            s[k as usize] = None;
            (s, o)
        }
    }
}
fn linearizable(h: &[Ev]) -> bool {
    fn go(
        h: &[Ev],
        done: u32,
        st: [Option<u64>; 2],
        seen: &mut HashSet<(u32, [Option<u64>; 2])>,
    ) -> bool {
        if done == (1u32 << h.len()) - 1 {
            return true;
        }
        if !seen.insert((done, st)) {
            return false;
        }
        (0..h.len()).any(|i| {
            if done >> i & 1 == 1
                || !(0..h.len()).all(|j| j == i || done >> j & 1 == 1 || h[j].ret > h[i].inv)
            {
                return false;
            }
            let (ns, r) = spec(st, h[i].op);
            r == h[i].res && go(h, done | 1 << i, ns, seen)
        })
    }
    go(h, 0, [None; 2], &mut HashSet::new())
}
fn histories() {
    let mut worst = 0;
    for seed in 0..300u64 {
        let m = SkipMap::with_capacity(1000);
        let clock = AtomicU64::new(0);
        let mut all = Vec::new();
        thread::scope(|s| {
            let hs: Vec<_> = (0..3u64)
                .map(|t| {
                    let (m, clock) = (&m, &clock);
                    s.spawn(move || {
                        let mut r = Rng(seed * 17 + t + 1);
                        (0..4u64)
                            .map(|i| {
                                let k = r.below(2);
                                let op = match r.below(3) {
                                    0 => Op::Insert(k, t * 10 + i + 1),
                                    1 => Op::Get(k),
                                    _ => Op::Remove(k),
                                };
                                let inv = clock.fetch_add(1, SeqCst);
                                let res = match op {
                                    Op::Insert(k, v) => m.insert(k, v),
                                    Op::Get(k) => m.get(k),
                                    Op::Remove(k) => m.remove(k),
                                };
                                Ev {
                                    inv,
                                    ret: clock.fetch_add(1, SeqCst),
                                    op,
                                    res,
                                }
                            })
                            .collect::<Vec<_>>()
                    })
                })
                .collect();
            for h in hs {
                all.extend(h.join().unwrap());
            }
        });
        assert!(linearizable(&all), "non-linearizable history: {all:#?}");
        worst = worst.max(all.len());
    }
    println!(
        "  linearizability: 300 random 3-thread histories ({worst} ops each) all linearizable"
    );
}

fn throughput() {
    let m = SkipMap::with_capacity(2_000_000);
    for k in 0..10_000 {
        m.insert(k * 2, k);
    }
    let t0 = Instant::now();
    thread::scope(|s| {
        for t in 0..8u64 {
            let m = &m;
            s.spawn(move || {
                let mut r = Rng(t + 5);
                for _ in 0..100_000 {
                    let k = r.below(20_000);
                    match r.below(10) {
                        0 => drop(m.insert(k, k)),
                        1 => drop(m.remove(k)),
                        _ => drop(std::hint::black_box(m.get(k))),
                    }
                }
            });
        }
    });
    let sk = 800_000.0 / t0.elapsed().as_secs_f64() / 1e6;
    let bt = Mutex::new(BTreeMap::new());
    for k in 0..10_000u64 {
        bt.lock().unwrap().insert(k * 2, k);
    }
    let t0 = Instant::now();
    thread::scope(|s| {
        for t in 0..8u64 {
            let bt = &bt;
            s.spawn(move || {
                let mut r = Rng(t + 5);
                for _ in 0..100_000 {
                    let k = r.below(20_000);
                    let mut g = bt.lock().unwrap();
                    match r.below(10) {
                        0 => drop(g.insert(k, k)),
                        1 => drop(g.remove(&k)),
                        _ => drop(std::hint::black_box(g.get(&k).copied())),
                    }
                }
            });
        }
    });
    let bm = 800_000.0 / t0.elapsed().as_secs_f64() / 1e6;
    println!(
        "  throughput (8 threads, 80% get): lazy skip list {sk:.1} Mops/s vs Mutex<BTreeMap> {bm:.1} Mops/s (printed only)"
    );
}

fn main() {
    println!("== lazy skip list ==");
    let m = SkipMap::with_capacity(100);
    assert_eq!(m.insert(5, 50), None);
    assert_eq!(m.insert(5, 51), Some(50));
    assert_eq!(m.get(5), Some(51));
    assert_eq!(m.remove(5), Some(51));
    assert_eq!(m.remove(5), None);
    assert_eq!(m.get(5), None);
    for k in [9, 3, 7, 1] {
        m.insert(k, k * 10);
    }
    assert_eq!(m.range(2, 8), vec![(3, 30), (7, 70)]);
    assert_eq!(m.check_structure(), 4);
    disjoint_workload();
    histories();
    scans_during_churn();
    throughput();
    println!("all skip list checks passed");
}
