//! RECIPE 713 (trees): B+tree concurrency control: latch crabbing, optimistic crabbing, and optimistic lock coupling (OLC).
//! LEVEL: advanced
//! Use in: any shared ordered index; main-memory databases where reader-side latch writes (cache-line bouncing) hurt scaling.   Run: cargo run --release --example optimistic_lock_coupling_btree
#![allow(dead_code)]
//!
//! WHAT: the same B+tree map (u64 -> u64) behind four concurrency schemes, so the cost of each can be measured:
//!  (0) GLOBAL: one `RwLock<BTreeMap>` (baseline: readers share, any writer excludes everyone).
//!  (1) LATCH CRABBING (lock coupling), pessimistic: every node has its own `RwLock`. Descend taking the CHILD latch before
//!      releasing the parent; for writers, once the child is SAFE (not full, so it cannot split and the split can never reach
//!      the parent) release all ancestors. The root pointer has its own latch, held until a safe node is found.
//!  (2) OPTIMISTIC CRABBING (Bayer-Schkolnick): assume the leaf will not split: take READ latches down, a WRITE latch only on
//!      the leaf; if the leaf is full, release and RESTART pessimistically. Writers rarely touch inner-node latches.
//!  (3) OPTIMISTIC LOCK COUPLING (Leis et al., "The ART of Practical Synchronization"): each node carries a VERSION word
//!      (bit 0 = locked, upper bits = counter). A reader records the version, reads the node WITHOUT writing any shared memory,
//!      then re-reads the version; if it changed the read may be torn, so RESTART from the root. Writers take the lock by CAS on
//!      the version (try-lock only, never blocks, so deadlock is impossible: on failure restart), mutate, then bump the version
//!      on unlock. Splits happen EAGERLY on the way down (a full inner node is split before descending into it) so a leaf
//!      split only ever needs the parent, which is known not to be full. The root keeps node id 0 forever: a root split copies
//!      the two halves into NEW nodes and turns node 0 into a 1-separator inner node.
//! Node fields in the OLC tree are atomics (Relaxed loads/stores plus an Acquire fence before validation, Release on unlock),
//! which makes the optimistic read data-race-free in safe Rust: a torn read yields garbage numbers that validation rejects; the
//! code clamps counts and never trusts a child id before validating the parent's version. No `unsafe` anywhere in this file.
//! Deletes are lazy (no merge), as in the OLC paper's insert/lookup focus.
//! TESTS: disjoint-key workloads equal a sequential model (all schemes); per-key counters with no lost updates under contention;
//! 20 stress rounds with a watchdog that aborts on deadlock; stable keys are always found while splits run concurrently;
//! and for OLC, readers validate every leaf snapshot (sorted, increasing across the sibling chain) while writers split.
//! THROUGHPUT is printed (machine dependent, never asserted).
//! COMMON USES: OLC is used by main-memory index research systems (ART/B+tree in HyPer and Umbra); latch crabbing is the
//! classic scheme in disk-based B+tree implementations; PostgreSQL btree uses a related right-link (Lehman-Yao) protocol.
//! Course: CMU 15-445 Lecture 10 (Index Concurrency Control): latch crabbing, optimistic crabbing, leaf scans and deadlock avoidance.
//! Read: Leis, Haubenschild, Neumann, "Optimistic Lock Coupling": https://db.in.tum.de/~leis/papers/artsync.pdf
//! Read: Wikipedia, B+ tree: https://en.wikipedia.org/wiki/B%2B_tree
use std::collections::BTreeMap;
use std::sync::atomic::{AtomicBool, AtomicU32, AtomicU64, AtomicUsize, Ordering::*, fence};
use std::sync::{RwLock, RwLockWriteGuard};
use std::time::{Duration, Instant};

const CAP: usize = 32; // max keys per node
const NODES: usize = 1 << 14;
const NIL32: u32 = u32::MAX;
const NIL: usize = usize::MAX;

struct Rng(u64);
impl Rng {
    fn next(&mut self) -> u64 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        self.0
    }
    fn below(&mut self, n: u64) -> u64 {
        self.next() % n
    }
}

/// The interface all four schemes implement. `upsert_with` runs `f` exactly once, under the leaf's exclusive protection.
trait Index: Sync {
    fn name(&self) -> &'static str;
    fn get(&self, key: u64) -> Option<u64>;
    fn upsert_with(&self, key: u64, f: &dyn Fn(Option<u64>) -> u64) -> Option<u64>;
    fn remove(&self, key: u64) -> Option<u64>;
    /// Quiescent ordered dump (walks the leaf chain).
    fn dump(&self) -> Vec<(u64, u64)>;
    fn insert(&self, key: u64, val: u64) -> Option<u64> {
        self.upsert_with(key, &|_| val)
    }
}

// ---- (0) global lock ------------------------------------------------------------------------------------------------
struct Global(RwLock<BTreeMap<u64, u64>>);
impl Index for Global {
    fn name(&self) -> &'static str {
        "global RwLock"
    }
    fn get(&self, key: u64) -> Option<u64> {
        self.0.read().unwrap().get(&key).copied()
    }
    fn upsert_with(&self, key: u64, f: &dyn Fn(Option<u64>) -> u64) -> Option<u64> {
        let mut m = self.0.write().unwrap();
        let old = m.get(&key).copied();
        m.insert(key, f(old));
        old
    }
    fn remove(&self, key: u64) -> Option<u64> {
        self.0.write().unwrap().remove(&key)
    }
    fn dump(&self) -> Vec<(u64, u64)> {
        self.0
            .read()
            .unwrap()
            .iter()
            .map(|(k, v)| (*k, *v))
            .collect()
    }
}

// ---- (1)+(2) latch crabbing -----------------------------------------------------------------------------------------
struct CNode {
    level: u32,
    keys: Vec<u64>,
    vals: Vec<u64>, // leaf: values; inner: child ids (keys.len() + 1 of them)
    next: usize,
}

struct Crab {
    nodes: Vec<RwLock<CNode>>,
    root: RwLock<usize>, // the "root latch": protects the root pointer itself
    next_id: AtomicUsize,
    optimistic: bool,
    fallbacks: AtomicUsize,
}

impl Crab {
    fn new(optimistic: bool) -> Crab {
        let nodes = (0..NODES)
            .map(|_| {
                RwLock::new(CNode {
                    level: 0,
                    keys: vec![],
                    vals: vec![],
                    next: NIL,
                })
            })
            .collect();
        Crab {
            nodes,
            root: RwLock::new(0),
            next_id: AtomicUsize::new(1),
            optimistic,
            fallbacks: AtomicUsize::new(0),
        }
    }
    fn alloc(&self) -> usize {
        let id = self.next_id.fetch_add(1, Relaxed);
        assert!(id < NODES, "node arena exhausted");
        id
    }
    fn route(n: &CNode, key: u64) -> usize {
        n.vals[n.keys.partition_point(|&k| k <= key)] as usize
    }
    /// Split an over-full node in place; returns (separator, id of the new right node).
    fn split_node(&self, node: &mut CNode) -> (u64, usize) {
        let mid = node.keys.len() / 2;
        let nid = self.alloc();
        let (sep, right) = if node.level == 0 {
            let (rk, rv) = (node.keys.split_off(mid), node.vals.split_off(mid));
            (
                rk[0],
                CNode {
                    level: 0,
                    keys: rk,
                    vals: rv,
                    next: node.next,
                },
            )
        } else {
            let mut rk = node.keys.split_off(mid);
            let sep = rk.remove(0); // the middle key moves UP, it is not copied
            (
                sep,
                CNode {
                    level: node.level,
                    keys: rk,
                    vals: node.vals.split_off(mid + 1),
                    next: NIL,
                },
            )
        };
        if node.level == 0 {
            node.next = nid;
        }
        *self.nodes[nid].write().unwrap() = right; // unreachable until the parent links it, so this lock is uncontended
        (sep, nid)
    }

    /// Write-latch the leaf for `key`, holding READ latches only above it. The parent's read latch is held while the leaf
    /// latch is taken, so the leaf cannot split away from us in between.
    fn write_leaf(&self, key: u64) -> RwLockWriteGuard<'_, CNode> {
        let rg = self.root.read().unwrap();
        let mut g = self.nodes[*rg].read().unwrap();
        if g.level == 0 {
            let id = *rg;
            drop(g);
            let w = self.nodes[id].write().unwrap(); // root id cannot change while `rg` is held
            drop(rg);
            return w;
        }
        drop(rg);
        loop {
            let c = Self::route(&g, key);
            if g.level == 1 {
                return self.nodes[c].write().unwrap();
            }
            let cg = self.nodes[c].read().unwrap();
            g = cg; // child latch acquired BEFORE the parent latch is released by this assignment
        }
    }

    fn pessimistic(&self, key: u64, f: &dyn Fn(Option<u64>) -> u64) -> Option<u64> {
        let mut root_g = Some(self.root.write().unwrap());
        let mut id = **root_g.as_ref().unwrap();
        let mut path: Vec<(usize, RwLockWriteGuard<'_, CNode>)> = vec![];
        loop {
            let g = self.nodes[id].write().unwrap();
            if g.keys.len() < CAP {
                path.clear(); // SAFE node: it cannot overflow, so no ancestor will be modified: release them all
                root_g = None;
            }
            let leaf = g.level == 0;
            let next = if leaf { 0 } else { Self::route(&g, key) };
            path.push((id, g));
            if leaf {
                break;
            }
            id = next;
        }
        let leaf = &mut path.last_mut().unwrap().1;
        match leaf.keys.binary_search(&key) {
            Ok(p) => {
                let old = leaf.vals[p];
                leaf.vals[p] = f(Some(old));
                return Some(old);
            }
            Err(p) => {
                leaf.keys.insert(p, key);
                leaf.vals.insert(p, f(None));
            }
        }
        let mut idx = path.len() - 1;
        while path[idx].1.keys.len() > CAP {
            let (sep, nid) = self.split_node(&mut path[idx].1);
            if idx == 0 {
                let (old_id, lvl) = (path[0].0, path[0].1.level + 1);
                let rid = self.alloc();
                *self.nodes[rid].write().unwrap() = CNode {
                    level: lvl,
                    keys: vec![sep],
                    vals: vec![old_id as u64, nid as u64],
                    next: NIL,
                };
                **root_g
                    .as_mut()
                    .expect("an unsafe root keeps the root latch") = rid;
                break;
            }
            idx -= 1;
            let p = &mut path[idx].1;
            let pos = p.keys.partition_point(|&k| k <= sep);
            p.keys.insert(pos, sep);
            p.vals.insert(pos + 1, nid as u64);
        }
        None
    }

    /// Returns None when the leaf is full (needs a split): the caller restarts pessimistically.
    fn optimistic_try(&self, key: u64, f: &dyn Fn(Option<u64>) -> u64) -> Option<Option<u64>> {
        let mut leaf = self.write_leaf(key);
        match leaf.keys.binary_search(&key) {
            Ok(p) => {
                let old = leaf.vals[p];
                leaf.vals[p] = f(Some(old));
                Some(Some(old))
            }
            Err(p) if leaf.keys.len() < CAP => {
                leaf.keys.insert(p, key);
                leaf.vals.insert(p, f(None));
                Some(None)
            }
            Err(_) => None,
        }
    }
}

impl Index for Crab {
    fn name(&self) -> &'static str {
        if self.optimistic {
            "crabbing, optimistic"
        } else {
            "crabbing, pessimistic"
        }
    }
    fn get(&self, key: u64) -> Option<u64> {
        let rg = self.root.read().unwrap();
        let mut g = self.nodes[*rg].read().unwrap();
        drop(rg);
        loop {
            if g.level == 0 {
                return g.keys.binary_search(&key).ok().map(|p| g.vals[p]);
            }
            let cg = self.nodes[Self::route(&g, key)].read().unwrap();
            g = cg;
        }
    }
    fn upsert_with(&self, key: u64, f: &dyn Fn(Option<u64>) -> u64) -> Option<u64> {
        if self.optimistic {
            if let Some(r) = self.optimistic_try(key, f) {
                return r;
            }
            self.fallbacks.fetch_add(1, Relaxed);
        }
        self.pessimistic(key, f)
    }
    fn remove(&self, key: u64) -> Option<u64> {
        let mut leaf = self.write_leaf(key);
        let p = leaf.keys.binary_search(&key).ok()?;
        leaf.keys.remove(p);
        Some(leaf.vals.remove(p))
    }
    fn dump(&self) -> Vec<(u64, u64)> {
        let mut id = *self.root.read().unwrap();
        loop {
            let g = self.nodes[id].read().unwrap();
            if g.level == 0 {
                break;
            }
            id = g.vals[0] as usize;
        }
        let mut out = vec![];
        while id != NIL {
            let g = self.nodes[id].read().unwrap();
            out.extend(g.keys.iter().copied().zip(g.vals.iter().copied()));
            id = g.next;
        }
        out
    }
}

// ---- (3) optimistic lock coupling ---------------------------------------------------------------------------------------
struct ONode {
    version: AtomicU64, // bit 0 = write-locked; the rest is a change counter
    level: AtomicU32,   // 0 = leaf
    count: AtomicU32,
    next: AtomicU32,
    keys: [AtomicU64; CAP],
    vals: [AtomicU64; CAP + 1],
}
impl ONode {
    fn new() -> ONode {
        ONode {
            version: AtomicU64::new(0),
            level: AtomicU32::new(0),
            count: AtomicU32::new(0),
            next: AtomicU32::new(NIL32),
            keys: std::array::from_fn(|_| AtomicU64::new(0)),
            vals: std::array::from_fn(|_| AtomicU64::new(0)),
        }
    }
    /// Clamped: a torn read may see garbage, and garbage must never index out of bounds.
    fn count(&self) -> usize {
        (self.count.load(Relaxed) as usize).min(CAP)
    }
    fn key(&self, i: usize) -> u64 {
        self.keys[i].load(Relaxed)
    }
    fn val(&self, i: usize) -> u64 {
        self.vals[i].load(Relaxed)
    }
    fn set(&self, i: usize, k: u64, v: u64) {
        self.keys[i].store(k, Relaxed);
        self.vals[i].store(v, Relaxed);
    }
    /// Number of keys <= `key` (inner routing) among the first `n`.
    fn upper(&self, n: usize, key: u64) -> usize {
        (0..n).take_while(|&i| self.key(i) <= key).count()
    }
}

struct Olc {
    nodes: Vec<ONode>,
    next_id: AtomicUsize,
    restarts: AtomicUsize,
}

impl Olc {
    fn new() -> Olc {
        Olc {
            nodes: (0..NODES).map(|_| ONode::new()).collect(),
            next_id: AtomicUsize::new(1),
            restarts: AtomicUsize::new(0),
        }
    }
    fn alloc(&self) -> usize {
        let id = self.next_id.fetch_add(1, Relaxed);
        assert!(id < NODES, "node arena exhausted");
        id
    }
    /// Start an optimistic read: None (restart) if a writer holds the node.
    fn read_lock(&self, id: usize) -> Option<u64> {
        let v = self.nodes[id].version.load(Acquire);
        (v & 1 == 0).then_some(v)
    }
    /// Everything read since `read_lock` is trustworthy iff the version did not change.
    fn validate(&self, id: usize, v: u64) -> bool {
        fence(Acquire); // order the preceding Relaxed data loads before this version re-read
        self.nodes[id].version.load(Relaxed) == v
    }
    /// Try to turn the optimistic read into an exclusive lock; success also proves nothing changed since `v`.
    fn upgrade(&self, id: usize, v: u64) -> bool {
        self.nodes[id]
            .version
            .compare_exchange(v, v | 1, Acquire, Relaxed)
            .is_ok()
    }
    fn unlock(&self, id: usize) {
        self.nodes[id].version.fetch_add(1, Release); // clears the lock bit AND bumps the counter
    }
    fn restart(&self) {
        // A try-lock holder may be descheduled (oversubscribed machine): yield now and then instead of burning the whole quantum.
        if self.restarts.fetch_add(1, Relaxed) % 8 == 7 {
            std::thread::yield_now();
        } else {
            std::hint::spin_loop();
        }
    }

    /// Descend to the leaf for `key`; None means "restart".
    fn find_leaf(&self, key: u64) -> Option<(usize, u64)> {
        let (mut id, mut v) = (0usize, self.read_lock(0)?);
        loop {
            let n = &self.nodes[id];
            if n.level.load(Relaxed) == 0 {
                return Some((id, v));
            }
            let child = n.val(n.upper(n.count(), key)) as usize;
            if !self.validate(id, v) {
                return None; // the child id may be garbage: never use it before validating its parent
            }
            let cv = self.read_lock(child)?;
            if !self.validate(id, v) {
                return None; // lock coupling: parent re-checked AFTER the child's version was taken
            }
            id = child;
            v = cv;
        }
    }

    /// Split a full node; parent (if any) must be non-full. Returns false (and holds no locks) if a CAS failed.
    fn split_full(&self, id: usize, v: u64, parent: Option<(usize, u64)>) -> bool {
        match parent {
            Some((pid, pv)) => {
                if !self.upgrade(pid, pv) {
                    return false;
                }
                if !self.upgrade(id, v) {
                    self.unlock(pid);
                    return false;
                }
                self.split_child(id, pid);
                self.unlock(id);
                self.unlock(pid);
            }
            None => {
                if !self.upgrade(id, v) {
                    return false;
                }
                self.split_root(id);
                self.unlock(id);
            }
        }
        true
    }

    /// Move entries `from..` of `src` into the fresh node `dst`; for inner nodes the middle key moves up instead of over.
    fn move_half(&self, src: &ONode, dst: &ONode, mid: usize, cnt: usize, leaf: bool) -> u64 {
        if leaf {
            for i in mid..cnt {
                dst.set(i - mid, src.key(i), src.val(i));
            }
            dst.count.store((cnt - mid) as u32, Relaxed);
            dst.next.store(src.next.load(Relaxed), Relaxed);
            src.count.store(mid as u32, Relaxed);
            dst.key(0)
        } else {
            let sep = src.key(mid);
            for i in mid + 1..cnt {
                dst.set(i - mid - 1, src.key(i), src.val(i));
            }
            dst.vals[cnt - mid - 1].store(src.val(cnt), Relaxed);
            dst.count.store((cnt - mid - 1) as u32, Relaxed);
            src.count.store(mid as u32, Relaxed);
            sep
        }
    }

    fn split_child(&self, id: usize, pid: usize) {
        let (n, p) = (&self.nodes[id], &self.nodes[pid]);
        let (cnt, leaf) = (n.count(), n.level.load(Relaxed) == 0);
        let nid = self.alloc();
        let r = &self.nodes[nid];
        r.level.store(n.level.load(Relaxed), Relaxed);
        let sep = self.move_half(n, r, cnt / 2, cnt, leaf);
        if leaf {
            n.next.store(nid as u32, Relaxed);
        }
        let pc = p.count();
        let pos = p.upper(pc, sep);
        for i in (pos..pc).rev() {
            p.keys[i + 1].store(p.key(i), Relaxed);
        }
        for i in (pos + 1..=pc).rev() {
            p.vals[i + 1].store(p.val(i), Relaxed);
        }
        p.keys[pos].store(sep, Relaxed);
        p.vals[pos + 1].store(nid as u64, Relaxed);
        p.count.store(pc as u32 + 1, Relaxed);
    }

    /// The root keeps id 0: copy both halves into new nodes and make node 0 a one-separator inner node.
    fn split_root(&self, id: usize) {
        let n = &self.nodes[id];
        let (cnt, leaf, level) = (n.count(), n.level.load(Relaxed) == 0, n.level.load(Relaxed));
        let (l, r) = (self.alloc(), self.alloc());
        let (ln, rn) = (&self.nodes[l], &self.nodes[r]);
        for x in [ln, rn] {
            x.level.store(level, Relaxed);
        }
        // Copy the whole root into `ln`, then split `ln` into `ln` + `rn`.
        for i in 0..cnt {
            ln.set(i, n.key(i), n.val(i));
        }
        ln.vals[cnt].store(n.val(cnt), Relaxed);
        ln.count.store(cnt as u32, Relaxed);
        ln.next.store(NIL32, Relaxed);
        let sep = self.move_half(ln, rn, cnt / 2, cnt, leaf);
        if leaf {
            ln.next.store(r as u32, Relaxed);
        }
        n.level.store(level + 1, Relaxed);
        n.set(0, sep, l as u64);
        n.vals[1].store(r as u64, Relaxed);
        n.count.store(1, Relaxed);
        n.next.store(NIL32, Relaxed);
    }

    /// Validated snapshot walk over the leaf chain for the torn-read test: each snapshot must be strictly increasing,
    /// and keys must increase across leaves. Returns the number of keys seen.
    fn scan_validated(&self, start: u64, max_leaves: usize) -> usize {
        let mut seen = 0;
        let mut last: Option<u64> = None;
        let mut id = loop {
            match self.find_leaf(start) {
                Some((id, _)) => break id,
                None => self.restart(),
            }
        };
        for _ in 0..max_leaves {
            let (keys, next) = loop {
                let Some(v) = self.read_lock(id) else {
                    self.restart();
                    continue;
                };
                let n = &self.nodes[id];
                let keys: Vec<u64> = (0..n.count()).map(|i| n.key(i)).collect();
                let next = n.next.load(Relaxed);
                if self.validate(id, v) {
                    break (keys, next);
                }
                self.restart();
            };
            assert!(
                keys.windows(2).all(|w| w[0] < w[1]),
                "validated snapshot must be sorted: torn read escaped validation"
            );
            if let (Some(l), Some(f)) = (last, keys.first()) {
                assert!(l < *f, "keys must increase across the sibling chain");
            }
            if let Some(k) = keys.last() {
                last = Some(*k);
            }
            seen += keys.len();
            if next == NIL32 {
                break;
            }
            id = next as usize;
        }
        seen
    }
}

impl Index for Olc {
    fn name(&self) -> &'static str {
        "optimistic lock coupling"
    }
    fn get(&self, key: u64) -> Option<u64> {
        loop {
            if let Some((id, v)) = self.find_leaf(key) {
                let n = &self.nodes[id];
                let cnt = n.count();
                let pos = (0..cnt).find(|&i| n.key(i) == key);
                let res = pos.map(|i| n.val(i));
                if self.validate(id, v) {
                    return res;
                }
            }
            self.restart();
        }
    }
    fn upsert_with(&self, key: u64, f: &dyn Fn(Option<u64>) -> u64) -> Option<u64> {
        loop {
            let mut parent: Option<(usize, u64)> = None;
            let Some(mut v) = self.read_lock(0) else {
                self.restart();
                continue;
            };
            let mut id = 0usize;
            'descend: loop {
                let n = &self.nodes[id];
                let cnt = n.count();
                if n.level.load(Relaxed) > 0 {
                    if cnt == CAP {
                        // Eager split: a full inner node is split BEFORE descending, so the parent of any split is never full.
                        self.split_full(id, v, parent);
                        break 'descend;
                    }
                    let child = n.val(n.upper(cnt, key)) as usize;
                    if !self.validate(id, v) {
                        break 'descend;
                    }
                    let Some(cv) = self.read_lock(child) else {
                        break 'descend;
                    };
                    if !self.validate(id, v) {
                        break 'descend;
                    }
                    parent = Some((id, v));
                    id = child;
                    v = cv;
                    continue;
                }
                let pos = (0..cnt).find(|&i| n.key(i) >= key).unwrap_or(cnt);
                let found = pos < cnt && n.key(pos) == key;
                if !found && cnt == CAP {
                    self.split_full(id, v, parent);
                    break 'descend;
                }
                if !self.upgrade(id, v) {
                    break 'descend; // another writer got there first; success of the CAS would prove our reads were consistent
                }
                let old = if found {
                    let old = n.val(pos);
                    n.vals[pos].store(f(Some(old)), Relaxed);
                    Some(old)
                } else {
                    for i in (pos..cnt).rev() {
                        n.set(i + 1, n.key(i), n.val(i));
                    }
                    n.set(pos, key, f(None));
                    n.count.store(cnt as u32 + 1, Relaxed);
                    None
                };
                self.unlock(id);
                return old;
            }
            self.restart();
        }
    }
    fn remove(&self, key: u64) -> Option<u64> {
        loop {
            if let Some((id, v)) = self.find_leaf(key) {
                let n = &self.nodes[id];
                let cnt = n.count();
                let pos = (0..cnt).find(|&i| n.key(i) == key);
                if pos.is_none() && self.validate(id, v) {
                    return None;
                }
                if self.upgrade(id, v) {
                    let Some(pos) = pos else {
                        self.unlock(id);
                        return None;
                    };
                    let old = n.val(pos);
                    for i in pos + 1..cnt {
                        n.set(i - 1, n.key(i), n.val(i));
                    }
                    n.count.store(cnt as u32 - 1, Relaxed);
                    self.unlock(id);
                    return Some(old);
                }
            }
            self.restart();
        }
    }
    fn dump(&self) -> Vec<(u64, u64)> {
        let mut id = 0;
        while self.nodes[id].level.load(Relaxed) > 0 {
            id = self.nodes[id].val(0) as usize;
        }
        let mut out = vec![];
        loop {
            let n = &self.nodes[id];
            out.extend((0..n.count()).map(|i| (n.key(i), n.val(i))));
            match n.next.load(Relaxed) {
                NIL32 => return out,
                x => id = x as usize,
            }
        }
    }
}

// ---- tests --------------------------------------------------------------------------------------------------------------
fn make_all() -> Vec<Box<dyn Index>> {
    vec![
        Box::new(Global(RwLock::new(BTreeMap::new()))),
        Box::new(Crab::new(false)),
        Box::new(Crab::new(true)),
        Box::new(Olc::new()),
    ]
}

const T: usize = 8;

fn disjoint_model(ix: &dyn Index) {
    let models: Vec<BTreeMap<u64, u64>> = std::thread::scope(|s| {
        let hs: Vec<_> = (0..T)
            .map(|tid| {
                s.spawn(move || {
                    let (mut rng, mut m) = (Rng(31 + tid as u64), BTreeMap::new());
                    for i in 0..15_000u64 {
                        let k = rng.below(4000) * T as u64 + tid as u64; // private residue class
                        match rng.below(100) {
                            0..=54 => assert_eq!(ix.insert(k, i), m.insert(k, i)),
                            55..=74 => assert_eq!(ix.remove(k), m.remove(&k)),
                            _ => assert_eq!(ix.get(k), m.get(&k).copied()),
                        }
                    }
                    m
                })
            })
            .collect();
        hs.into_iter().map(|h| h.join().unwrap()).collect()
    });
    let want: Vec<(u64, u64)> = models
        .into_iter()
        .flatten()
        .collect::<BTreeMap<_, _>>()
        .into_iter()
        .collect();
    assert_eq!(
        ix.dump(),
        want,
        "{}: contents differ from the sequential model",
        ix.name()
    );
    for (k, v) in want.iter().step_by(11) {
        assert_eq!(ix.get(*k), Some(*v));
    }
}

fn counters_no_lost_updates(ix: &dyn Index) {
    const KEYS: u64 = 400;
    const INCS: usize = 20_000;
    let tallies: Vec<Vec<u64>> = std::thread::scope(|s| {
        let hs: Vec<_> = (0..T)
            .map(|tid| {
                s.spawn(move || {
                    let (mut rng, mut tally) = (Rng(99 + tid as u64), vec![0u64; KEYS as usize]);
                    for _ in 0..INCS {
                        let k = rng.below(KEYS);
                        ix.upsert_with(k, &|o| o.unwrap_or(0) + 1);
                        tally[k as usize] += 1;
                    }
                    tally
                })
            })
            .collect();
        hs.into_iter().map(|h| h.join().unwrap()).collect()
    });
    let mut total = 0;
    for k in 0..KEYS {
        let want: u64 = tallies.iter().map(|t| t[k as usize]).sum();
        assert_eq!(
            ix.get(k).unwrap_or(0),
            want,
            "{}: lost update on key {k}",
            ix.name()
        );
        total += want;
    }
    assert_eq!(total, (T * INCS) as u64);
}

/// Mixed ops, many rounds, with a watchdog: a deadlock would hang forever, so the watchdog aborts the process instead.
fn stress_rounds(mk: &dyn Fn() -> Box<dyn Index>, rounds: usize) {
    let done = AtomicBool::new(false);
    let progress = AtomicUsize::new(0);
    std::thread::scope(|s| {
        s.spawn(|| {
            // Progress-based, not wall-clock: a slow run on an oversubscribed machine keeps finishing operations and is left
            // alone; only 60 s with ZERO finished operations (a real deadlock/livelock) aborts with a diagnostic.
            let (mut last, mut stalled) = (usize::MAX, 0u32);
            while !done.load(Acquire) {
                let now = progress.load(Relaxed);
                stalled = if now == last { stalled + 1 } else { 0 };
                last = now;
                if stalled >= 3000 {
                    eprintln!("WATCHDOG: no operation finished for 60 s ({now} done): deadlock or livelock suspected, aborting");
                    std::process::exit(2);
                }
                std::thread::sleep(Duration::from_millis(20));
            }
        });
        for round in 0..rounds {
            let ix = mk();
            std::thread::scope(|s2| {
                for tid in 0..T {
                    let (ix, progress) = (&*ix, &progress);
                    s2.spawn(move || {
                        let mut rng = Rng(round as u64 * 1009 + tid as u64 + 1);
                        for i in 0..4_000u64 {
                            let k = rng.below(1200);
                            match rng.below(10) {
                                0..=4 => {
                                    ix.upsert_with(k, &|o| o.unwrap_or(0) + 1);
                                }
                                5..=6 => {
                                    ix.remove(k);
                                }
                                _ => {
                                    ix.get(k);
                                }
                            }
                            if i % 64 == 0 {
                                progress.fetch_add(64, Relaxed);
                            }
                        }
                    });
                }
            });
            let d = ix.dump();
            assert!(
                d.windows(2).all(|w| w[0].0 < w[1].0),
                "leaf chain must stay strictly ordered"
            );
        }
        done.store(true, Release);
    });
}

/// Stable keys (multiples of 3) are inserted first; writers then insert the other keys, forcing many splits, while readers
/// must ALWAYS find every stable key. For OLC the readers also validate whole leaf snapshots.
fn stable_keys_always_found(ix: &dyn Index, olc: Option<&Olc>) {
    const STABLE: u64 = 3000;
    for k in 0..STABLE {
        ix.insert(k * 3, k);
    }
    let stop = AtomicBool::new(false);
    let reads = AtomicUsize::new(0);
    std::thread::scope(|s| {
        for tid in 0..4u64 {
            let (stop, reads) = (&stop, &reads);
            s.spawn(move || {
                let mut rng = Rng(7 + tid);
                while !stop.load(Acquire) {
                    // Readers never block, so on an oversubscribed machine they would burn whole quanta (and, with an
                    // RwLock-based index, can starve the writers they are waiting for): give the CPU back each iteration.
                    std::thread::yield_now();
                    let k = rng.below(STABLE);
                    assert_eq!(
                        ix.get(k * 3),
                        Some(k),
                        "{}: stable key vanished during a split",
                        ix.name()
                    );
                    if let Some(o) = olc {
                        o.scan_validated(rng.below(STABLE * 3), 6);
                    }
                    reads.fetch_add(1, Relaxed);
                }
            });
        }
        let writers: Vec<_> = (0..4u64)
            .map(|tid| {
                s.spawn(move || {
                    let mut rng = Rng(555 + tid);
                    for _ in 0..30_000 {
                        let k = rng.below(STABLE) * 3 + 1 + rng.below(2);
                        ix.insert(k, k);
                    }
                })
            })
            .collect();
        for w in writers {
            w.join().unwrap();
        }
        stop.store(true, Release);
    });
    assert!(reads.load(Relaxed) > 0);
    let d = ix.dump();
    assert!(d.windows(2).all(|w| w[0].0 < w[1].0));
    assert!(d.len() as u64 >= STABLE);
}

fn throughput() {
    println!(
        "throughput, Mops/s (400k ops per cell, 50k keys prefilled in 0..100k; machine dependent):"
    );
    println!(
        "  {:<24} {:>26} {:>26}",
        "scheme", "read-heavy 95% get  1/2/4/8 thr", "write-heavy 80% upsert 1/2/4/8 thr"
    );
    for which in 0..4 {
        let mut cells = [String::new(), String::new()];
        for (m, &write_pct) in [5u64, 80].iter().enumerate() {
            for &th in &[1usize, 2, 4, 8] {
                let ix: Box<dyn Index> = make_all().remove(which);
                let mut rng = Rng(1);
                for _ in 0..50_000 {
                    let k = rng.below(100_000);
                    ix.insert(k, k);
                }
                let per = 400_000 / th;
                let t0 = Instant::now();
                std::thread::scope(|s| {
                    for tid in 0..th {
                        let ix = &*ix;
                        s.spawn(move || {
                            let mut rng = Rng(100 + tid as u64);
                            for _ in 0..per {
                                let k = rng.below(100_000);
                                if rng.below(100) < write_pct {
                                    ix.upsert_with(k, &|o| o.unwrap_or(0) + 1);
                                } else {
                                    std::hint::black_box(ix.get(k));
                                }
                            }
                        });
                    }
                });
                let mops = (per * th) as f64 / t0.elapsed().as_secs_f64() / 1e6;
                cells[m].push_str(&format!("{mops:>6.2}"));
            }
        }
        println!(
            "  {:<24} {:>26} {:>26}",
            make_all()[which].name(),
            cells[0],
            cells[1]
        );
    }
}

fn main() {
    let t_start = Instant::now();
    println!("B+tree concurrency control, CAP={CAP} keys/node, {T} threads");
    for ix in make_all() {
        disjoint_model(&*ix);
        println!(
            "  {:<24} disjoint-key workload equals the sequential model ({} keys)",
            ix.name(),
            ix.dump().len()
        );
    }
    for ix in make_all() {
        counters_no_lost_updates(&*ix);
        println!(
            "  {:<24} {} contended increments, none lost",
            ix.name(),
            T * 20_000
        );
    }
    let makers: Vec<(&str, Box<dyn Fn() -> Box<dyn Index>>)> = vec![
        (
            "crabbing, pessimistic",
            Box::new(|| Box::new(Crab::new(false))),
        ),
        (
            "crabbing, optimistic",
            Box::new(|| Box::new(Crab::new(true))),
        ),
        (
            "optimistic lock coupling",
            Box::new(|| Box::new(Olc::new())),
        ),
    ];
    for (name, mk) in &makers {
        stress_rounds(mk.as_ref(), 20);
        println!("  {name:<24} 20 stress rounds finished, no deadlock");
    }
    for ix in [
        Box::new(Crab::new(false)) as Box<dyn Index>,
        Box::new(Crab::new(true)),
    ] {
        stable_keys_always_found(&*ix, None);
    }
    let olc = Olc::new();
    stable_keys_always_found(&olc, Some(&olc));
    println!(
        "  stable keys always found during concurrent splits; OLC snapshots validated ({} restarts so far)",
        olc.restarts.load(Relaxed)
    );
    println!("  correctness phases took {:?}", t_start.elapsed());
    throughput();
    println!("all assertions passed ({:?} total)", t_start.elapsed());
}
