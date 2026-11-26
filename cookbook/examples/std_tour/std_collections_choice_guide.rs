//! RECIPE 1030 (std): which collection? Ten workloads, each solved with the right structure, with COUNTED comparisons
//! and hashes (asserted) and timings (printed, never asserted).
//! LEVEL: intermediate
//! Use in: choosing data structures in services, code review, interview prep.   Run: cargo run --release --example std_collections_choice_guide
//! COMMON USES: caches and indexes, job queues, schedulers, sorted time indexes, dedupe sets, top-k, tiny inline lists.
//! METHODS: (everyday first) std::vec::Vec, std::collections::HashMap, std::collections::HashSet, std::collections::VecDeque,
//! METHODS: std::collections::BTreeMap, std::collections::BTreeSet, std::collections::BinaryHeap, std::collections::LinkedList,
//! METHODS: HashMap::get, HashMap::entry, HashSet::insert, HashSet::intersection, HashSet::difference, BTreeMap::range, BTreeMap::pop_first,
//! METHODS: BTreeMap::first_key_value, BTreeMap::last_key_value, BTreeSet::union, BTreeSet::intersection, VecDeque::pop_front, BinaryHeap::push,
//! METHODS: BinaryHeap::pop, BinaryHeap::peek, BinaryHeap::peek_mut, BinaryHeap::from, BinaryHeap::into_sorted_vec, LinkedList::append,
//! METHODS: LinkedList::split_off, slice::binary_search, slice::contains
//! DOCS: https://doc.rust-lang.org/std/collections/index.html
//! DEFINED IN: alloc: Vec, VecDeque, LinkedList, BTreeMap, BTreeSet, BinaryHeap. std only: HashMap and HashSet (their default
//! hasher needs OS randomness). Arrays are core.
//! NOT COVERED (and why): the full API of each type (this page is for CHOOSING: see std_vec_vecdeque_methods and the
//! hash map / collections look-up pages), cursor APIs (unstable), concurrent maps (not in std), memory footprint measurement
//! (see std_alloc_layout_global_allocator), external crates such as indexmap or smallvec.
#![allow(dead_code)]
//!
//! CHEAT SHEET (the need -> the structure)
//!   index by position, iterate, append at the end        -> Vec          (the default; contiguous)
//!   FIFO, or push/pop at both ends                       -> VecDeque
//!   look up by exact key, order irrelevant               -> HashMap / HashSet
//!   keys in order, ranges, floor/ceiling, min and max    -> BTreeMap / BTreeSet
//!   repeatedly take the largest (smallest: Reverse)      -> BinaryHeap
//!   build once, then search many times                   -> sorted Vec + binary_search
//!   a handful of small items                             -> Vec or inline array, linear search
//!   move whole lists in O(1)                             -> LinkedList (rarely; try VecDeque or Vec first)
//!
//! COSTS (std::collections module docs; n = size). get(i)/insert(i)/remove(i)
//!   Vec         O(1) / O(n-i) / O(n-i)            VecDeque    O(1) / O(min(i, n-i)) both
//!   LinkedList  O(min(i, n-i)) all three, but append is O(1)
//!   HashMap     O(1) expected get/insert/remove   BTreeMap    O(log n) get/insert/remove, plus range
//!   BinaryHeap  push O(1) expected, pop O(log n), peek O(1) (BinaryHeap docs)
//! BTreeMap detail (its docs): nodes hold B-1..2B-1 items and are searched LINEARLY, so a lookup makes more comparisons
//! than binary search while touching few cache lines.
//!
//! Read: https://doc.rust-lang.org/std/collections/index.html
//! TRY NEXT:
//!   - [S] Change n from 100_000 to 1_000 in workloads 1 and 3. Do the comparison counts change the way big-O predicts?
//!   - [S] Predict the comparison count for LinkedList::contains of a missing key before running.
//!   - [M] Add an 11th workload: an LRU cache from HashMap + VecDeque; count what makes it O(n), then fix the design.
//!   - [M] Find the size where a HashSet beats a linear Vec scan for u64 keys on your machine. Print, do not assert.
//!   - [A] PROJECT: an event store with a HashMap primary index and a BTreeMap time index, kept consistent under random
//!         operations and differential-tested against a Vec model (std_vec_vecdeque_methods, std_cmp_hash_default_traits).
use std::cell::Cell;
use std::cmp::Reverse;
use std::collections::{BTreeMap, BTreeSet, BinaryHeap, HashMap, HashSet, LinkedList, VecDeque};
use std::hash::{Hash, Hasher};
use std::time::Instant;

const COVERED: &[&str] = &[
    "Vec",
    "HashMap",
    "HashSet",
    "VecDeque",
    "BTreeMap",
    "BTreeSet",
    "BinaryHeap",
    "LinkedList",
    "get",
    "entry",
    "insert",
    "intersection",
    "difference",
    "range",
    "pop_first",
    "first_key_value",
    "last_key_value",
    "union",
    "pop_front",
    "push",
    "pop",
    "peek",
    "peek_mut",
    "from",
    "into_sorted_vec",
    "append",
    "split_off",
    "binary_search",
    "contains",
];

thread_local! {
    static COMPARES: Cell<u64> = const { Cell::new(0) };
    static HASHES: Cell<u64> = const { Cell::new(0) };
}

/// A u64 that counts every ==, cmp and hash made on it.
#[derive(Debug, Clone, Copy)]
struct Key(u64);
fn bump(c: &'static std::thread::LocalKey<Cell<u64>>) {
    c.with(|v| v.set(v.get() + 1));
}
impl PartialEq for Key {
    fn eq(&self, o: &Self) -> bool {
        bump(&COMPARES);
        self.0 == o.0
    }
}
impl Eq for Key {}
impl PartialOrd for Key {
    fn partial_cmp(&self, o: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(o))
    }
}
impl Ord for Key {
    fn cmp(&self, o: &Self) -> std::cmp::Ordering {
        bump(&COMPARES);
        self.0.cmp(&o.0)
    }
}
impl Hash for Key {
    fn hash<H: Hasher>(&self, h: &mut H) {
        bump(&HASHES);
        self.0.hash(h);
    }
}

/// Runs f and returns (its result, comparisons made, hashes made). Prints the time for interest only.
fn measure<T>(label: &str, f: impl FnOnce() -> T) -> (T, u64, u64) {
    COMPARES.set(0);
    HASHES.set(0);
    let t = Instant::now();
    let r = f();
    let (c, h) = (COMPARES.get(), HASHES.get());
    println!(
        "   {label:<44} compares={c:<10} hashes={h:<8} {:?}",
        t.elapsed()
    );
    (r, c, h)
}

struct XorShift(u64);
impl XorShift {
    fn below(&mut self, n: u64) -> u64 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        self.0 % n
    }
}

const N: u64 = 100_000;
const Q: u64 = 1_000;

// ---- Workload 1: exact-key lookup ------------------------------------------------------------------------------
fn w1_lookup() {
    println!("1. {Q} exact lookups among {N} keys");
    let vec: Vec<Key> = (0..N).map(Key).collect();
    let hash: HashMap<Key, u64> = (0..N).map(|i| (Key(i), i)).collect();
    let tree: BTreeMap<Key, u64> = (0..N).map(|i| (Key(i), i)).collect();
    let mut rng = XorShift(1);
    let probes: Vec<Key> = (0..Q).map(|_| Key(rng.below(N))).collect();
    let (a, linear, _) = measure("Vec::contains (linear)", || {
        probes.iter().filter(|k| vec.contains(k)).count()
    });
    let (b, binary, _) = measure("sorted Vec::binary_search", || {
        probes
            .iter()
            .filter(|k| vec.binary_search(k).is_ok())
            .count()
    });
    let (c, hashed_cmp, hashes) = measure("HashMap::get", || {
        probes.iter().filter(|k| hash.get(k).is_some()).count()
    });
    let (d, btree, _) = measure("BTreeMap::get", || {
        probes.iter().filter(|k| tree.get(k).is_some()).count()
    });
    assert_eq!(
        (a, b, c, d),
        (Q as usize, Q as usize, Q as usize, Q as usize)
    );
    assert!(
        linear > Q * N / 4,
        "linear search averages about n/2 comparisons"
    );
    assert!(binary <= Q * 18, "about log2(100000) = 17 per probe");
    assert_eq!(hashes, Q, "one hash per lookup");
    assert!(
        hashed_cmp >= Q && hashed_cmp < 3 * Q,
        "about one equality check per hit"
    );
    assert!(
        btree < Q * 100 && btree > binary,
        "logarithmic, with linear search inside nodes"
    );
}

// ---- Workload 2: FIFO queue ------------------------------------------------------------------------------------
fn w2_fifo() {
    println!("2. FIFO of 20000 jobs");
    // Element moves cannot be counted from outside, so only equal behaviour is asserted. By the documented O(n - i) cost,
    // draining with remove(0) shifts n(n-1)/2 (about 200 million) elements; pop_front shifts none. See the timings.
    let n = 20_000u64;
    let mut v: Vec<u64> = (0..n).collect();
    let (order_v, _, _) = measure("Vec::remove(0)", || {
        std::iter::from_fn(|| (!v.is_empty()).then(|| v.remove(0))).collect::<Vec<_>>()
    });
    let mut d: VecDeque<u64> = (0..n).collect();
    let (order_d, _, _) = measure("VecDeque::pop_front", || {
        std::iter::from_fn(|| d.pop_front()).collect::<Vec<_>>()
    });
    assert_eq!(order_v, order_d);
    assert!(order_d.iter().copied().eq(0..n));
}

// ---- Workload 3: range query -----------------------------------------------------------------------------------
fn w3_range() {
    println!("3. range query over {N} timestamps");
    let tree: BTreeMap<Key, ()> = (0..N).map(|i| (Key(i * 2), ())).collect();
    let hash: HashMap<Key, ()> = (0..N).map(|i| (Key(i * 2), ())).collect();
    let (lo, hi) = (Key(1000), Key(1200));
    let (a, tree_cmp, _) = measure("BTreeMap::range", || tree.range(lo..hi).count());
    let (b, hash_cmp, _) = measure("HashMap full scan + filter", || {
        hash.keys().filter(|k| **k >= lo && **k < hi).count()
    });
    assert_eq!((a, b), (100, 100));
    assert!(tree_cmp < 100, "two descents, then walk: {tree_cmp}");
    assert!(
        hash_cmp >= N,
        "a hash map must examine every key: {hash_cmp}"
    );
}

// ---- Workload 4: always take the most urgent -------------------------------------------------------------------
fn w4_priority() {
    println!("4. scheduler: 20000 pushes and pops by priority");
    let mut rng = XorShift(2);
    let items: Vec<u64> = (0..20_000).map(|_| rng.below(1_000_000)).collect();
    let (from_heap, heap_cmp, _) = measure("BinaryHeap<Reverse<_>>", || {
        let mut h = BinaryHeap::new();
        items.iter().for_each(|&x| h.push(Reverse(Key(x))));
        std::iter::from_fn(|| h.pop().map(|Reverse(k)| k.0)).collect::<Vec<_>>()
    });
    let (from_vec, scan_cmp, _) = measure("Vec min scan + swap_remove (2000 only)", || {
        let mut v: Vec<Key> = items[..2000].iter().map(|&x| Key(x)).collect();
        std::iter::from_fn(|| {
            let i = v.iter().enumerate().min_by_key(|(_, k)| **k)?.0;
            Some(v.swap_remove(i).0)
        })
        .collect::<Vec<_>>()
    });
    let mut sorted = items.clone();
    sorted.sort_unstable();
    assert_eq!(from_heap, sorted);
    assert!(from_vec.is_sorted());
    assert!(heap_cmp < 20_000 * 2 * 16, "O(n log n): {heap_cmp}");
    assert!(scan_cmp >= 2000 * 1999 / 2, "O(n^2): {scan_cmp}");
    // peek is free; peek_mut edits the top and re-sifts when the guard drops.
    let mut h = BinaryHeap::from([Key(5), Key(1), Key(9)]); // From = O(n) heapify
    let (top, c, _) = measure("BinaryHeap::peek", || h.peek().map(|k| k.0));
    assert_eq!((top, c), (Some(9), 0));
    if let Some(mut t) = h.peek_mut() {
        *t = Key(0);
    }
    assert_eq!(
        h.into_sorted_vec().iter().map(|k| k.0).collect::<Vec<_>>(),
        [0, 1, 5]
    ); // ascending
}

// ---- Workload 5: top-k of a stream -----------------------------------------------------------------------------
fn w5_top_k() {
    println!("5. largest 10 of a stream of {N}, using O(k) memory");
    let mut rng = XorShift(3);
    let stream: Vec<u64> = (0..N).map(|_| rng.below(u64::MAX)).collect();
    let (top, cmp, _) = measure("min-heap of size k", || {
        let mut h: BinaryHeap<Reverse<Key>> = BinaryHeap::with_capacity(11);
        for &x in &stream {
            if h.len() < 10 {
                h.push(Reverse(Key(x)));
            } else if let Some(mut smallest) = h.peek_mut() {
                if x > smallest.0.0 {
                    *smallest = Reverse(Key(x));
                }
            }
        }
        let mut out: Vec<u64> = h.into_iter().map(|r| r.0.0).collect();
        out.sort_unstable_by(|a, b| b.cmp(a));
        out
    });
    let mut all = stream.clone();
    all.sort_unstable_by(|a, b| b.cmp(a));
    assert_eq!(top, all[..10]);
    assert!(
        cmp < N,
        "most items are rejected without touching the heap order: {cmp}"
    );
}

// ---- Workload 6: dedupe and set algebra ------------------------------------------------------------------------
fn w6_sets() {
    println!("6. dedupe a stream and compare two sets");
    let events = [3u64, 1, 3, 2, 1, 3];
    let mut seen = HashSet::new();
    let firsts: Vec<u64> = events.iter().copied().filter(|e| seen.insert(*e)).collect(); // keeps first-seen order
    assert_eq!(firsts, [3, 1, 2]);
    let a: BTreeSet<u32> = (1..=5).collect();
    let b: BTreeSet<u32> = (4..=8).collect();
    // BTreeSet operations yield sorted output; HashSet output order is arbitrary, so sort before showing it.
    assert!(a.union(&b).copied().eq(1..=8));
    assert!(a.intersection(&b).copied().eq([4, 5]));
    let (ha, hb): (HashSet<u32>, HashSet<u32>) =
        (a.iter().copied().collect(), b.iter().copied().collect());
    let mut only_a: Vec<_> = ha.difference(&hb).copied().collect();
    only_a.sort_unstable();
    assert_eq!((only_a, ha.intersection(&hb).count()), (vec![1, 2, 3], 2));
}

// ---- Workload 7: counting and grouping -------------------------------------------------------------------------
fn w7_group() {
    println!("7. word counts: entry() hashes once per word");
    let words: Vec<Key> = (0..3000u64).map(|i| Key(i % 7)).collect();
    let mut m: HashMap<Key, u32> = HashMap::with_capacity(16);
    let ((), _, h) = measure("HashMap::entry", || {
        words.iter().for_each(|w| *m.entry(*w).or_insert(0) += 1)
    });
    assert_eq!(h, 3000);
    let mut m2: HashMap<Key, u32> = HashMap::with_capacity(16);
    let ((), _, h2) = measure("contains_key + get_mut / insert", || {
        words.iter().for_each(|w| {
            if m2.contains_key(w) {
                *m2.get_mut(w).expect("present") += 1;
            } else {
                m2.insert(*w, 1);
            }
        })
    });
    assert_eq!(m, m2);
    assert!(h2 >= 2 * 3000 - 7, "two hashes per word: {h2}");
}

// ---- Workload 8: a moving set with min and max -----------------------------------------------------------------
fn w8_ordered() {
    println!("8. order book: best bid and best ask under updates");
    let mut book: BTreeMap<u64, u32> = BTreeMap::new();
    for (price, qty) in [(101, 5), (99, 2), (105, 1), (100, 7)] {
        *book.entry(price).or_default() += qty;
    }
    assert_eq!(
        (book.first_key_value(), book.last_key_value()),
        (Some((&99, &2)), Some((&105, &1)))
    );
    assert_eq!(book.pop_first(), Some((99, 2)));
    assert_eq!(book.range(..=103).next_back(), Some((&101, &5))); // best price not above 103
    // A BinaryHeap gives only one end and cannot remove an arbitrary price: that is why a tree is used here.
}

// ---- Workload 9: tiny collections ------------------------------------------------------------------------------
/// Up to 8 pairs stored inline: no heap allocation, linear search.
#[derive(Default)]
struct Inline8 {
    items: [(Key8, u32); 8],
    len: usize,
}
type Key8 = u64;
impl Inline8 {
    /// false means "full": the caller must fall back to a heap collection.
    fn put(&mut self, k: u64, v: u32) -> bool {
        if let Some(slot) = self.items[..self.len].iter_mut().find(|s| s.0 == k) {
            slot.1 = v;
            return true;
        }
        if self.len == self.items.len() {
            return false;
        }
        self.items[self.len] = (k, v);
        self.len += 1;
        true
    }
    fn get(&self, k: u64) -> Option<u32> {
        self.items[..self.len]
            .iter()
            .find(|s| s.0 == k)
            .map(|s| s.1)
    }
}
fn w9_tiny() {
    println!("9. tiny maps: inline array of 8, checked against HashMap");
    let mut rng = XorShift(4);
    for _ in 0..2000 {
        let (mut small, mut model) = (Inline8::default(), HashMap::new());
        for step in 0..20u32 {
            let k = rng.below(12);
            let fits = model.contains_key(&k) || model.len() < 8;
            assert_eq!(small.put(k, step), fits);
            if fits {
                model.insert(k, step);
            }
            assert_eq!(small.get(k), model.get(&k).copied());
        }
    }
}

// ---- Workload 10: splicing whole lists -------------------------------------------------------------------------
fn w10_splice() {
    println!("10. moving whole batches between queues");
    let mut a: LinkedList<Key> = (0..50_000).map(Key).collect();
    let mut b: LinkedList<Key> = (50_000..100_000).map(Key).collect();
    let ((), c, _) = measure("LinkedList::append (relinks, O(1))", || a.append(&mut b));
    assert_eq!((a.len(), b.len(), c), (100_000, 0, 0));
    let (found, c, _) = measure("LinkedList::contains (walks nodes)", || {
        a.contains(&Key(99_999))
    });
    assert!(found && c == 100_000);
    let tail = a.split_off(99_998); // O(min(at, len - at)) walk, then O(1) relink
    assert_eq!(
        tail.iter().map(|k| k.0).collect::<Vec<_>>(),
        [99_998, 99_999]
    );
    // VecDeque::append is O(m) but contiguous: it is usually the better queue unless O(1) splicing dominates.
    let mut x: VecDeque<u64> = (0..3).collect();
    let mut y: VecDeque<u64> = (3..6).collect();
    x.append(&mut y);
    assert!(x.into_iter().eq(0..6));
}

// ---- Production tips and tricks --------------------------------------------------------------------------------
fn section_production_tips() {
    // 1. Start with Vec; change only for a measured or clearly asymptotic reason.
    // 2. Pre-size with with_capacity when the length is known (Vec, VecDeque, HashMap, HashSet, BinaryHeap).
    assert!(HashMap::<u8, u8>::with_capacity(100).capacity() >= 100);
    // 3. Output that must be deterministic comes from a BTreeMap or a sorted Vec, never from HashMap iteration order.
    // 4. Use entry() rather than contains_key + insert (clippy::map_entry).
    // 5. Vec::contains in a loop over another Vec is O(n*m): build a HashSet first.
    // 6. clippy::linkedlist (pedantic) flags LinkedList because Vec or VecDeque is usually better.
    // 7. Every long-lived queue, cache and map needs a bound and an eviction rule.
    // 8. Never mutate the ordering or hash of a key that is inside a collection.
    println!("production tips ok");
}

fn main() {
    let mut seen = HashSet::new();
    assert!(
        COVERED.iter().all(|m| seen.insert(*m)),
        "COVERED has duplicates"
    );
    w1_lookup();
    w2_fifo();
    w3_range();
    w4_priority();
    w5_top_k();
    w6_sets();
    w7_group();
    w8_ordered();
    w9_tiny();
    w10_splice();
    section_production_tips();
    println!("10 workloads, {} names; all asserts passed", COVERED.len());
}
