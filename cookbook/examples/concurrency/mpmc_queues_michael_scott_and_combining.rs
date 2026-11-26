//! RECIPE 902 (concurrency): four MPMC queues side by side: Michael-Scott (tagged indices), two-lock, flat combining, bounded ring.
//! LEVEL: advanced
//! Use in: picking a work queue for a thread pool or pipeline, understanding ABA and why tags/epochs exist, interviews on lock-free queues.   Run: cargo run --release --example mpmc_queues_michael_scott_and_combining
//! COMMON USES: task queues in schedulers (tokio's inject queue, crossbeam `SegQueue`/`ArrayQueue`), log/metric pipelines, actor mailboxes, the Java `ConcurrentLinkedQueue` (a Michael-Scott variant).
//! Course: Herlihy & Shavit, "The Art of Multiprocessor Programming" ch. 10 (queues); Michael & Scott, PODC 1996.
#![allow(dead_code)]
//!
//! WHAT: one `Queue` trait (`push(tid, u64)`, `pop(tid) -> Option<u64>`) and four implementations, all in SAFE Rust:
//!   1. `MsQueue`   : the Michael-Scott lock-free queue. Nodes are slots in an arena; a "pointer" is a packed u64 = (index:32 | tag:32).
//!                    The TAG is bumped on every successful CAS of that word, so a CAS that would succeed only because the same index
//!                    came back ("A -> B -> A") fails instead. That is the ABA defence without hazard pointers: arena memory is never
//!                    returned to the OS, so a stale index is always a valid slot, and the tag makes stale *decisions* fail. A Treiber
//!                    free list (also tagged) recycles dequeued nodes, which is exactly what makes ABA possible in the first place.
//!                    The "helping" rule: if `tail.next` is non-null the tail is lagging, so ANY thread advances it before doing its own work.
//!                    This is what makes the queue lock-free: a stalled enqueuer never blocks others.
//!   2. `TwoLock`   : the textbook two-lock queue needs a dummy node and unsafe pointers; this safe variant keeps the idea (producers and
//!                    consumers use DIFFERENT locks) with an in-vector and an out-vector, swapped when the out side runs dry.
//!   3. `FlatCombining`: threads publish a request in their own slot and one thread at a time (whoever wins `try_lock`) executes every pending
//!                    request against a plain `VecDeque`. Few cache-line transfers for the data; great when the structure is the bottleneck.
//!   4. `Ring`      : bounded array queue with per-cell sequence numbers (Vyukov); see `mpmc_bounded_queue` for the `MaybeUninit` version, this one
//!                    stores `AtomicU64` so it needs no unsafe. `push` spins when full (back-pressure).
//! VERIFICATION: values are encoded `(producer << 32) | seq`. With 4 producers and 4 consumers (8 threads) we check exactly-once delivery
//! (a counter array, no loss, no duplicates) and that each consumer sees each producer's items in INCREASING order (per-producer FIFO is
//! implied by linearizable FIFO). Single-threaded differential tests against `VecDeque` cover empty/full edge cases.
//! ORDERINGS: all CAS/loads that carry a hand-off use Acquire/Release; the tag only has to be correct per word so CAS on the word is enough.
//! Read: Michael & Scott, "Simple, Fast, and Practical Non-Blocking and Blocking Concurrent Queue Algorithms" (PODC 1996, title only, not re-read here).
//! Read: Hendler, Incze, Shavit, Tzafrir, "Flat Combining and the Synchronization-Parallelism Tradeoff" (SPAA 2010, title only).
//! Read: https://www.1024cores.net/home/lock-free-algorithms/queues and https://marabos.nl/atomics/
//!
//! TRY NEXT:
//!   - [S] In `MsQueue`, make `tag` constant (always 0) in `pack`. Predict what the stress test does (wrong totals, hang, or still OK on your machine?), run it with a tiny arena (64 nodes) to force reuse, and explain what you saw.
//!   - [S] Remove the "help advance a lagging tail" branch in `push`. Which thread can now spin forever, and in what interleaving? Try to provoke it with `thread::yield_now()` between the two CASes.
//!   - [M] Add `len_hint()` to `FlatCombining` and a batch limit (a combiner serves at most 16 requests per pass); predict how the latency tail of waiting threads changes and print the max wait in spins.
//!   - [M] Make `Ring` MPSC-optimised: a single consumer needs no CAS on `deq`. Measure against the generic ring and reconcile with `spsc_ring_buffer`.
//!   - [A] PROJECT: drive `thread_pool_work_stealing` with each queue here as its injector and compare tail latency; then run the same workload through `model_checker_for_concurrent_code` style exploration for a 2-thread `push`/`pop` pair.

use std::collections::VecDeque;
use std::sync::Mutex;
use std::sync::atomic::{AtomicU32, AtomicU64, AtomicUsize, Ordering::*};
use std::thread;
use std::time::Instant;

trait Queue: Sync {
    fn name(&self) -> &'static str;
    fn push(&self, tid: usize, v: u64);
    fn pop(&self, tid: usize) -> Option<u64>;
}

// ---------------------------------------------------------------- 1. Michael-Scott with tagged arena indices
const NIL: u32 = u32::MAX;
fn pack(idx: u32, tag: u32) -> u64 {
    (tag as u64) << 32 | idx as u64
}
fn idx(w: u64) -> u32 {
    w as u32
}
fn tag(w: u64) -> u32 {
    (w >> 32) as u32
}

struct MsNode {
    val: AtomicU64,
    next: AtomicU64,  // tagged index of the next node in the queue
    fnext: AtomicU32, // link used only while the node sits in the free list
}
struct MsQueue {
    nodes: Vec<MsNode>,
    head: AtomicU64,
    tail: AtomicU64,
    free: AtomicU64, // tagged Treiber stack of recycled nodes
    recycled: AtomicUsize,
}
impl MsQueue {
    fn new(cap: usize) -> Self {
        // node 0 is the initial dummy; nodes 1..cap start on the free list
        let nodes: Vec<MsNode> = (0..cap)
            .map(|i| MsNode {
                val: AtomicU64::new(0),
                next: AtomicU64::new(pack(NIL, 0)),
                fnext: AtomicU32::new(if i + 1 < cap { i as u32 + 1 } else { NIL }),
            })
            .collect();
        MsQueue {
            nodes,
            head: AtomicU64::new(pack(0, 0)),
            tail: AtomicU64::new(pack(0, 0)),
            free: AtomicU64::new(pack(1, 0)),
            recycled: AtomicUsize::new(0),
        }
    }
    fn alloc(&self) -> u32 {
        loop {
            let h = self.free.load(Acquire);
            let i = idx(h);
            assert!(i != NIL, "arena exhausted");
            let nx = self.nodes[i as usize].fnext.load(Relaxed);
            if self
                .free
                .compare_exchange(h, pack(nx, tag(h).wrapping_add(1)), AcqRel, Acquire)
                .is_ok()
            {
                return i;
            }
        }
    }
    fn release(&self, i: u32) {
        loop {
            let h = self.free.load(Acquire);
            self.nodes[i as usize].fnext.store(idx(h), Relaxed);
            if self
                .free
                .compare_exchange(h, pack(i, tag(h).wrapping_add(1)), AcqRel, Acquire)
                .is_ok()
            {
                self.recycled.fetch_add(1, Relaxed);
                return;
            }
        }
    }
}
impl Queue for MsQueue {
    fn name(&self) -> &'static str {
        "Michael-Scott (tagged)"
    }
    fn push(&self, _tid: usize, v: u64) {
        let n = self.alloc();
        let node = &self.nodes[n as usize];
        node.val.store(v, Relaxed);
        node.next.store(
            pack(NIL, tag(node.next.load(Relaxed)).wrapping_add(1)),
            Relaxed,
        );
        loop {
            let t = self.tail.load(Acquire);
            let tn = &self.nodes[idx(t) as usize].next;
            let next = tn.load(Acquire);
            if t != self.tail.load(Acquire) {
                continue; // tail moved while we looked: our snapshot is inconsistent
            }
            if idx(next) == NIL {
                // tail really is last: try to link our node after it
                if tn
                    .compare_exchange(next, pack(n, tag(next).wrapping_add(1)), AcqRel, Acquire)
                    .is_ok()
                {
                    let _ = self.tail.compare_exchange(
                        t,
                        pack(n, tag(t).wrapping_add(1)),
                        AcqRel,
                        Acquire,
                    );
                    return;
                }
            } else {
                // lagging tail: help whoever linked a node but has not swung the tail yet
                let _ = self.tail.compare_exchange(
                    t,
                    pack(idx(next), tag(t).wrapping_add(1)),
                    AcqRel,
                    Acquire,
                );
            }
        }
    }
    fn pop(&self, _tid: usize) -> Option<u64> {
        loop {
            let h = self.head.load(Acquire);
            let t = self.tail.load(Acquire);
            let next = self.nodes[idx(h) as usize].next.load(Acquire);
            if h != self.head.load(Acquire) {
                continue;
            }
            if idx(h) == idx(t) {
                if idx(next) == NIL {
                    return None; // empty
                }
                let _ = self.tail.compare_exchange(
                    t,
                    pack(idx(next), tag(t).wrapping_add(1)),
                    AcqRel,
                    Acquire,
                );
            } else {
                // read the value BEFORE the CAS: after it, another thread may recycle `next` and overwrite it
                let v = self.nodes[idx(next) as usize].val.load(Acquire);
                if self
                    .head
                    .compare_exchange(h, pack(idx(next), tag(h).wrapping_add(1)), AcqRel, Acquire)
                    .is_ok()
                {
                    self.release(idx(h)); // the old dummy is garbage now; `next` becomes the new dummy
                    return Some(v);
                }
            }
        }
    }
}

// ---------------------------------------------------------------- 2. two locks (in / out vectors)
struct TwoLock {
    inp: Mutex<Vec<u64>>,
    out: Mutex<Vec<u64>>, // stored reversed: pop() from the back gives FIFO
}
impl Queue for TwoLock {
    fn name(&self) -> &'static str {
        "two-lock (in/out)"
    }
    fn push(&self, _t: usize, v: u64) {
        self.inp.lock().unwrap().push(v);
    }
    fn pop(&self, _t: usize) -> Option<u64> {
        let mut out = self.out.lock().unwrap();
        if out.is_empty() {
            // lock order is always out -> inp, so there is no deadlock; producers are blocked only for the swap
            let mut inp = self.inp.lock().unwrap();
            std::mem::swap(&mut *out, &mut *inp);
            out.reverse();
        }
        out.pop()
    }
}

// ---------------------------------------------------------------- 3. flat combining
const IDLE: u64 = 0;
const REQ_PUSH: u64 = 1;
const REQ_POP: u64 = 2;
const DONE_OK: u64 = 3;
const DONE_EMPTY: u64 = 4;
struct Slot {
    state: AtomicU64,
    val: AtomicU64,
}
struct FlatCombining {
    slots: Vec<Slot>,
    q: Mutex<VecDeque<u64>>,
    combined: AtomicUsize, // total requests served by combiners (for the stats line)
    passes: AtomicUsize,
}
impl FlatCombining {
    fn new(threads: usize) -> Self {
        FlatCombining {
            slots: (0..threads)
                .map(|_| Slot {
                    state: AtomicU64::new(IDLE),
                    val: AtomicU64::new(0),
                })
                .collect(),
            q: Mutex::new(VecDeque::new()),
            combined: AtomicUsize::new(0),
            passes: AtomicUsize::new(0),
        }
    }
    fn combine(&self, q: &mut VecDeque<u64>) {
        let mut served = 0;
        for s in &self.slots {
            match s.state.load(Acquire) {
                REQ_PUSH => {
                    q.push_back(s.val.load(Relaxed));
                    s.state.store(DONE_OK, Release);
                    served += 1;
                }
                REQ_POP => {
                    match q.pop_front() {
                        Some(v) => {
                            s.val.store(v, Relaxed);
                            s.state.store(DONE_OK, Release);
                        }
                        None => s.state.store(DONE_EMPTY, Release),
                    }
                    served += 1;
                }
                _ => {}
            }
        }
        self.combined.fetch_add(served, Relaxed);
        self.passes.fetch_add(1, Relaxed);
    }
    fn run(&self, tid: usize, req: u64, v: u64) -> Option<u64> {
        let s = &self.slots[tid];
        s.val.store(v, Relaxed);
        s.state.store(req, Release); // publish the request
        loop {
            if let Ok(mut g) = self.q.try_lock() {
                self.combine(&mut g); // we are the combiner: serve everybody, including ourselves
            }
            let st = s.state.load(Acquire);
            if st >= DONE_OK {
                let r = (st == DONE_OK && req == REQ_POP).then(|| s.val.load(Relaxed));
                s.state.store(IDLE, Relaxed);
                return r;
            }
            std::hint::spin_loop();
        }
    }
}
impl Queue for FlatCombining {
    fn name(&self) -> &'static str {
        "flat combining"
    }
    fn push(&self, tid: usize, v: u64) {
        self.run(tid, REQ_PUSH, v);
    }
    fn pop(&self, tid: usize) -> Option<u64> {
        self.run(tid, REQ_POP, 0)
    }
}

// ---------------------------------------------------------------- 4. bounded ring with sequence numbers
struct Cell {
    seq: AtomicUsize,
    val: AtomicU64,
}
struct Ring {
    cells: Vec<Cell>,
    mask: usize,
    enq: AtomicUsize,
    deq: AtomicUsize,
}
impl Ring {
    fn new(cap: usize) -> Self {
        assert!(cap.is_power_of_two());
        Ring {
            cells: (0..cap)
                .map(|i| Cell {
                    seq: AtomicUsize::new(i),
                    val: AtomicU64::new(0),
                })
                .collect(),
            mask: cap - 1,
            enq: AtomicUsize::new(0),
            deq: AtomicUsize::new(0),
        }
    }
    fn try_push(&self, v: u64) -> bool {
        let mut pos = self.enq.load(Relaxed);
        loop {
            let c = &self.cells[pos & self.mask];
            let seq = c.seq.load(Acquire);
            let diff = seq as isize - pos as isize;
            if diff == 0 {
                match self
                    .enq
                    .compare_exchange_weak(pos, pos + 1, Relaxed, Relaxed)
                {
                    Ok(_) => {
                        c.val.store(v, Relaxed);
                        c.seq.store(pos + 1, Release); // hand the cell to the consumer
                        return true;
                    }
                    Err(p) => pos = p,
                }
            } else if diff < 0 {
                return false; // the cell still holds an unconsumed item from the previous lap: full
            } else {
                pos = self.enq.load(Relaxed);
            }
        }
    }
    fn try_pop(&self) -> Option<u64> {
        let mut pos = self.deq.load(Relaxed);
        loop {
            let c = &self.cells[pos & self.mask];
            let seq = c.seq.load(Acquire);
            let diff = seq as isize - (pos + 1) as isize;
            if diff == 0 {
                match self
                    .deq
                    .compare_exchange_weak(pos, pos + 1, Relaxed, Relaxed)
                {
                    Ok(_) => {
                        let v = c.val.load(Relaxed);
                        c.seq.store(pos + self.mask + 1, Release); // free the cell for the next lap
                        return Some(v);
                    }
                    Err(p) => pos = p,
                }
            } else if diff < 0 {
                return None;
            } else {
                pos = self.deq.load(Relaxed);
            }
        }
    }
}
impl Queue for Ring {
    fn name(&self) -> &'static str {
        "bounded ring (seq)"
    }
    fn push(&self, _t: usize, v: u64) {
        while !self.try_push(v) {
            thread::yield_now(); // back-pressure
        }
    }
    fn pop(&self, _t: usize) -> Option<u64> {
        self.try_pop()
    }
}

// ---------------------------------------------------------------- verification
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
}

/// Single-threaded differential test against VecDeque (unbounded queues only compare while len < cap).
fn sequential_oracle(q: &dyn Queue, cap_limit: usize) {
    let mut model = VecDeque::new();
    let mut r = Rng(42);
    for i in 0..20_000u64 {
        if r.next() % 2 == 0 && model.len() < cap_limit {
            q.push(0, i);
            model.push_back(i);
        } else {
            assert_eq!(
                q.pop(0),
                model.pop_front(),
                "{} diverged from VecDeque at step {i}",
                q.name()
            );
        }
    }
    while let Some(v) = model.pop_front() {
        assert_eq!(q.pop(0), Some(v));
    }
    assert_eq!(q.pop(0), None);
}

const PRODUCERS: usize = 4;
const CONSUMERS: usize = 4;

/// Returns elapsed seconds. Panics on loss, duplication or per-producer reordering.
fn stress(q: &dyn Queue, per_producer: u64) -> f64 {
    let total = PRODUCERS as u64 * per_producer;
    let consumed = AtomicUsize::new(0);
    let t0 = Instant::now();
    let logs: Vec<Vec<u64>> = thread::scope(|s| {
        for p in 0..PRODUCERS {
            s.spawn(move || {
                for seq in 1..=per_producer {
                    q.push(p, (p as u64) << 32 | seq);
                }
            });
        }
        let hs: Vec<_> = (0..CONSUMERS)
            .map(|c| {
                let consumed = &consumed;
                s.spawn(move || {
                    let mut log = Vec::new();
                    while consumed.load(Relaxed) < total as usize {
                        match q.pop(PRODUCERS + c) {
                            Some(v) => {
                                consumed.fetch_add(1, Relaxed);
                                log.push(v);
                            }
                            None => std::hint::spin_loop(),
                        }
                    }
                    log
                })
            })
            .collect();
        hs.into_iter().map(|h| h.join().unwrap()).collect()
    });
    let secs = t0.elapsed().as_secs_f64();
    let mut seen = vec![0u8; total as usize];
    for log in &logs {
        let mut last = [0u64; PRODUCERS];
        for &v in log {
            let (p, seq) = ((v >> 32) as usize, v & 0xffff_ffff);
            assert!(
                seq > last[p],
                "{}: a consumer saw producer {p}'s items out of order ({seq} after {})",
                q.name(),
                last[p]
            );
            last[p] = seq;
            let slot = &mut seen[p * per_producer as usize + seq as usize - 1];
            assert_eq!(*slot, 0, "{}: duplicate delivery of ({p},{seq})", q.name());
            *slot = 1;
        }
    }
    assert!(seen.iter().all(|&b| b == 1), "{}: lost items", q.name());
    assert_eq!(
        q.pop(0),
        None,
        "{}: queue should be empty afterwards",
        q.name()
    );
    secs
}

fn main() {
    println!("== sequential differential tests vs VecDeque ==");
    sequential_oracle(&MsQueue::new(64), 40);
    sequential_oracle(
        &TwoLock {
            inp: Mutex::new(vec![]),
            out: Mutex::new(vec![]),
        },
        usize::MAX,
    );
    sequential_oracle(&FlatCombining::new(1), usize::MAX);
    sequential_oracle(&Ring::new(16), 16);
    let ring = Ring::new(4);
    for i in 0..4 {
        assert!(ring.try_push(i));
    }
    assert!(!ring.try_push(99), "a full ring rejects");
    assert_eq!(ring.try_pop(), Some(0));
    assert!(ring.try_push(4), "one pop frees exactly one slot");
    println!("  ok");

    println!("== 4 producers + 4 consumers, exactly-once + per-producer FIFO ==");
    const N: u64 = 40_000;
    let ms = MsQueue::new(1 << 18);
    let fc = FlatCombining::new(PRODUCERS + CONSUMERS);
    let tl = TwoLock {
        inp: Mutex::new(vec![]),
        out: Mutex::new(vec![]),
    };
    let rg = Ring::new(1024);
    println!("  {:<24}{:>12}{:>14}", "queue", "seconds", "Mitems/s");
    let queues: [&dyn Queue; 4] = [&ms, &tl, &fc, &rg];
    for q in queues {
        let secs = stress(q, N);
        println!(
            "  {:<24}{:>12.3}{:>14.2}",
            q.name(),
            secs,
            (PRODUCERS as u64 * N) as f64 / secs / 1e6
        );
    }
    println!(
        "  flat combining: {} requests in {} passes ({:.1} per pass)",
        fc.combined.load(Relaxed),
        fc.passes.load(Relaxed),
        fc.combined.load(Relaxed) as f64 / fc.passes.load(Relaxed) as f64
    );

    println!("== MS queue with a tiny arena: heavy node recycling (the ABA stress) ==");
    // Producers are gated to at most ~1000 items ahead of consumers, so 4096 nodes suffice and every node is recycled many times.
    let small = MsQueue::new(4096);
    // (see the `produced > consumed + 1000` gate below)
    let consumed = AtomicUsize::new(0);
    let produced = AtomicUsize::new(0);
    let per = 30_000u64;
    let total = PRODUCERS as u64 * per;
    let logs: Vec<Vec<u64>> = thread::scope(|s| {
        for p in 0..PRODUCERS {
            let (small, consumed, produced) = (&small, &consumed, &produced);
            s.spawn(move || {
                for seq in 1..=per {
                    while produced.load(Relaxed) > consumed.load(Relaxed) + 1000 {
                        thread::yield_now();
                    }
                    produced.fetch_add(1, Relaxed);
                    small.push(p, (p as u64) << 32 | seq);
                }
            });
        }
        let hs: Vec<_> = (0..CONSUMERS)
            .map(|_| {
                let (small, consumed) = (&small, &consumed);
                s.spawn(move || {
                    let mut log = Vec::new();
                    while consumed.load(Relaxed) < total as usize {
                        if let Some(v) = small.pop(0) {
                            consumed.fetch_add(1, Relaxed);
                            log.push(v);
                        }
                    }
                    log
                })
            })
            .collect();
        hs.into_iter().map(|h| h.join().unwrap()).collect()
    });
    let mut count = vec![0u8; total as usize];
    for log in &logs {
        for &v in log {
            let (p, seq) = ((v >> 32) as usize, (v & 0xffff_ffff) as usize);
            count[p * per as usize + seq - 1] += 1;
        }
    }
    assert!(
        count.iter().all(|&c| c == 1),
        "recycled nodes corrupted the queue"
    );
    println!(
        "  {} items through a 4096-node arena, {} node recycles, no loss, no duplicates",
        total,
        small.recycled.load(Relaxed)
    );
    assert!(small.recycled.load(Relaxed) as u64 >= total / 2);
    println!("all queue checks passed");
}
