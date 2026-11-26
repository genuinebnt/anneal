//! RECIPE 145 (concurrency): lock-free Treiber stack, the ABA problem, and tagged heads that defeat it.
//! LEVEL: advanced
//! Use in: free lists, object pools, lock-free allocators, work-stealing building blocks.   Run: cargo run --release --example lockfree_stack_aba
#![allow(dead_code)]
//!
//! WHAT: a Treiber stack: `head` points at the top node; push links a node to the old head and CASes head; pop reads
//! `head` and `head.next`, then CASes head from `old` to `next`.
//! THE ABA HAZARD: pop reads (head = A, next = B), is descheduled; meanwhile other threads pop A, pop B, and push A
//! back (now A.next = C). The first thread resumes: head is "still A", so its CAS succeeds and installs the STALE
//! `next = B`: B is a node somebody else owns/freed, and C or the re-pushed item is lost. The CAS compared only the
//! value, not the history. Part 1 replays exactly this interleaving by hand, single-threaded, so it is reproducible.
//! THE FIX: pack a generation tag next to the index in ONE `AtomicU64`: `(tag << 32) | index`. Every successful
//! push/pop bumps the tag, so "A again" has a different tag and the stale CAS fails. (The tag can wrap after 2^32
//! updates between one thread's read and its CAS; that is accepted in practice. 128-bit CAS or pointer tagging are alternatives.)
//! WHY INDICES IN AN ARENA: nodes live in one pre-allocated `Vec` and are never freed while the stack lives, so a
//! stale `nodes[i].next` read is a harmless atomic read, not a use-after-free. This whole recipe needs no `unsafe`.
//! A second stack (`free`) of the same type recycles nodes, which also makes ABA very likely; the stress test hammers that.
//! MEMORY RECLAMATION: with real pointers, a popped node cannot be freed immediately: another thread may be about to
//! read `old.next`. Solutions: hazard pointers (readers publish "I am using X"; the freer scans them before
//! freeing), epoch-based reclamation (readers pin an epoch; garbage is freed once all pinned threads moved on;
//! crossbeam-epoch https://github.com/crossbeam-rs/crossbeam), reference counting, or never freeing (an arena, as here).
//! ORDERINGS: successful push CAS is `Release` (publishes value + next), pop CAS is `Acquire` (sees them); a node
//! returned to the free list is Released after its value was read, so the next owner's writes cannot race that read.
//! Check with `cargo +nightly miri run --example lockfree_stack_aba` (reduce the stress size first).
//! COMMON USES: lock-free allocator free lists and object pools, task freelists in runtimes; crossbeam-epoch's
//! docs include a Treiber stack as the canonical example.
//! Read: https://marabos.nl/atomics/ and https://docs.rs/crossbeam-epoch/latest/crossbeam_epoch/ and https://en.wikipedia.org/wiki/ABA_problem
use std::sync::atomic::{AtomicU32, AtomicU64, Ordering::*};
use std::thread;

const NIL: u32 = u32::MAX;

struct Node {
    value: AtomicU64,
    next: AtomicU32,
}

fn pack(tag: u32, idx: u32) -> u64 {
    ((tag as u64) << 32) | idx as u64
}
fn idx_of(w: u64) -> u32 {
    w as u32
}
fn tag_of(w: u64) -> u32 {
    (w >> 32) as u32
}

/// A Treiber stack of node indices. `tagged == false` is the naive (ABA-prone) variant: the tag stays 0.
struct IndexStack {
    head: AtomicU64,
    tagged: bool,
}

impl IndexStack {
    fn new(tagged: bool) -> Self {
        Self {
            head: AtomicU64::new(pack(0, NIL)),
            tagged,
        }
    }
    fn bump(&self, h: u64) -> u32 {
        if self.tagged {
            tag_of(h).wrapping_add(1)
        } else {
            0
        }
    }
    fn push(&self, nodes: &[Node], idx: u32) {
        let mut h = self.head.load(Relaxed); // only its index is used (as a value), never dereferenced
        loop {
            nodes[idx as usize].next.store(idx_of(h), Relaxed);
            match self
                .head
                .compare_exchange_weak(h, pack(self.bump(h), idx), Release, Relaxed)
            {
                Ok(_) => return,
                Err(cur) => h = cur,
            }
        }
    }
    /// First half of pop: snapshot the head word and read `next`. Exposed so the demo can pause between halves.
    fn pop_begin(&self, nodes: &[Node]) -> Option<(u64, u32)> {
        let h = self.head.load(Acquire);
        let i = idx_of(h);
        (i != NIL).then(|| (h, nodes[i as usize].next.load(Relaxed)))
    }
    /// Second half: CAS head from the snapshot to `next`.
    fn pop_commit(&self, snapshot: u64, next: u32) -> bool {
        self.head
            .compare_exchange(snapshot, pack(self.bump(snapshot), next), Acquire, Relaxed)
            .is_ok()
    }
    fn pop(&self, nodes: &[Node]) -> Option<u32> {
        loop {
            let (h, next) = self.pop_begin(nodes)?;
            if self.pop_commit(h, next) {
                return Some(idx_of(h));
            }
        }
    }
    fn reachable(&self, nodes: &[Node]) -> Vec<u32> {
        let mut out = Vec::new();
        let mut i = idx_of(self.head.load(Acquire));
        while i != NIL && out.len() < 16 {
            out.push(i);
            i = nodes[i as usize].next.load(Relaxed);
        }
        out
    }
}

fn nodes_abc() -> Vec<Node> {
    [10, 20, 30]
        .map(|v| Node {
            value: AtomicU64::new(v),
            next: AtomicU32::new(NIL),
        })
        .into()
}

/// Hand-scheduled interleaving of two threads. Stack is A(0) -> B(1) -> C(2).
/// Returns (did T1's stale CAS succeed, resulting reachable list).
fn aba_schedule(tagged: bool) -> (bool, Vec<u32>) {
    let nodes = nodes_abc();
    let st = IndexStack::new(tagged);
    for i in [2, 1, 0] {
        st.push(&nodes, i);
    }
    assert_eq!(st.reachable(&nodes), [0, 1, 2]);
    // T1: starts pop, reads head = A and A.next = B, then is descheduled
    let (snap, next) = st.pop_begin(&nodes).unwrap();
    assert_eq!((idx_of(snap), next), (0, 1));
    // T2: pops A, pops B (B is now owned by T2), pushes A back; A.next is now C
    assert_eq!(st.pop(&nodes), Some(0));
    assert_eq!(st.pop(&nodes), Some(1));
    st.push(&nodes, 0);
    assert_eq!(st.reachable(&nodes), [0, 2]);
    // T1 resumes: head still *looks* like A
    let ok = st.pop_commit(snap, next);
    (ok, st.reachable(&nodes))
}

/// Tagged, arena-backed stack of u64 values with a bounded node pool.
struct Arena {
    nodes: Vec<Node>,
    top: IndexStack,
    free: IndexStack,
}

impl Arena {
    fn new(cap: u32) -> Self {
        let nodes: Vec<Node> = (0..cap)
            .map(|_| Node {
                value: AtomicU64::new(0),
                next: AtomicU32::new(NIL),
            })
            .collect();
        let a = Self {
            nodes,
            top: IndexStack::new(true),
            free: IndexStack::new(true),
        };
        for i in 0..cap {
            a.free.push(&a.nodes, i);
        }
        a
    }
    /// Err(v) when the node pool is exhausted.
    fn push(&self, v: u64) -> Result<(), u64> {
        let Some(i) = self.free.pop(&self.nodes) else {
            return Err(v);
        };
        self.nodes[i as usize].value.store(v, Relaxed); // published by the Release CAS in top.push
        self.top.push(&self.nodes, i);
        Ok(())
    }
    fn pop(&self) -> Option<u64> {
        let i = self.top.pop(&self.nodes)?;
        let v = self.nodes[i as usize].value.load(Relaxed); // we own node i now (Acquire CAS)
        self.free.push(&self.nodes, i); // Release: our read above happens-before the next owner's write
        Some(v)
    }
}

fn stress(round: usize) {
    const THREADS: u64 = 8;
    const PER: u64 = 25_000; // 8 x 25k = 200k items
    let arena = Arena::new(64); // tiny pool => nodes are recycled constantly, ABA opportunities everywhere
    let arena = &arena;
    let mut all: Vec<u64> = thread::scope(|s| {
        let hs: Vec<_> = (0..THREADS)
            .map(|t| {
                s.spawn(move || {
                    let mut got = Vec::new();
                    for i in 0..PER {
                        let mut v = t * PER + i;
                        while let Err(back) = arena.push(v) {
                            v = back;
                            thread::yield_now();
                        }
                        if let Some(x) = arena.pop() {
                            got.push(x);
                        }
                    }
                    got
                })
            })
            .collect();
        hs.into_iter().flat_map(|h| h.join().unwrap()).collect()
    });
    while let Some(x) = arena.pop() {
        all.push(x);
    }
    all.sort_unstable();
    let expected: Vec<u64> = (0..THREADS * PER).collect();
    assert_eq!(all, expected, "items lost or duplicated");
    println!("stress round {round}: 200k items, exact multiset equality, no loss or duplication");
}

fn main() {
    let (ok, list) = aba_schedule(false);
    println!(
        "naive : stale CAS succeeded={ok}, stack now {list:?} (expected [0, 2]: A re-pushed, then C)"
    );
    assert!(ok, "naive CAS is fooled");
    assert_eq!(
        list,
        [1, 2],
        "head points at B, which T2 owns; the re-pushed A is lost: corruption"
    );

    let (ok, list) = aba_schedule(true);
    println!("tagged: stale CAS succeeded={ok}, stack still {list:?}");
    assert!(!ok, "tag mismatch rejects the stale CAS");
    assert_eq!(list, [0, 2]);

    for round in 0..3 {
        stress(round);
    }
    println!("all assertions passed");
}
