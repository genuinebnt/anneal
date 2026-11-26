//! RECIPE 440 (concurrency): safe memory reclamation for lock-free structures: leak vs hazard pointers vs epochs.
//! LEVEL: advanced
//! Use in: lock-free stacks/queues/maps with heap nodes, anywhere readers traverse memory that writers unlink.   Run: cargo run --release --example epoch_hazard_pointer_reclamation
#![allow(dead_code)]
//!
//! THE PROBLEM: in a lock-free structure a thread unlinks a node, but other threads may still hold a pointer to it
//! (they read `head`, then were preempted). Freeing at once is a use-after-free; and if the allocator reuses the
//! address, the stale CAS in a Treiber pop can even SUCCEED on a recycled node (ABA, see lockfree_stack_aba). A GC would
//! solve it; Rust must do it by hand: unlinked nodes are RETIRED and freed only once no thread can hold a pointer.
//! ONE TRAIT, FOUR STRATEGIES (`Reclaim`): `enter/leave` bracket an operation, `protect` loads a shared pointer
//! safely, `retire` hands over an unlinked node. A Treiber stack and a Michael-Scott queue are written once over it.
//! (a) `Leak`: never free. Correct, simplest, fastest, memory grows without bound (fine for short-lived programs).
//! (b) `HazardDomain` (Maged Michael): each thread has 2 published hazard slots. `protect` = publish the pointer, fence,
//!     re-read the source, retry if it changed. `retire` appends to the thread's list; at THRESH entries a SCAN collects
//!     every hazard slot and frees the unprotected nodes. Per-thread unfreed nodes are bounded by THRESH, even if
//!     another thread stalls forever. Cost: a SeqCst fence per protect.
//! (c) `EpochDomain` (Fraser; crossbeam-epoch): a global epoch; a thread PINS by publishing the epoch it saw. The epoch
//!     advances only when every pinned thread has observed the current one. A node retired in epoch e goes to limbo list
//!     e%3 and is freed when the global epoch reaches e+2 (no pinned thread can still be in epoch <= e). Reads are cheap
//!     (a plain load), but ONE stalled pinned thread blocks all reclamation: memory is unbounded.
//! (d) `Naive`: free immediately: the bug. The scripted demo replays the interleaving with a manual scheduler.
//! HOW WE PROVE IT (no real `free`, on purpose): freed nodes are QUARANTINED, never returned to the OS until the end:
//! `free` poisons the node (`magic = POISON`, value overwritten) and counts it; a second free of the same node is
//! counted as a double free. Every node read under protection checks `magic == LIVE`; a poisoned read is a
//! use-after-free that real memory could hide. Quarantine keeps the program sound (all accesses are atomics on valid
//! memory) while still detecting premature frees. With `recycle`, freed nodes are handed out again FIFO, which provokes ABA.
//! Swap `Alloc::free` for `dealloc` for production (and use crossbeam-epoch instead of this recipe). Run
//! `cargo +nightly miri run --example epoch_hazard_pointer_reclamation` after reducing the constants to check the unsafe code.
//! COMMON USES: crossbeam-epoch backs crossbeam's SkipMap/deque/queue; hazard pointers are in folly (Meta), the C++26
//! `std::hazard_pointer`, and the `haphazard` crate; epoch-style schemes in the Linux kernel RCU and in concurrent maps; leaking
//! is used by short-lived tools and arenas.
//! Read: Maged M. Michael, "Hazard Pointers: Safe Memory Reclamation for Lock-Free Objects" (IEEE TPDS 2004) https://www.research.ibm.com/people/m/michael/ieeetpds-2004.pdf
//! and Keir Fraser, "Practical lock-freedom" (PhD thesis, 2004) https://www.cl.cam.ac.uk/techreports/UCAM-CL-TR-579.pdf
//! and Michael & Scott, "Simple, Fast, and Practical Non-Blocking and Blocking Concurrent Queue Algorithms" https://www.cs.rochester.edu/~scott/papers/1996_PODC_queues.pdf
//! and crossbeam-epoch https://github.com/crossbeam-rs/crossbeam and Aaron Turon, "Lock-freedom without garbage collection" https://aturon.github.io/blog/2015/08/27/epoch/
use crossbeam_utils::CachePadded;
use std::collections::VecDeque;
use std::ptr::null_mut;
use std::sync::atomic::{AtomicPtr, AtomicU8, AtomicU64, AtomicUsize, Ordering::*, fence};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Instant;

const LIVE: u64 = 0xC0FF_EE00_0000_0001;
const POISON: u64 = 0xDEAD_DEAD_DEAD_DEAD;
const MAX_THREADS: usize = 16;
const HP_THRESHOLD: usize = 64; // retired nodes per thread before a hazard scan
const THREADS: usize = 8;

// ================================================================ nodes and the quarantine allocator

struct Node {
    magic: AtomicU64,
    value: AtomicU64,
    next: AtomicPtr<Node>,
    all_next: AtomicPtr<Node>, // intrusive list of every node ever allocated, freed for real in `Alloc::drop`
}

/// SAFETY of `Send`: a `*mut Node` points to a heap node that is only ever accessed through atomics.
struct NodePtr(*mut Node);
unsafe impl Send for NodePtr {}

/// # Safety
/// `p` must point to a node allocated by an `Alloc` that is still alive (nodes are only deallocated in `Alloc::drop`).
unsafe fn node<'a>(p: *mut Node) -> &'a Node {
    // SAFETY: guaranteed by the caller; the node memory is quarantined, never reused as another type.
    unsafe { &*p }
}

struct Alloc {
    recycle: bool,
    all: AtomicPtr<Node>,
    pool: Mutex<VecDeque<NodePtr>>,
    allocs: AtomicUsize,
    freed: AtomicUsize,
    double_free: AtomicUsize,
    bad_reads: AtomicUsize,
}

impl Alloc {
    fn new(recycle: bool) -> Arc<Self> {
        Arc::new(Alloc {
            recycle,
            all: AtomicPtr::new(null_mut()),
            pool: Mutex::new(VecDeque::new()),
            allocs: AtomicUsize::new(0),
            freed: AtomicUsize::new(0),
            double_free: AtomicUsize::new(0),
            bad_reads: AtomicUsize::new(0),
        })
    }

    fn new_node(&self, v: u64) -> *mut Node {
        self.allocs.fetch_add(1, Relaxed);
        if self.recycle {
            if let Some(NodePtr(p)) = self.pool.lock().unwrap().pop_front() {
                // SAFETY: pooled pointers come from `new_node` and the memory lives until `Alloc::drop`.
                let n = unsafe { node(p) };
                n.value.store(v, Relaxed);
                n.next.store(null_mut(), Relaxed);
                n.magic.store(LIVE, Release);
                return p;
            }
        }
        let p = Box::into_raw(Box::new(Node {
            magic: AtomicU64::new(LIVE),
            value: AtomicU64::new(v),
            next: AtomicPtr::new(null_mut()),
            all_next: AtomicPtr::new(null_mut()),
        }));
        loop {
            let h = self.all.load(Relaxed);
            // SAFETY: `p` was just created and is not yet shared.
            unsafe { node(p) }.all_next.store(h, Relaxed);
            if self
                .all
                .compare_exchange_weak(h, p, Release, Relaxed)
                .is_ok()
            {
                return p;
            }
        }
    }

    /// "Frees" a node: poison + count (+ recycle). A second free of the same node is counted, not performed.
    /// # Safety
    /// `p` must have been returned by `new_node` of this allocator.
    unsafe fn free(&self, p: *mut Node) {
        // SAFETY: the caller guarantees provenance; memory stays valid until `Alloc::drop`.
        let n = unsafe { node(p) };
        if n.magic.swap(POISON, AcqRel) != LIVE {
            self.double_free.fetch_add(1, Relaxed);
            return;
        }
        n.value.store(POISON, Relaxed);
        self.freed.fetch_add(1, Relaxed);
        if self.recycle {
            self.pool.lock().unwrap().push_back(NodePtr(p));
        }
    }

    /// A node read under protection must still be alive; otherwise reclamation freed it too early.
    fn check(&self, n: &Node) {
        if n.magic.load(Acquire) != LIVE {
            self.bad_reads.fetch_add(1, Relaxed);
        }
    }
}

impl Drop for Alloc {
    fn drop(&mut self) {
        let mut p = *self.all.get_mut();
        while !p.is_null() {
            // SAFETY: every node on `all` came from `Box::into_raw` exactly once and we have exclusive access (&mut self).
            let b = unsafe { Box::from_raw(p) };
            p = b.all_next.load(Relaxed);
        }
    }
}

// ================================================================ the reclamation interface

trait Reclaim: Sync + Send {
    type Local;
    const NAME: &'static str;
    /// True when the scheme never frees (the balance check differs).
    const LEAKS: bool = false;
    fn register(&self) -> Self::Local;
    /// Thread is done: flush its retired nodes (leftovers go to a global orphan list freed at teardown).
    fn unregister(&self, l: Self::Local);
    fn enter(&self, _l: &mut Self::Local) {}
    fn leave(&self, _l: &mut Self::Local) {}
    /// Load `src` such that the returned node cannot be freed until `leave`.
    fn protect(&self, _l: &mut Self::Local, _slot: usize, src: &AtomicPtr<Node>) -> *mut Node {
        src.load(Acquire)
    }
    /// # Safety
    /// `p` is unlinked from the structure (unreachable for new readers) and is retired exactly once.
    unsafe fn retire(&self, l: &mut Self::Local, p: *mut Node);
    /// Try hard to reclaim now (used by tests, teardown, and the scripted scheduler).
    fn flush(&self, _l: &mut Self::Local) {}
    /// Peak number of retired-but-unfreed nodes held by any single thread.
    fn peak_pending(&self) -> usize;
}

// ---------------------------------------------------------------- (d) naive: free at once

struct Naive(Arc<Alloc>);
impl Reclaim for Naive {
    type Local = ();
    const NAME: &'static str = "naive-free";
    fn register(&self) {}
    fn unregister(&self, _: ()) {}
    unsafe fn retire(&self, _: &mut (), p: *mut Node) {
        // SAFETY: `p` came from our allocator. (Freeing while others may still read it is the bug being shown.)
        unsafe { self.0.free(p) }
    }
    fn peak_pending(&self) -> usize {
        0
    }
}

// ---------------------------------------------------------------- (a) leak

struct Leak {
    retired: AtomicUsize,
}
impl Reclaim for Leak {
    type Local = ();
    const NAME: &'static str = "leak";
    const LEAKS: bool = true;
    fn register(&self) {}
    fn unregister(&self, _: ()) {}
    unsafe fn retire(&self, _: &mut (), _p: *mut Node) {
        self.retired.fetch_add(1, Relaxed); // the node is simply forgotten
    }
    fn peak_pending(&self) -> usize {
        self.retired.load(Relaxed)
    }
}

// ---------------------------------------------------------------- (b) hazard pointers

#[derive(Default)]
struct Slots([AtomicPtr<Node>; 2]);

struct HazardDomain {
    alloc: Arc<Alloc>,
    slots: Vec<CachePadded<Slots>>,
    next_id: AtomicUsize,
    orphans: Mutex<Vec<NodePtr>>,
    peak: AtomicUsize,
}

struct HpLocal {
    id: usize,
    retired: Vec<*mut Node>,
}

impl HazardDomain {
    fn new(alloc: Arc<Alloc>) -> Self {
        HazardDomain {
            alloc,
            slots: (0..MAX_THREADS)
                .map(|_| CachePadded::new(Slots::default()))
                .collect(),
            next_id: AtomicUsize::new(0),
            orphans: Mutex::new(Vec::new()),
            peak: AtomicUsize::new(0),
        }
    }

    /// Free every retired node of this thread that no hazard slot currently names.
    fn scan(&self, l: &mut HpLocal) {
        fence(SeqCst); // pairs with the fence in `protect`: either the reader sees the unlink or we see its hazard
        let mut hazards: Vec<usize> = self
            .slots
            .iter()
            .flat_map(|s| s.0.iter())
            .map(|h| h.load(SeqCst) as usize)
            .filter(|&p| p != 0)
            .collect();
        hazards.sort_unstable();
        let (keep, free): (Vec<_>, Vec<_>) = l
            .retired
            .drain(..)
            .partition(|&p| hazards.binary_search(&(p as usize)).is_ok());
        l.retired = keep;
        for p in free {
            // SAFETY: retired nodes are unlinked, retired once, and no hazard slot protects `p`.
            unsafe { self.alloc.free(p) };
        }
    }
}

impl Reclaim for HazardDomain {
    type Local = HpLocal;
    const NAME: &'static str = "hazard-ptr";
    fn register(&self) -> HpLocal {
        let id = self.next_id.fetch_add(1, Relaxed);
        assert!(id < MAX_THREADS, "too many threads for this demo");
        HpLocal {
            id,
            retired: Vec::new(),
        }
    }
    fn unregister(&self, mut l: HpLocal) {
        self.leave(&mut l);
        self.scan(&mut l);
        self.orphans
            .lock()
            .unwrap()
            .extend(l.retired.drain(..).map(NodePtr));
    }
    fn leave(&self, l: &mut HpLocal) {
        for h in &self.slots[l.id].0 {
            h.store(null_mut(), Release);
        }
    }
    fn protect(&self, l: &mut HpLocal, slot: usize, src: &AtomicPtr<Node>) -> *mut Node {
        let hazard = &self.slots[l.id].0[slot];
        let mut p = src.load(Acquire);
        loop {
            hazard.store(p, SeqCst); // publish BEFORE validating
            fence(SeqCst);
            let q = src.load(Acquire);
            if p == q {
                return p; // still reachable after publication, so no scan can free it until we clear the slot
            }
            p = q;
        }
    }
    unsafe fn retire(&self, l: &mut HpLocal, p: *mut Node) {
        l.retired.push(p);
        self.peak.fetch_max(l.retired.len(), Relaxed);
        if l.retired.len() >= HP_THRESHOLD {
            self.scan(l);
        }
    }
    fn flush(&self, l: &mut HpLocal) {
        self.scan(l);
    }
    fn peak_pending(&self) -> usize {
        self.peak.load(Relaxed)
    }
}

impl Drop for HazardDomain {
    fn drop(&mut self) {
        for NodePtr(p) in self.orphans.get_mut().unwrap().drain(..) {
            // SAFETY: exclusive access at teardown, every orphan was retired once.
            unsafe { self.alloc.free(p) };
        }
    }
}

// ---------------------------------------------------------------- (c) epoch based reclamation

struct EpochDomain {
    alloc: Arc<Alloc>,
    global: AtomicUsize,
    /// Per thread: 0 = not pinned, else `(epoch << 1) | 1`.
    slots: Vec<CachePadded<AtomicUsize>>,
    next_id: AtomicUsize,
    orphans: Mutex<Vec<NodePtr>>,
    peak: AtomicUsize,
}

struct EbrLocal {
    id: usize,
    limbo: [Vec<*mut Node>; 3], // list e%3 holds nodes retired in epoch e
    tags: [usize; 3],
    ops: usize,
}

impl EpochDomain {
    fn new(alloc: Arc<Alloc>) -> Self {
        EpochDomain {
            alloc,
            global: AtomicUsize::new(0),
            slots: (0..MAX_THREADS)
                .map(|_| CachePadded::new(AtomicUsize::new(0)))
                .collect(),
            next_id: AtomicUsize::new(0),
            orphans: Mutex::new(Vec::new()),
            peak: AtomicUsize::new(0),
        }
    }

    /// Epoch g -> g+1, but only if every pinned thread has already observed g.
    fn try_advance(&self) {
        let g = self.global.load(SeqCst);
        fence(SeqCst);
        for s in &self.slots {
            let v = s.load(SeqCst);
            if v & 1 == 1 && v >> 1 != g {
                return; // a straggler is still pinned in an older epoch
            }
        }
        let _ = self.global.compare_exchange(g, g + 1, SeqCst, Relaxed);
    }

    /// Free limbo lists that are at least two epochs old.
    fn collect(&self, l: &mut EbrLocal) {
        let g = self.global.load(SeqCst);
        for i in 0..3 {
            if l.tags[i] + 2 <= g {
                for p in l.limbo[i].drain(..) {
                    // SAFETY: retired in epoch tags[i] <= g-2: every thread pinned back then has since unpinned.
                    unsafe { self.alloc.free(p) };
                }
            }
        }
    }
}

impl Reclaim for EpochDomain {
    type Local = EbrLocal;
    const NAME: &'static str = "epoch";
    fn register(&self) -> EbrLocal {
        let id = self.next_id.fetch_add(1, Relaxed);
        assert!(id < MAX_THREADS, "too many threads for this demo");
        EbrLocal {
            id,
            limbo: Default::default(),
            tags: [0; 3],
            ops: 0,
        }
    }
    fn unregister(&self, mut l: EbrLocal) {
        self.slots[l.id].store(0, Release);
        self.flush(&mut l);
        let mut orphans = self.orphans.lock().unwrap();
        for list in &mut l.limbo {
            orphans.extend(list.drain(..).map(NodePtr));
        }
    }
    fn enter(&self, l: &mut EbrLocal) {
        let g = self.global.load(SeqCst);
        self.slots[l.id].store((g << 1) | 1, SeqCst); // pin: "I may hold pointers read in epoch g"
        fence(SeqCst);
        self.collect(l);
    }
    fn leave(&self, l: &mut EbrLocal) {
        self.slots[l.id].store(0, Release);
        l.ops += 1;
        if l.ops % 32 == 0 {
            self.try_advance();
        }
    }
    unsafe fn retire(&self, l: &mut EbrLocal, p: *mut Node) {
        let g = self.global.load(SeqCst);
        let i = g % 3;
        if l.tags[i] != g {
            // The slot held an older epoch (tag <= g-3): safe to free before reusing it for epoch g.
            for old in l.limbo[i].drain(..) {
                // SAFETY: tag <= g-3 <= g-2, see `collect`.
                unsafe { self.alloc.free(old) };
            }
            l.tags[i] = g;
        }
        l.limbo[i].push(p);
        self.peak
            .fetch_max(l.limbo.iter().map(Vec::len).sum(), Relaxed);
    }
    fn flush(&self, l: &mut EbrLocal) {
        for _ in 0..4 {
            self.try_advance();
        }
        self.collect(l);
    }
    fn peak_pending(&self) -> usize {
        self.peak.load(Relaxed)
    }
}

impl Drop for EpochDomain {
    fn drop(&mut self) {
        for NodePtr(p) in self.orphans.get_mut().unwrap().drain(..) {
            // SAFETY: exclusive access at teardown, every orphan was retired once.
            unsafe { self.alloc.free(p) };
        }
    }
}

// ================================================================ the structures, generic over Reclaim

trait Container<R: Reclaim>: Sync {
    const NAME: &'static str;
    const FIFO: bool;
    fn new(alloc: Arc<Alloc>, r: R) -> Self;
    fn domain(&self) -> &R;
    fn put(&self, l: &mut R::Local, v: u64);
    fn take(&self, l: &mut R::Local) -> Option<u64>;
}

struct TreiberStack<R: Reclaim> {
    head: AtomicPtr<Node>,
    alloc: Arc<Alloc>,
    r: R,
}

impl<R: Reclaim> Container<R> for TreiberStack<R> {
    const NAME: &'static str = "treiber-stack";
    const FIFO: bool = false;
    fn new(alloc: Arc<Alloc>, r: R) -> Self {
        Self {
            head: AtomicPtr::new(null_mut()),
            alloc,
            r,
        }
    }
    fn domain(&self) -> &R {
        &self.r
    }
    fn put(&self, _l: &mut R::Local, v: u64) {
        let n = self.alloc.new_node(v);
        loop {
            let h = self.head.load(Relaxed);
            // SAFETY: `n` is ours until the CAS below publishes it.
            unsafe { node(n) }.next.store(h, Relaxed);
            if self
                .head
                .compare_exchange_weak(h, n, Release, Relaxed)
                .is_ok()
            {
                return;
            }
        }
    }
    fn take(&self, l: &mut R::Local) -> Option<u64> {
        self.r.enter(l);
        let out = loop {
            let h = self.r.protect(l, 0, &self.head);
            if h.is_null() {
                break None;
            }
            // SAFETY: `h` is protected, so the reclaimer will not free it before `leave`.
            let hn = unsafe { node(h) };
            let next = hn.next.load(Acquire);
            let v = hn.value.load(Relaxed);
            self.alloc.check(hn);
            if self.head.compare_exchange(h, next, AcqRel, Acquire).is_ok() {
                // SAFETY: the CAS unlinked `h`; only the winner retires it.
                unsafe { self.r.retire(l, h) };
                break Some(v);
            }
        };
        self.r.leave(l);
        out
    }
}

impl<R: Reclaim> Drop for TreiberStack<R> {
    fn drop(&mut self) {
        let mut p = *self.head.get_mut();
        while !p.is_null() {
            // SAFETY: exclusive access; nodes still linked were never retired.
            let next = unsafe { node(p) }.next.load(Relaxed);
            unsafe { self.alloc.free(p) };
            p = next;
        }
    }
}

/// Michael-Scott queue with a dummy head node.
struct MsQueue<R: Reclaim> {
    head: AtomicPtr<Node>,
    tail: AtomicPtr<Node>,
    alloc: Arc<Alloc>,
    r: R,
}

impl<R: Reclaim> Container<R> for MsQueue<R> {
    const NAME: &'static str = "ms-queue";
    const FIFO: bool = true;
    fn new(alloc: Arc<Alloc>, r: R) -> Self {
        let dummy = alloc.new_node(0);
        Self {
            head: AtomicPtr::new(dummy),
            tail: AtomicPtr::new(dummy),
            alloc,
            r,
        }
    }
    fn domain(&self) -> &R {
        &self.r
    }
    fn put(&self, l: &mut R::Local, v: u64) {
        let n = self.alloc.new_node(v);
        self.r.enter(l);
        loop {
            let t = self.r.protect(l, 0, &self.tail);
            // SAFETY: `t` is protected until `leave`.
            let tn = unsafe { node(t) };
            self.alloc.check(tn);
            let next = tn.next.load(Acquire);
            if self.tail.load(Acquire) != t {
                continue;
            }
            if !next.is_null() {
                let _ = self.tail.compare_exchange(t, next, Release, Relaxed); // help a lagging enqueuer
            } else if tn
                .next
                .compare_exchange(null_mut(), n, AcqRel, Relaxed)
                .is_ok()
            {
                let _ = self.tail.compare_exchange(t, n, Release, Relaxed);
                break;
            }
        }
        self.r.leave(l);
    }
    fn take(&self, l: &mut R::Local) -> Option<u64> {
        self.r.enter(l);
        let out = loop {
            let h = self.r.protect(l, 0, &self.head);
            let t = self.tail.load(Acquire); // only compared, never dereferenced
            // SAFETY: `h` is protected.
            let hn = unsafe { node(h) };
            let next = self.r.protect(l, 1, &hn.next);
            if self.head.load(Acquire) != h {
                continue; // `h` was dequeued meanwhile, so `next` may not be safe: revalidate
            }
            if next.is_null() {
                break None;
            }
            if h == t {
                let _ = self.tail.compare_exchange(t, next, Release, Relaxed); // help
                continue;
            }
            // SAFETY: `next` is protected (slot 1) and was validated while `h` was still the head.
            let nn = unsafe { node(next) };
            let v = nn.value.load(Relaxed);
            self.alloc.check(nn);
            self.alloc.check(hn);
            if self.head.compare_exchange(h, next, AcqRel, Acquire).is_ok() {
                // SAFETY: `h` (the old dummy) is unlinked and retired by the single winner.
                unsafe { self.r.retire(l, h) };
                break Some(v);
            }
        };
        self.r.leave(l);
        out
    }
}

impl<R: Reclaim> Drop for MsQueue<R> {
    fn drop(&mut self) {
        let mut p = *self.head.get_mut();
        while !p.is_null() {
            // SAFETY: exclusive access; linked nodes were never retired.
            let next = unsafe { node(p) }.next.load(Relaxed);
            unsafe { self.alloc.free(p) };
            p = next;
        }
    }
}

// ================================================================ 1. scripted ABA / use-after-free

#[derive(Debug)]
struct Script {
    cas_ok: bool,
    rest: Vec<u64>,
    bad_reads: usize,
    double_free: usize,
    freed_at_end: usize,
    allocs: usize,
}

/// A manual scheduler on ONE thread: T1 starts `pop`, is "preempted" after reading `head=A, next=B`; T2 pops A and B,
/// pushes 100 (the FIFO recycler hands A's address back = ABA); T1 resumes and does its CAS.
fn scripted_interleaving<R: Reclaim>(mk: fn(Arc<Alloc>) -> R) -> Script {
    let alloc = Alloc::new(true);
    let s = TreiberStack::new(alloc.clone(), mk(alloc.clone()));
    let (mut t1, mut t2) = (s.r.register(), s.r.register());
    for v in [3, 2, 1] {
        s.put(&mut t2, v); // head: A(1) -> B(2) -> C(3)
    }
    // ---- T1: first half of pop()
    s.r.enter(&mut t1);
    let a = s.r.protect(&mut t1, 0, &s.head);
    // SAFETY: protected by T1's hazard/pin (or, for naive-free, valid because nothing was freed yet).
    let b = unsafe { node(a) }.next.load(Acquire);
    // ---- scheduler switches to T2, which runs to completion
    assert_eq!(s.take(&mut t2), Some(1));
    assert_eq!(s.take(&mut t2), Some(2));
    s.r.flush(&mut t2);
    s.put(&mut t2, 100);
    // ---- back to T1: second half of pop(): CAS(head, A, B)
    let cas_ok = s.head.compare_exchange(a, b, AcqRel, Acquire).is_ok();
    s.r.leave(&mut t1);
    let mut rest = Vec::new();
    while let Some(v) = s.take(&mut t2) {
        rest.push(v);
    }
    s.r.flush(&mut t2);
    let out = Script {
        cas_ok,
        rest,
        bad_reads: alloc.bad_reads.load(Relaxed),
        double_free: alloc.double_free.load(Relaxed),
        freed_at_end: alloc.freed.load(Relaxed),
        allocs: alloc.allocs.load(Relaxed),
    };
    s.r.unregister(t1);
    s.r.unregister(t2);
    out
}

fn demo_scripted() {
    let naive = scripted_interleaving(|a| Naive(a));
    println!("1. scripted interleaving, naive free:   {naive:?}");
    assert!(
        naive.cas_ok,
        "ABA: the stale CAS succeeded on the recycled node"
    );
    assert!(
        naive.rest.contains(&POISON) && !naive.rest.contains(&100),
        "garbage popped and the push of 100 was lost"
    );
    assert!(
        naive.bad_reads >= 1 && naive.double_free >= 1,
        "use-after-free and double free were detected"
    );
    for (name, o) in [
        ("hazard-ptr", scripted_interleaving(HazardDomain::new)),
        ("epoch", scripted_interleaving(EpochDomain::new)),
    ] {
        println!("   scripted interleaving, {name:10}: {o:?}");
        assert!(
            !o.cas_ok,
            "{name}: the protected node was not recycled, so the stale CAS fails"
        );
        assert_eq!(o.rest, [100, 3], "{name}: nothing lost, nothing corrupted");
        assert_eq!((o.bad_reads, o.double_free), (0, 0), "{name}");
        assert_eq!(
            o.freed_at_end, o.allocs,
            "{name}: every node freed exactly once"
        );
    }
}

// ================================================================ 2. stalled reader: bounded vs unbounded garbage

fn stalled_reader<R: Reclaim>(mk: fn(Arc<Alloc>) -> R) -> (usize, usize) {
    const OPS: usize = 50_000;
    let alloc = Alloc::new(false);
    let s = TreiberStack::new(alloc.clone(), mk(alloc.clone()));
    let (mut stalled, mut worker) = (s.r.register(), s.r.register());
    s.put(&mut worker, 0);
    s.r.enter(&mut stalled); // a reader that pins/protects the head and then never returns (descheduled)
    let _held = s.r.protect(&mut stalled, 0, &s.head);
    for i in 0..OPS as u64 {
        s.put(&mut worker, i);
        assert!(s.take(&mut worker).is_some());
    }
    let (peak, freed_during) = (s.r.peak_pending(), alloc.freed.load(Relaxed));
    s.r.leave(&mut stalled); // the reader finally wakes up
    s.r.flush(&mut worker);
    assert_eq!(
        alloc.freed.load(Relaxed),
        OPS,
        "once the stall ends, all garbage is reclaimed"
    );
    while s.take(&mut worker).is_some() {}
    s.r.unregister(stalled);
    s.r.unregister(worker);
    (peak, freed_during)
}

fn demo_stalled() {
    let (hp, hp_freed) = stalled_reader(HazardDomain::new);
    let (ebr, ebr_freed) = stalled_reader(EpochDomain::new);
    println!(
        "2. stalled reader, 50k retirements: hazard peak pending {hp} (freed {hp_freed}); epoch peak pending {ebr} (freed {ebr_freed})"
    );
    assert!(
        hp <= HP_THRESHOLD,
        "hazard pointers: garbage bounded by the scan threshold"
    );
    assert!(
        ebr >= 49_000 && ebr_freed < 1000,
        "epochs: one stalled thread blocks reclamation"
    );
}

// ================================================================ 3. stress and throughput

struct Metrics {
    ops: usize,
    secs: f64,
    peak_pending: usize,
    allocs: usize,
    freed: usize,
}

/// THREADS threads, each: `iters` x (put unique value, take one). Asserts no loss/duplication, per-producer FIFO for
/// queues, zero poisoned reads, zero double frees, and exact free accounting.
fn stress<R: Reclaim, C: Container<R>>(
    mk: fn(Arc<Alloc>) -> R,
    recycle: bool,
    iters: usize,
) -> Metrics {
    let alloc = Alloc::new(recycle);
    let c = C::new(alloc.clone(), mk(alloc.clone()));
    let seen: Vec<AtomicU8> = (0..THREADS * iters).map(|_| AtomicU8::new(0)).collect();
    let record = |v: u64, last: &mut [i64; THREADS]| {
        let (producer, seq) = ((v >> 32) as usize, (v & 0xFFFF_FFFF) as usize);
        assert!(
            producer < THREADS && seq < iters,
            "garbage value {v:#x} (use-after-free?)"
        );
        assert_eq!(
            seen[producer * iters + seq].fetch_add(1, Relaxed),
            0,
            "value {v:#x} delivered twice"
        );
        if C::FIFO {
            assert!(
                seq as i64 > last[producer],
                "FIFO order from producer {producer} violated"
            );
            last[producer] = seq as i64;
        }
    };
    let t0 = Instant::now();
    thread::scope(|sc| {
        for t in 0..THREADS {
            let (c, record) = (&c, &record);
            sc.spawn(move || {
                let mut l = c.domain().register();
                let mut last = [-1i64; THREADS];
                for i in 0..iters {
                    c.put(&mut l, ((t as u64) << 32) | i as u64);
                    if let Some(v) = c.take(&mut l) {
                        record(v, &mut last);
                    }
                }
                c.domain().unregister(l);
            });
        }
    });
    let secs = t0.elapsed().as_secs_f64();
    let mut l = c.domain().register();
    let mut last = [-1i64; THREADS];
    while let Some(v) = c.take(&mut l) {
        record(v, &mut last);
    }
    c.domain().unregister(l);
    assert!(
        seen.iter().all(|s| s.load(Relaxed) == 1),
        "an item was lost"
    );
    let peak_pending = c.domain().peak_pending();
    drop(c); // structure + reclaimer teardown: frees what is still linked and the orphans
    let m = Metrics {
        ops: THREADS * iters * 2,
        secs,
        peak_pending,
        allocs: alloc.allocs.load(Relaxed),
        freed: alloc.freed.load(Relaxed),
    };
    assert_eq!(
        alloc.bad_reads.load(Relaxed),
        0,
        "{}: a node was read after being freed",
        R::NAME
    );
    assert_eq!(
        alloc.double_free.load(Relaxed),
        0,
        "{}: double free",
        R::NAME
    );
    if R::LEAKS {
        assert_eq!(
            m.freed,
            usize::from(C::FIFO),
            "leak frees only what the structure's own Drop frees (the queue's dummy)"
        );
    } else {
        assert_eq!(
            m.freed,
            m.allocs,
            "{}: every node freed exactly once",
            R::NAME
        );
    }
    m
}

fn run_all(title: &str, recycle: bool, iters: usize, bound_check: bool) {
    println!("{title}");
    println!(
        "   {:14} {:10} {:>9} {:>12} {:>10} {:>10}",
        "structure", "scheme", "Mops/s", "peak pending", "allocs", "freed"
    );
    macro_rules! row {
        ($c:ident, $r:ty, $mk:expr) => {{
            let m = stress::<$r, $c<$r>>($mk, recycle, iters);
            println!(
                "   {:14} {:10} {:9.2} {:12} {:10} {:10}",
                <$c<$r> as Container<$r>>::NAME,
                <$r as Reclaim>::NAME,
                m.ops as f64 / m.secs / 1e6,
                m.peak_pending,
                m.allocs,
                m.freed
            );
            if bound_check && !<$r as Reclaim>::LEAKS && <$r as Reclaim>::NAME == "hazard-ptr" {
                assert!(
                    m.peak_pending <= HP_THRESHOLD,
                    "hazard pointer garbage exceeded the per-thread bound"
                );
            }
        }};
    }
    row!(TreiberStack, Leak, |_| Leak {
        retired: AtomicUsize::new(0)
    });
    row!(TreiberStack, HazardDomain, HazardDomain::new);
    row!(TreiberStack, EpochDomain, EpochDomain::new);
    row!(MsQueue, Leak, |_| Leak {
        retired: AtomicUsize::new(0)
    });
    row!(MsQueue, HazardDomain, HazardDomain::new);
    row!(MsQueue, EpochDomain, EpochDomain::new);
}

fn main() {
    println!(
        "node = {} bytes, hazard threshold {HP_THRESHOLD}, {THREADS} threads",
        std::mem::size_of::<Node>()
    );
    demo_scripted();
    demo_stalled();
    // 8 threads x 18_750 iterations x (put + take) = 300_000 operations, nodes recycled FIFO to provoke ABA.
    run_all(
        "3. stress, 300k operations per row, recycling allocator (0 poisoned reads, 0 double frees, no loss):",
        true,
        18_750,
        true,
    );
    run_all(
        "4. throughput, 1.6M operations per row, no recycling (peak pending = overhead in nodes):",
        false,
        100_000,
        true,
    );
}
