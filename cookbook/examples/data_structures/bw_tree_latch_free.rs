//! RECIPE 712 (trees): a Bw-tree style latch-free B+tree: mapping table, delta chains, consolidation, split deltas, epochs.
//! LEVEL: advanced
//! Use in: latch-free ordered indexes for multicore main-memory and flash storage engines.   Run: cargo run --release --example bw_tree_latch_free
#![allow(dead_code)]
//!
//! WHAT: a B+tree where NO thread ever takes a latch and pages are never updated in place. Every page is named by a page id (PID);
//! a MAPPING TABLE maps PID -> pointer to the head of that page's chain. A page is an immutable BASE (sorted entries) with a
//! chain of immutable DELTA records prepended on top: `Ins(k,v)` (insert or update), `Del(k)`, and `Split(sep, right)`.
//! A write allocates a delta whose `next` is the head it saw and installs it with ONE compare-and-swap on the table slot; if the
//! CAS fails somebody else changed the page, so rebuild the delta against the new head. The CAS is the linearization point.
//! Because parents refer to children by PID, a page can be replaced wholesale (consolidated) with a single CAS, and no parent
//! pointer ever needs updating.
//! CONSOLIDATION: when a chain reaches CONSOL deltas, a thread merges base+deltas into a fresh base and CASes it in; the old
//! chain is retired. SPLIT is two steps: (1) if the merged page is too big, the same CAS installs `Split(sep,right)` over
//! the new left half instead (its `high` becomes `sep`, and keys >= sep are redirected through the `right` sibling link,
//! so the tree is already correct); (2) post an index entry `Ins(sep,right)`
//! on the PARENT. Any thread that sees a Split delta without a parent entry HELPS finish step 2 ("help-along"), so a stalled
//! splitter never blocks anyone. Root split installs a new root with a CAS on the root pointer. Inner pages use the same
//! records: an inner base is `(separator, child PID)` pairs and routing is "largest separator <= key".
//! RECLAMATION: readers hold raw pointers into old chains, so a retired chain may be freed only when no reader can still see it.
//! A simplified epoch scheme: a global epoch, a per-thread pinned-epoch slot, per-thread garbage tagged with the retire epoch,
//! freed once the global epoch is >= tag + 2. (Alloc/free counters are checked to match at shutdown: nothing leaks or double frees.)
//! Unsafe is confined to dereferencing record pointers; SAFETY comments say why each is protected by the epoch pin.
//! Verified by: a sequential 20k-op differential test vs `BTreeMap`; 8 threads x 25k ops on DISJOINT keys (every result checked
//! against a per-thread model, final contents equal the merged model); 8 threads x 25k ops on OVERLAPPING hot keys where every
//! insert writes a unique token and the per-key log is checked for linearizability (each token consumed at most once, None-states
//! balance, final state consistent); structural checks of the leaf level (ordered, contiguous [low, high) ranges).
//! LIVENESS vs PROGRESS (a real bug this file had): "some CAS always succeeds" (lock-free) does not mean the tree stays
//! usable. A writer needs one cheap CAS; a consolidator must first materialize the WHOLE chain. So on a busy page the
//! consolidator loses, the chain gets longer, consolidating gets slower, and it loses even more surely. Chains reached tens
//! of thousands of records; `materialize` (one `Vec::insert` per delta) became quadratic and a sub-second run could take
//! minutes while every thread burned CPU. Backoff on CAS losers did not stop it, and neither could a "no progress" watchdog,
//! since operations still finished, just very slowly. Fix: (1) MAX_CHAIN: nobody appends to a full chain; they consolidate it first,
//! so its head is frozen against everything but consolidation and one consolidator must win. (2) A split is part of that
//! SAME CAS, so page size is bounded too. Both bounds are asserted, and `starved_consolidators_stay_bounded` replays the
//! worst schedule deterministically.
//! HELPING THE ROOT: if the root's splitter is slow, its right sibling can itself split before any parent level exists;
//! the helper must install the new root first (`stalled_splitter_is_helped`).
//! COMMON USES: the Bw-tree is the index in Microsoft's Hekaton (SQL Server in-memory OLTP), Azure Cosmos DB's indexing and the
//! Deuteronomy/LLAMA stack; the ideas (immutable deltas + CAS on an indirection table) also appear in log-structured stores.
//! Course: CMU 15-445 Lecture 10 (Index Concurrency Control), and 15-721 (latch-free indexes / Bw-tree).
//! Read: Levandoski, Lomet, Sengupta, "The Bw-Tree: A B-tree for New Hardware Platforms": https://www.microsoft.com/en-us/research/wp-content/uploads/2016/02/bw-tree-icde2013-final.pdf
//! Read: Wang et al., "Building a Bw-Tree Takes More Than Just Buzz Words": https://www.cs.cmu.edu/~huanche1/publications/open_bwtree.pdf
use std::collections::{BTreeMap, HashMap, HashSet};
use std::marker::PhantomData;
use std::ptr::null_mut;
use std::sync::Mutex;
use std::sync::atomic::{AtomicPtr, AtomicU32, AtomicU64, AtomicUsize, Ordering::*};

const NIL: u32 = u32::MAX;
const INF: u64 = u64::MAX; // keys must be < INF
const MAX_PAGE: usize = 32; // split when a consolidated page exceeds this many entries
const CONSOL: u32 = 6; // TRY to consolidate when a chain reaches this many records (the attempt may lose its CAS)
const MAX_CHAIN: u32 = 24; // HARD limit: nobody prepends to a chain this long; they must consolidate it first
// A consolidated page holds at most MAX_PAGE base entries + (MAX_CHAIN - 1) deltas; one split must bring each half back
// under MAX_PAGE, which holds when MAX_CHAIN <= MAX_PAGE.
const _: () = assert!(CONSOL < MAX_CHAIN && MAX_CHAIN as usize <= MAX_PAGE);
const TABLE_CAP: usize = 1 << 16;
const MAX_THREADS: usize = 16;

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

enum Kind {
    Base(Vec<(u64, u64)>),
    Ins(u64, u64),
    Del(u64),
    Split(u64, u32),
}

/// One immutable record. `level/high/right` always describe the page AS OF this record, so the head alone tells a reader
/// whether a key belongs to this page (`key < high`) or must follow the sibling link.
struct Rec {
    kind: Kind,
    next: *mut Rec,
    level: u8,
    high: u64,
    right: u32,
    len: u32,
    size: u32,
}

struct Chain<'a> {
    cur: *mut Rec,
    _m: PhantomData<&'a Rec>,
}
impl<'a> Iterator for Chain<'a> {
    type Item = &'a Rec;
    fn next(&mut self) -> Option<&'a Rec> {
        if self.cur.is_null() {
            return None;
        }
        // SAFETY: every chain pointer was published through the mapping table and the calling thread is epoch-pinned, so the
        // record cannot have been freed; records are immutable after publication.
        let r = unsafe { &*self.cur };
        self.cur = r.next;
        Some(r)
    }
}
fn chain<'a>(p: *mut Rec) -> Chain<'a> {
    Chain {
        cur: p,
        _m: PhantomData,
    }
}
fn rec<'a>(p: *mut Rec) -> &'a Rec {
    // SAFETY: as for `Chain::next`; `p` is a non-null head loaded from the table while pinned (or owned by this thread).
    unsafe { &*p }
}

struct Retired(*mut Rec);
// SAFETY: a retired record is owned by whoever holds the list; it is only freed, never read, after retirement.
unsafe impl Send for Retired {}

/// Per-thread context: id, private garbage list.
struct Ctx {
    tid: usize,
    garbage: Vec<(u64, *mut Rec)>,
    ops: u32,
}
impl Ctx {
    fn new(tid: usize) -> Ctx {
        Ctx {
            tid,
            garbage: vec![],
            ops: 0,
        }
    }
}

struct Guard<'a>(&'a AtomicU64);
impl Drop for Guard<'_> {
    fn drop(&mut self) {
        self.0.store(0, SeqCst); // 0 = not pinned
    }
}

#[derive(Default)]
struct Stats {
    allocs: AtomicUsize,
    frees: AtomicUsize,
    retired: AtomicUsize,
    consolidations: AtomicUsize,
    leaf_splits: AtomicUsize,
    inner_splits: AtomicUsize,
    root_splits: AtomicUsize,
    index_posts: AtomicUsize,
    cas_failures: AtomicUsize,
    forced_consolidations: AtomicUsize,
    root_helps: AtomicUsize,
    peak_chain: AtomicU64, // longest chain ever built (tracked at allocation, so no schedule can hide it)
    peak_base: AtomicU64,  // largest base page ever built
    chain_sum: AtomicU64,
    chain_samples: AtomicU64,
    chain_max: AtomicU64,
}

struct BwTree {
    table: Vec<AtomicPtr<Rec>>,
    next_pid: AtomicU32,
    root: AtomicU32,
    epoch: AtomicU64,
    slots: Vec<AtomicU64>,
    orphans: Mutex<Vec<Retired>>,
    st: Stats,
    /// Test hook: every OPTIONAL consolidation "loses the race" (the worst possible schedule for chain length).
    starve_consolidators: bool,
    /// Test hook: the splitting thread is "descheduled" between split step 1 and step 2; only helpers can finish.
    stall_splitters: bool,
}

impl BwTree {
    fn new() -> BwTree {
        let t = BwTree {
            table: (0..TABLE_CAP).map(|_| AtomicPtr::new(null_mut())).collect(),
            next_pid: AtomicU32::new(1),
            root: AtomicU32::new(0),
            epoch: AtomicU64::new(1),
            slots: (0..MAX_THREADS).map(|_| AtomicU64::new(0)).collect(),
            orphans: Mutex::new(vec![]),
            st: Stats::default(),
            starve_consolidators: false,
            stall_splitters: false,
        };
        let base = t.new_rec(Kind::Base(vec![]), null_mut(), 0, INF, NIL, 1, 0);
        t.table[0].store(base, Release);
        t
    }

    // ---- records and memory ------------------------------------------------------------------------------------------
    #[allow(clippy::too_many_arguments)]
    fn new_rec(
        &self,
        kind: Kind,
        next: *mut Rec,
        level: u8,
        high: u64,
        right: u32,
        len: u32,
        size: u32,
    ) -> *mut Rec {
        self.st.allocs.fetch_add(1, Relaxed);
        self.st.peak_chain.fetch_max(len as u64, Relaxed);
        if let Kind::Base(v) = &kind {
            self.st.peak_base.fetch_max(v.len() as u64, Relaxed);
        }
        Box::into_raw(Box::new(Rec {
            kind,
            next,
            level,
            high,
            right,
            len,
            size,
        }))
    }
    /// Free ONE record that was never published or is past its grace period.
    fn free_rec(&self, p: *mut Rec) {
        self.st.frees.fetch_add(1, Relaxed);
        // SAFETY: `p` came from `Box::into_raw` in `new_rec` and is freed exactly once (unpublished, or past its epoch grace).
        drop(unsafe { Box::from_raw(p) });
    }
    fn alloc_pid(&self) -> u32 {
        let p = self.next_pid.fetch_add(1, Relaxed);
        assert!((p as usize) < TABLE_CAP, "mapping table exhausted");
        p
    }
    fn pin(&self, tid: usize) -> Guard<'_> {
        loop {
            let g = self.epoch.load(SeqCst);
            self.slots[tid].store(g, SeqCst);
            if self.epoch.load(SeqCst) == g {
                return Guard(&self.slots[tid]);
            }
        }
    }
    fn retire_chain(&self, ctx: &mut Ctx, head: *mut Rec) {
        let e = self.epoch.load(SeqCst);
        let mut p = head;
        while !p.is_null() {
            let nx = rec(p).next;
            ctx.garbage.push((e, p));
            self.st.retired.fetch_add(1, Relaxed);
            p = nx;
        }
    }
    /// Advance the epoch if every pinned thread has seen the current one, then free what is two epochs old.
    fn collect(&self, ctx: &mut Ctx) {
        let g = self.epoch.load(SeqCst);
        if self
            .slots
            .iter()
            .all(|s| matches!(s.load(SeqCst), 0) || s.load(SeqCst) == g)
        {
            let _ = self.epoch.compare_exchange(g, g + 1, SeqCst, SeqCst);
        }
        let g = self.epoch.load(SeqCst);
        let mut keep = vec![];
        for (e, p) in ctx.garbage.drain(..) {
            if e + 2 <= g {
                self.free_rec(p);
            } else {
                keep.push((e, p));
            }
        }
        ctx.garbage = keep;
    }
    fn tick(&self, ctx: &mut Ctx) {
        ctx.ops += 1;
        if ctx.ops % 64 == 0 {
            self.collect(ctx);
        }
    }
    /// Thread exit: hand leftover garbage to the tree, which frees it at shutdown.
    fn drain(&self, ctx: &mut Ctx) {
        let mut o = self.orphans.lock().unwrap();
        o.extend(ctx.garbage.drain(..).map(|(_, p)| Retired(p)));
    }
    fn shutdown(&mut self) -> (usize, usize) {
        for slot in &self.table {
            let mut p = slot.swap(null_mut(), Acquire);
            while !p.is_null() {
                let nx = rec(p).next;
                self.free_rec(p);
                p = nx;
            }
        }
        let orphans: Vec<Retired> = std::mem::take(&mut *self.orphans.lock().unwrap());
        for Retired(p) in orphans {
            self.free_rec(p);
        }
        (self.st.allocs.load(SeqCst), self.st.frees.load(SeqCst))
    }

    // ---- reading chains ----------------------------------------------------------------------------------------------
    fn find_split(h: &Rec) -> Option<(u64, u32)> {
        chain(h as *const Rec as *mut Rec).find_map(|r| {
            if let Kind::Split(s, r) = r.kind {
                Some((s, r))
            } else {
                None
            }
        })
    }
    /// Exact lookup (leaf level).
    fn chain_get(h: &Rec, key: u64) -> Option<u64> {
        for r in chain(h as *const Rec as *mut Rec) {
            match &r.kind {
                Kind::Ins(k, v) if *k == key => return Some(*v),
                Kind::Del(k) if *k == key => return None,
                Kind::Base(v) => return v.binary_search_by_key(&key, |e| e.0).ok().map(|i| v[i].1),
                _ => {}
            }
        }
        None
    }
    /// Routing (inner level): the child of the largest separator <= key, over deltas and base.
    fn floor_child(h: &Rec, key: u64) -> u32 {
        let mut best: Option<(u64, u64)> = None;
        let mut consider = |c: (u64, u64)| {
            if best.is_none_or(|b| c.0 > b.0) {
                best = Some(c);
            }
        };
        for r in chain(h as *const Rec as *mut Rec) {
            match &r.kind {
                Kind::Ins(k, v) if *k <= key => consider((*k, *v)),
                Kind::Base(v) => {
                    let i = v.partition_point(|e| e.0 <= key);
                    if i > 0 {
                        consider(v[i - 1]);
                    }
                }
                _ => {}
            }
        }
        best.expect("inner page always has a separator <= key").1 as u32
    }
    fn has_entry(h: &Rec, sep: u64) -> bool {
        chain(h as *const Rec as *mut Rec).any(|r| match &r.kind {
            Kind::Ins(k, _) => *k == sep,
            Kind::Base(v) => v.binary_search_by_key(&sep, |e| e.0).is_ok(),
            _ => false,
        })
    }
    /// Apply the deltas oldest-first onto the base, then drop everything the page no longer owns (>= high).
    fn materialize(h: &Rec) -> Vec<(u64, u64)> {
        let recs: Vec<&Rec> = chain(h as *const Rec as *mut Rec).collect();
        let mut v = match &recs.last().expect("chain ends in a base").kind {
            Kind::Base(v) => v.clone(),
            _ => unreachable!("chain must end in a base"),
        };
        for r in recs.iter().rev() {
            match &r.kind {
                Kind::Ins(k, val) => match v.binary_search_by_key(k, |e| e.0) {
                    Ok(i) => v[i].1 = *val,
                    Err(i) => v.insert(i, (*k, *val)),
                },
                Kind::Del(k) => {
                    if let Ok(i) = v.binary_search_by_key(k, |e| e.0) {
                        v.remove(i);
                    }
                }
                _ => {}
            }
        }
        v.retain(|e| e.0 < h.high);
        v
    }

    // ---- traversal with help-along -------------------------------------------------------------------------------------
    /// Find the page at `target` level whose range contains `key`. Returns the PID and the head it validated.
    fn descend(&self, ctx: &mut Ctx, key: u64, target: u8) -> (u32, *mut Rec) {
        let mut pid = self.root.load(Acquire);
        let mut helped = NIL;
        loop {
            let head = self.table[pid as usize].load(Acquire);
            let h = rec(head);
            if key >= h.high {
                pid = h.right; // the page split: follow the sibling link; correct even before the parent knows
                continue;
            }
            if helped != pid && Self::find_split(h).is_some() {
                helped = pid;
                self.consolidate(ctx, pid); // posts the missing parent entry first, then drops the split delta
                continue;
            }
            if h.level == target {
                return (pid, head);
            }
            assert!(h.level > target);
            pid = Self::floor_child(h, key);
        }
    }

    /// Make sure the parent (or a new root) knows about the split of `pid` into `pid | right` at `sep`.
    fn ensure_index_entry(&self, ctx: &mut Ctx, pid: u32, level: u8, sep: u64, right: u32) {
        loop {
            if self.root.load(Acquire) == pid {
                let np = self.alloc_pid();
                let base = Kind::Base(vec![(0, pid as u64), (sep, right as u64)]);
                let r = self.new_rec(base, null_mut(), level + 1, INF, NIL, 1, 2);
                self.table[np as usize].store(r, Release);
                if self.root.compare_exchange(pid, np, AcqRel, Acquire).is_ok() {
                    self.st.root_splits.fetch_add(1, Relaxed);
                } else {
                    self.table[np as usize].store(null_mut(), Release);
                    self.free_rec(r);
                }
                return;
            }
            let root = self.root.load(Acquire);
            if root != pid && rec(self.table[root as usize].load(Acquire)).level == level {
                // `pid` is a right sibling of a root whose own split has no new root yet (its splitter is slow), so there
                // is no parent level to descend to. Finish the root's split first, then retry.
                self.st.root_helps.fetch_add(1, Relaxed);
                self.consolidate(ctx, root);
                continue;
            }
            let (ppid, phead) = self.descend(ctx, sep, level + 1);
            let h = rec(phead);
            if Self::has_entry(h, sep) {
                return;
            }
            if h.len >= MAX_CHAIN {
                self.force_consolidate(ctx, ppid);
                continue;
            }
            let d = self.new_rec(
                Kind::Ins(sep, right as u64),
                phead,
                h.level,
                h.high,
                h.right,
                h.len + 1,
                h.size + 1,
            );
            if self.table[ppid as usize]
                .compare_exchange(phead, d, AcqRel, Acquire)
                .is_ok()
            {
                self.st.index_posts.fetch_add(1, Relaxed);
                self.try_consolidate(ctx, ppid, h.len + 1);
                return;
            }
            self.st.cas_failures.fetch_add(1, Relaxed);
            self.free_rec(d); // unpublished: free just this record, not the chain it pointed at
        }
    }

    /// Optional consolidation after an append. It may lose its CAS to a faster writer; that is fine because of MAX_CHAIN.
    fn try_consolidate(&self, ctx: &mut Ctx, pid: u32, chain_len: u32) {
        if chain_len >= CONSOL && !self.starve_consolidators {
            self.consolidate(ctx, pid);
        }
    }

    /// Mandatory consolidation of a full chain. Every writer must come here instead of appending, so the head of a full
    /// page can only be replaced by a consolidation: at least one of the competing consolidators wins its CAS.
    fn force_consolidate(&self, ctx: &mut Ctx, pid: u32) {
        self.st.forced_consolidations.fetch_add(1, Relaxed);
        self.consolidate(ctx, pid);
    }

    /// Replace the whole chain of `pid` with ONE CAS: by a fresh base, or, if the page became too big, by a Split delta
    /// over the new left half. Doing the split in the same CAS matters: a separate "consolidate, then split" CAS can keep
    /// losing to writers, and the page then grows without bound.
    fn consolidate(&self, ctx: &mut Ctx, pid: u32) {
        let head = self.table[pid as usize].load(Acquire);
        let h = rec(head);
        if h.len == 1 {
            return; // already a bare base; replacing it would only make concurrent appends fail
        }
        if let Some((sep, right)) = Self::find_split(h) {
            // Never drop a split delta before the parent has the entry (it is the only record of step 2).
            self.ensure_index_entry(ctx, pid, h.level, sep, right);
        }
        let mut entries = Self::materialize(h);
        if entries.len() > MAX_PAGE {
            self.consolidate_and_split(ctx, pid, head, entries);
            return;
        }
        entries.shrink_to_fit();
        let n = entries.len() as u32;
        let nb = self.new_rec(
            Kind::Base(entries),
            null_mut(),
            h.level,
            h.high,
            h.right,
            1,
            n,
        );
        if self.table[pid as usize]
            .compare_exchange(head, nb, AcqRel, Acquire)
            .is_ok()
        {
            self.st.consolidations.fetch_add(1, Relaxed);
            self.retire_chain(ctx, head);
        } else {
            self.st.cas_failures.fetch_add(1, Relaxed);
            self.free_rec(nb);
        }
    }

    /// Step 1 of a split: build both halves, then CAS `Split delta -> left base` over the old chain.
    fn consolidate_and_split(
        &self,
        ctx: &mut Ctx,
        pid: u32,
        head: *mut Rec,
        mut left: Vec<(u64, u64)>,
    ) {
        let h = rec(head);
        let upper = left.split_off(left.len() / 2);
        let (sep, mid, rest) = (upper[0].0, left.len() as u32, upper.len() as u32);
        let rpid = self.alloc_pid();
        let right = self.new_rec(
            Kind::Base(upper),
            null_mut(),
            h.level,
            h.high,
            h.right,
            1,
            rest,
        );
        self.table[rpid as usize].store(right, Release); // published before the split delta can point at it
        let base = self.new_rec(Kind::Base(left), null_mut(), h.level, sep, rpid, 1, mid);
        let sp = self.new_rec(Kind::Split(sep, rpid), base, h.level, sep, rpid, 2, mid);
        if self.table[pid as usize]
            .compare_exchange(head, sp, AcqRel, Acquire)
            .is_ok()
        {
            self.st.consolidations.fetch_add(1, Relaxed);
            let ctr = if h.level == 0 {
                &self.st.leaf_splits
            } else {
                &self.st.inner_splits
            };
            ctr.fetch_add(1, Relaxed);
            self.retire_chain(ctx, head);
            if !self.stall_splitters {
                self.consolidate(ctx, pid); // step 2 (parent entry) happens inside, then the delta is folded away
            }
        } else {
            self.st.cas_failures.fetch_add(1, Relaxed);
            self.table[rpid as usize].store(null_mut(), Release);
            self.free_rec(right);
            self.free_rec(base);
            self.free_rec(sp);
        }
    }

    // ---- public operations -------------------------------------------------------------------------------------------
    fn sample(&self, h: &Rec) {
        self.st.chain_sum.fetch_add(h.len as u64, Relaxed);
        self.st.chain_samples.fetch_add(1, Relaxed);
        self.st.chain_max.fetch_max(h.len as u64, Relaxed);
    }

    fn get(&self, ctx: &mut Ctx, key: u64) -> Option<u64> {
        let g = self.pin(ctx.tid);
        let (_, head) = self.descend(ctx, key, 0);
        let h = rec(head);
        self.sample(h);
        let r = Self::chain_get(h, key);
        drop(g);
        self.tick(ctx);
        r
    }

    /// Insert or update; returns the previous value. Linearizes at the successful CAS.
    fn upsert(&self, ctx: &mut Ctx, key: u64, val: u64) -> Option<u64> {
        assert!(key < INF);
        let g = self.pin(ctx.tid);
        let r = loop {
            let (pid, head) = self.descend(ctx, key, 0);
            let h = rec(head);
            if h.len >= MAX_CHAIN {
                self.force_consolidate(ctx, pid);
                continue;
            }
            let cur = Self::chain_get(h, key);
            let d = self.new_rec(
                Kind::Ins(key, val),
                head,
                0,
                h.high,
                h.right,
                h.len + 1,
                h.size + 1,
            );
            if self.table[pid as usize]
                .compare_exchange(head, d, AcqRel, Acquire)
                .is_ok()
            {
                self.sample(rec(d));
                self.try_consolidate(ctx, pid, h.len + 1);
                break cur;
            }
            self.st.cas_failures.fetch_add(1, Relaxed);
            self.free_rec(d);
        };
        drop(g);
        self.tick(ctx);
        r
    }

    /// Delete; returns the removed value. A miss linearizes at the head load and needs no CAS.
    fn remove(&self, ctx: &mut Ctx, key: u64) -> Option<u64> {
        let g = self.pin(ctx.tid);
        let r = loop {
            let (pid, head) = self.descend(ctx, key, 0);
            let h = rec(head);
            let Some(cur) = Self::chain_get(h, key) else {
                break None;
            };
            if h.len >= MAX_CHAIN {
                self.force_consolidate(ctx, pid);
                continue;
            }
            let d = self.new_rec(
                Kind::Del(key),
                head,
                0,
                h.high,
                h.right,
                h.len + 1,
                h.size.saturating_sub(1),
            );
            if self.table[pid as usize]
                .compare_exchange(head, d, AcqRel, Acquire)
                .is_ok()
            {
                self.sample(rec(d));
                self.try_consolidate(ctx, pid, h.len + 1);
                break Some(cur);
            }
            self.st.cas_failures.fetch_add(1, Relaxed);
            self.free_rec(d);
        };
        drop(g);
        self.tick(ctx);
        r
    }

    /// Quiescent: walk the leaf level along sibling links, checking page ranges are contiguous, ordered and cover everything.
    fn scan_all(&self, ctx: &mut Ctx) -> (Vec<(u64, u64)>, usize) {
        let _g = self.pin(ctx.tid);
        let (mut pid, _) = self.descend(ctx, 0, 0);
        let (mut out, mut pages, mut low) = (vec![], 0, 0u64);
        loop {
            let h = rec(self.table[pid as usize].load(Acquire));
            assert!(Self::find_split(h).is_none() || h.right != NIL);
            for (k, v) in Self::materialize(h) {
                assert!(
                    k >= low && k < h.high,
                    "key {k} outside page range [{low}, {})",
                    h.high
                );
                assert!(
                    out.last().is_none_or(|l: &(u64, u64)| l.0 < k),
                    "leaf level must be strictly ordered"
                );
                out.push((k, v));
            }
            pages += 1;
            if h.right == NIL {
                assert_eq!(h.high, INF);
                break;
            }
            low = h.high;
            pid = h.right;
        }
        (out, pages)
    }
    fn height(&self) -> u8 {
        rec(self.table[self.root.load(Acquire) as usize].load(Acquire)).level + 1
    }
    /// Schedule-independent: these bounds are what keeps every operation cheap, however unlucky the thread timing is.
    fn assert_bounds(&self) {
        let (chain, base) = (
            self.st.peak_chain.load(SeqCst),
            self.st.peak_base.load(SeqCst),
        );
        assert!(
            chain <= MAX_CHAIN as u64,
            "delta chain reached {chain} records (limit {MAX_CHAIN}): consolidators starved"
        );
        assert!(
            base <= MAX_PAGE as u64,
            "base page reached {base} entries (limit {MAX_PAGE}): splits starved"
        );
    }
    fn report(&self, label: &str) {
        let s = &self.st;
        let n = s.chain_samples.load(Relaxed).max(1) as f64;
        println!(
            "  [{label}] height {} pages {} | consolidations {} leaf splits {} inner splits {} root splits {} index posts {} | chain len avg {:.2} max {} | forced consolidations {} | peak page {} | CAS failures {} | retired {} records",
            self.height(),
            self.next_pid.load(Relaxed),
            s.consolidations.load(Relaxed),
            s.leaf_splits.load(Relaxed),
            s.inner_splits.load(Relaxed),
            s.root_splits.load(Relaxed),
            s.index_posts.load(Relaxed),
            s.chain_sum.load(Relaxed) as f64 / n,
            s.peak_chain.load(Relaxed),
            s.forced_consolidations.load(Relaxed),
            s.peak_base.load(Relaxed),
            s.cas_failures.load(Relaxed),
            s.retired.load(Relaxed)
        );
    }
}

// ---- tests -------------------------------------------------------------------------------------------------------------
fn sequential_differential() {
    let mut t = BwTree::new();
    let mut ctx = Ctx::new(0);
    let mut oracle = BTreeMap::new();
    let mut rng = Rng(0xB77);
    for step in 0..20_000u64 {
        let k = rng.below(3000);
        match rng.below(100) {
            0..=44 => assert_eq!(t.upsert(&mut ctx, k, step), oracle.insert(k, step)),
            45..=69 => assert_eq!(t.remove(&mut ctx, k), oracle.remove(&k)),
            _ => assert_eq!(t.get(&mut ctx, k), oracle.get(&k).copied()),
        }
    }
    let (all, pages) = t.scan_all(&mut ctx);
    assert_eq!(
        all,
        oracle.iter().map(|(k, v)| (*k, *v)).collect::<Vec<_>>()
    );
    for (k, v) in &oracle {
        assert_eq!(t.get(&mut ctx, *k), Some(*v), "index routes every key");
    }
    assert!(t.height() >= 2 && pages > 10);
    t.report("sequential");
    t.assert_bounds();
    t.drain(&mut ctx);
    let (a, f) = t.shutdown();
    assert_eq!(a, f, "every record allocated was freed exactly once");
}

/// The worst schedule, made deterministic: EVERY optional consolidation loses. Only the MAX_CHAIN gate is left, and it
/// alone must keep chains and pages bounded. (Without the gate this builds one 20k-record chain.)
fn starved_consolidators_stay_bounded() {
    let mut t = BwTree::new();
    t.starve_consolidators = true;
    let mut ctx = Ctx::new(0);
    let mut oracle = BTreeMap::new();
    let mut rng = Rng(0x57A2);
    for step in 0..20_000u64 {
        let k = rng.below(3000);
        match rng.below(100) {
            0..=59 => assert_eq!(t.upsert(&mut ctx, k, step), oracle.insert(k, step)),
            60..=79 => assert_eq!(t.remove(&mut ctx, k), oracle.remove(&k)),
            _ => assert_eq!(t.get(&mut ctx, k), oracle.get(&k).copied()),
        }
    }
    let (all, pages) = t.scan_all(&mut ctx);
    assert_eq!(all, oracle.into_iter().collect::<Vec<_>>());
    t.assert_bounds();
    assert_eq!(
        t.st.peak_chain.load(SeqCst),
        MAX_CHAIN as u64,
        "the gate was exercised"
    );
    assert!(
        t.st.forced_consolidations.load(SeqCst) > 0 && pages > 10,
        "pages still split"
    );
    t.report("starved consolidators");
    t.drain(&mut ctx);
    let (a, f) = t.shutdown();
    assert_eq!(a, f);
}

/// Splitters that stop after step 1. The root leaf splits into R | R2 and no new root appears; then R2 splits into R2 | R3.
/// A helper working on R2 now needs a parent level that does not exist yet: it must finish the ROOT's split first.
fn stalled_splitter_is_helped() {
    let mut t = BwTree::new();
    t.stall_splitters = true;
    let mut ctx = Ctx::new(0);
    let mut oracle = BTreeMap::new();
    let mut k = 0u64;
    // Increasing keys only ever write to the rightmost page, so nobody helps the pages that split.
    while t.st.leaf_splits.load(SeqCst) < 2 {
        t.upsert(&mut ctx, k * 10, k);
        oracle.insert(k * 10, k);
        k += 1;
    }
    assert_eq!(
        t.height(),
        1,
        "both splits are half done: still no new root"
    );
    let root = rec(t.table[t.root.load(SeqCst) as usize].load(SeqCst));
    let sep1 = root.high; // the lowest key of R2
    assert_eq!(t.upsert(&mut ctx, sep1 + 1, 7), oracle.insert(sep1 + 1, 7)); // lands in R2 and helps it
    assert!(t.st.root_helps.load(SeqCst) >= 1 && t.height() == 2);
    let mut rng = Rng(0x57A11);
    for step in 0..20_000u64 {
        let k = rng.below(5000);
        match rng.below(100) {
            0..=59 => assert_eq!(t.upsert(&mut ctx, k, step), oracle.insert(k, step)),
            60..=79 => assert_eq!(t.remove(&mut ctx, k), oracle.remove(&k)),
            _ => assert_eq!(t.get(&mut ctx, k), oracle.get(&k).copied()),
        }
    }
    let (all, _) = t.scan_all(&mut ctx);
    assert_eq!(all, oracle.into_iter().collect::<Vec<_>>());
    t.assert_bounds();
    t.report("stalled splitters");
    t.drain(&mut ctx);
    let (a, f) = t.shutdown();
    assert_eq!(a, f);
}

const THREADS: usize = 8;
const OPS_PER_THREAD: usize = 25_000;

fn disjoint_concurrent() {
    let mut t = BwTree::new();
    let models: Vec<BTreeMap<u64, u64>> = std::thread::scope(|s| {
        let hs: Vec<_> = (0..THREADS)
            .map(|tid| {
                let t = &t;
                s.spawn(move || {
                    let mut ctx = Ctx::new(tid);
                    let (mut rng, mut m) = (Rng(1000 + tid as u64), BTreeMap::new());
                    for i in 0..OPS_PER_THREAD as u64 {
                        let k = rng.below(6000) * THREADS as u64 + tid as u64; // this thread's private residue class
                        match rng.below(100) {
                            0..=44 => assert_eq!(t.upsert(&mut ctx, k, i), m.insert(k, i)),
                            45..=64 => assert_eq!(t.remove(&mut ctx, k), m.remove(&k)),
                            _ => assert_eq!(t.get(&mut ctx, k), m.get(&k).copied()),
                        }
                    }
                    t.drain(&mut ctx);
                    m
                })
            })
            .collect();
        hs.into_iter().map(|h| h.join().unwrap()).collect()
    });
    let mut ctx = Ctx::new(0);
    let expect: BTreeMap<u64, u64> = models.into_iter().flatten().collect();
    let (all, pages) = t.scan_all(&mut ctx);
    assert_eq!(
        all,
        expect.iter().map(|(k, v)| (*k, *v)).collect::<Vec<_>>(),
        "final contents equal the sequential model"
    );
    for (k, v) in expect.iter().step_by(7) {
        assert_eq!(t.get(&mut ctx, *k), Some(*v));
    }
    println!(
        "disjoint ranges: {} keys in {pages} leaf pages after {} ops",
        expect.len(),
        THREADS * OPS_PER_THREAD
    );
    t.report("disjoint");
    t.assert_bounds();
    t.drain(&mut ctx);
    let (a, f) = t.shutdown();
    assert_eq!(a, f);
}

#[derive(Clone, Copy)]
enum Op {
    Ins { token: u64, prev: Option<u64> },
    Del { prev: Option<u64> },
    Get { seen: Option<u64> },
}

fn overlapping_linearizable() {
    let mut t = BwTree::new();
    let logs: Vec<Vec<(u64, Op)>> = std::thread::scope(|s| {
        let hs: Vec<_> = (0..THREADS)
            .map(|tid| {
                let t = &t;
                s.spawn(move || {
                    let mut ctx = Ctx::new(tid);
                    let mut rng = Rng(5000 + tid as u64 * 7919);
                    let mut log = Vec::with_capacity(OPS_PER_THREAD);
                    for i in 0..OPS_PER_THREAD as u64 {
                        let k = rng.below(1500); // every thread hits the same hot keys
                        let op = match rng.below(100) {
                            0..=39 => {
                                let token = ((tid as u64) << 32) | (i + 1); // unique per insert
                                Op::Ins {
                                    token,
                                    prev: t.upsert(&mut ctx, k, token),
                                }
                            }
                            40..=64 => Op::Del {
                                prev: t.remove(&mut ctx, k),
                            },
                            _ => Op::Get {
                                seen: t.get(&mut ctx, k),
                            },
                        };
                        log.push((k, op));
                    }
                    t.drain(&mut ctx);
                    log
                })
            })
            .collect();
        hs.into_iter().map(|h| h.join().unwrap()).collect()
    });
    let mut ctx = Ctx::new(0);
    let (all, _) = t.scan_all(&mut ctx);
    let final_state: HashMap<u64, u64> = all.into_iter().collect();
    let mut by_key: HashMap<u64, Vec<Op>> = HashMap::new();
    for log in &logs {
        for &(k, op) in log {
            by_key.entry(k).or_default().push(op);
        }
    }
    let mut checked = 0;
    for (k, ops) in &by_key {
        // Each key is a register whose states are None or Some(unique token). A state-changing op consumes the state it
        // observed. Linearizability implies: every consumed token was written, consumed at most once; None-states
        // (initial + one per successful delete) are consumed by at most one inserting op; the one unconsumed state is the final one.
        let written: HashSet<u64> = ops
            .iter()
            .filter_map(|o| {
                if let Op::Ins { token, .. } = o {
                    Some(*token)
                } else {
                    None
                }
            })
            .collect();
        let mut consumed: HashMap<u64, usize> = HashMap::new();
        let (mut ins_on_none, mut del_hits) = (0usize, 0usize);
        for op in ops {
            match *op {
                Op::Ins { prev: None, .. } => ins_on_none += 1,
                Op::Ins { prev: Some(p), .. } => *consumed.entry(p).or_default() += 1,
                Op::Del { prev: Some(p) } => {
                    del_hits += 1;
                    *consumed.entry(p).or_default() += 1;
                }
                Op::Del { prev: None } => {}
                Op::Get { seen: Some(p) } => {
                    assert!(written.contains(&p), "key {k}: read a token nobody wrote")
                }
                Op::Get { seen: None } => {}
            }
        }
        for (tok, n) in &consumed {
            assert!(written.contains(tok), "key {k}: consumed unknown token");
            assert_eq!(
                *n, 1,
                "key {k}: token {tok:#x} consumed {n} times (lost or duplicated update)"
            );
        }
        let unconsumed: Vec<u64> = written
            .iter()
            .copied()
            .filter(|t| !consumed.contains_key(t))
            .collect();
        match final_state.get(k) {
            Some(v) => {
                assert_eq!(
                    unconsumed,
                    vec![*v],
                    "key {k}: final value must be the one unconsumed write"
                );
                assert_eq!(ins_on_none, 1 + del_hits);
            }
            None => {
                assert!(
                    unconsumed.is_empty(),
                    "key {k}: deleted at the end but a write was never consumed"
                );
                assert_eq!(ins_on_none, del_hits, "key {k}: None-state accounting");
            }
        }
        checked += 1;
    }
    for k in final_state.keys() {
        assert!(by_key.contains_key(k));
    }
    println!(
        "overlapping hot keys: {checked} per-key logs linearizable, {} keys alive at the end",
        final_state.len()
    );
    t.report("overlap");
    t.assert_bounds();
    t.drain(&mut ctx);
    let (a, f) = t.shutdown();
    assert_eq!(a, f, "no leak, no double free");
    println!("  memory: {a} records allocated, {f} freed (epoch reclamation + shutdown)");
}

fn main() {
    println!(
        "Bw-tree: MAX_PAGE={MAX_PAGE}, CONSOL={CONSOL}, {THREADS} threads x {OPS_PER_THREAD} ops per phase"
    );
    sequential_differential();
    starved_consolidators_stay_bounded();
    stalled_splitter_is_helped();
    disjoint_concurrent();
    overlapping_linearizable();
    println!("all assertions passed");
}
