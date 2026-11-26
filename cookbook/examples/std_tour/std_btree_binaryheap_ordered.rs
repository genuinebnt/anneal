//! RECIPE 1044 (std-collections): ordered collections: `BTreeMap`, `BTreeSet` and `BinaryHeap` by use case.
//! LEVEL: intermediate
//! Use in: time-series and range queries, leaderboards, timers, priority queues, top-k.   Run: cargo run --release --example std_btree_binaryheap_ordered
//! COMMON USES: "latest value at or before t" (`range(..=t).next_back()`), expiring entries in time order, interval lookup, timer queues, Dijkstra, top-k.
//! METHODS: BTreeMap::new, BTreeMap::from, BTreeMap::insert, BTreeMap::get, BTreeMap::get_mut, BTreeMap::remove, BTreeMap::contains_key,
//! BTreeMap::entry, BTreeMap::range, BTreeMap::range_mut, BTreeMap::first_key_value, BTreeMap::last_key_value, BTreeMap::pop_first,
//! BTreeMap::pop_last, BTreeMap::first_entry, BTreeMap::split_off, BTreeMap::append, BTreeMap::extend, BTreeMap::retain, BTreeMap::extract_if,
//! BTreeMap::keys, BTreeMap::values, BTreeMap::into_values, BTreeSet::insert, BTreeSet::contains, BTreeSet::take, BTreeSet::replace,
//! BTreeSet::range, BTreeSet::first, BTreeSet::last, BTreeSet::pop_first, BTreeSet::union, BTreeSet::intersection, BTreeSet::difference,
//! BTreeSet::symmetric_difference, BTreeSet::is_subset, BTreeSet::split_off, BinaryHeap::new, BinaryHeap::with_capacity, BinaryHeap::from,
//! BinaryHeap::push, BinaryHeap::pop, BinaryHeap::peek, BinaryHeap::peek_mut, PeekMut::pop, BinaryHeap::len, BinaryHeap::into_sorted_vec,
//! BinaryHeap::into_vec, BinaryHeap::iter, BinaryHeap::as_slice, BinaryHeap::append, BinaryHeap::extend, BinaryHeap::drain, BinaryHeap::retain,
//! std::cmp::Reverse, std::cmp::Ordering::then_with, Ord, f64::total_cmp, std::ops::Bound
//! DOCS: https://doc.rust-lang.org/std/collections/struct.BTreeMap.html
//! DEFINED IN: alloc: `BTreeMap`, `BTreeSet`, `BinaryHeap` (`alloc::collections`; std re-exports them, so they work in no_std + alloc).
//!   core: `Ord`, `Ordering`, `Reverse` (`core::cmp`), `Bound` (`core::ops`), `f64::total_cmp`.
//! NOT COVERED (and why): BTreeMap cursors (`lower_bound`, `upper_bound`, `CursorMut`), `try_insert`, BTreeSet `entry`/`get_or_insert_with`,
//! `BinaryHeap::{pop_if, drain_sorted, into_iter_sorted}`, `PeekMut::refresh` (all unstable in 1.98.1; `range` with `Bound` replaces most cursor uses).
//! Third-party structures (indexmap, priority-queue with decrease-key, interval trees) are out of scope for a std tour.
//!
//! CHEAT SHEET (I want to ... -> use ...)
//!   iterate keys in sorted order                 -> `BTreeMap` (no extra sort)
//!   last entry at or before k (floor)            -> `m.range(..=k).next_back()`
//!   first entry at or after k (ceiling)          -> `m.range(k..).next()`
//!   strictly after k                             -> `m.range((Bound::Excluded(k), Bound::Unbounded))`
//!   smallest / largest                           -> `first_key_value` / `last_key_value`; remove: `pop_first` / `pop_last`
//!   expire everything older than t               -> `let keep = m.split_off(&t);` (old stays in `m`)  or `extract_if(..t, |_,_| true)`
//!   merge two maps (right wins)                  -> `a.append(&mut b)`
//!   max-priority queue                           -> `BinaryHeap`
//!   min-priority queue                           -> `BinaryHeap<Reverse<T>>`
//!   fix up / replace the top in one sift         -> `peek_mut()` (mutate through it, or `PeekMut::pop`)
//!   top-k of a stream in O(n log k)              -> min-heap of size k: `BinaryHeap<Reverse<T>>`
//!   order floats                                 -> a newtype whose `Ord` calls `total_cmp`
//!   need remove-arbitrary or update-priority     -> `BTreeSet<(priority, id)>`; or heap + lazy deletion
//!
//! PITFALLS: `range` PANICS if start > end (or start == end with both excluded); heap order among EQUAL elements is unspecified (add a sequence number);
//! `BinaryHeap::iter` is NOT sorted; changing a key's order while it is inside a collection (via Cell/atomics) is a logic error (wrong results, not UB).
//! TRY NEXT:
//!   - [S] Remove the `seq` field from `Timer` and write an assertion that would catch two same-time events firing out of order. Why can it pass by luck?
//!   - [S] Predict `m.range(5..5)` and `m.range(6..5)`; assert both behaviours.
//!   - [M] Add `IntervalMap::remove(range)` that trims partially covered intervals; extend the point-wise oracle test to cover it.
//!   - [M] Rewrite the sliding median with `BTreeSet<(i64, usize)>` halves and compare it with the heap version on the same inputs.
//!   - [A] PROJECT: combine with `std_time_instant_duration_systemtime` and `std_sync_primitives_use_cases`: a timer thread that sleeps on a `Condvar` until the heap's earliest deadline, and wakes early when an earlier timer is added.
//! Read: https://doc.rust-lang.org/std/collections/index.html (when to use which, with complexity table) ; https://doc.rust-lang.org/std/collections/struct.BinaryHeap.html
#![allow(dead_code)]

use std::cmp::{Ordering, Reverse};
use std::collections::binary_heap::PeekMut;
use std::collections::{BTreeMap, BTreeSet, BinaryHeap, HashMap};
use std::ops::Bound::{Excluded, Included, Unbounded};

struct Rng(u64);
impl Rng {
    fn below(&mut self, n: u64) -> u64 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        self.0 % n
    }
}

fn panics<R>(f: impl FnOnce() -> R) -> bool {
    let old = std::panic::take_hook();
    std::panic::set_hook(Box::new(|_| {}));
    let r = std::panic::catch_unwind(std::panic::AssertUnwindSafe(f)).is_err();
    std::panic::set_hook(old);
    r
}

// ------------------------------------------------------------ 1. BTreeMap range queries
fn s01_ranges() {
    // price history: effective-from day -> price in cents
    let m = BTreeMap::from([(10, 500), (20, 550), (35, 525)]);
    let price_at = |day: u32| m.range(..=day).next_back().map(|(_, p)| *p);
    assert_eq!(price_at(9), None);
    assert_eq!(
        (price_at(10), price_at(34), price_at(35), price_at(1_000)),
        (Some(500), Some(550), Some(525), Some(525))
    );
    assert_eq!(m.range(21..).next(), Some((&35, &525))); // ceiling
    assert_eq!(m.range((Excluded(20), Unbounded)).next(), Some((&35, &525))); // strictly greater
    assert_eq!(m.range(10..20).count(), 1); // half open
    assert_eq!(m.range(10..=20).count(), 2); // inclusive
    assert_eq!(
        m.range((Included(11), Excluded(35)))
            .map(|(k, _)| *k)
            .collect::<Vec<_>>(),
        [20]
    );
    assert_eq!(
        m.range(..).rev().map(|(k, _)| *k).collect::<Vec<_>>(),
        [35, 20, 10]
    ); // ranges are double-ended
    assert_eq!(m.range(20..20).count(), 0);
    assert!(panics(|| m.range(30..20).count()), "start > end panics");
    assert!(
        panics(|| m.range((Excluded(20), Excluded(20))).count()),
        "equal and both excluded panics"
    );

    // String keys, &str bounds: borrow the range ends. Prefix scan = start at the prefix, stop when it no longer matches.
    let routes: BTreeMap<String, u8> = ["/a", "/api/users", "/api/x", "/apj", "/b"]
        .iter()
        .enumerate()
        .map(|(i, s)| (s.to_string(), i as u8))
        .collect();
    let under: Vec<&str> = routes
        .range::<str, _>((Included("/api/"), Unbounded))
        .map(|(k, _)| k.as_str())
        .take_while(|k| k.starts_with("/api/"))
        .collect();
    assert_eq!(under, ["/api/users", "/api/x"]);

    // range_mut: apply a change to a window of keys
    let mut m2 = m.clone();
    for (_, price) in m2.range_mut(20..) {
        *price += 1;
    }
    assert_eq!(m2.into_values().collect::<Vec<_>>(), [500, 551, 526]);
    println!("1. btreemap ranges ok");
}

// ------------------------------------------------------------ 2. Ends, entry, splitting and merging
fn s02_ends_split_merge() {
    let mut m: BTreeMap<u64, &str> = BTreeMap::new();
    for (k, v) in [(30, "c"), (10, "a"), (20, "b"), (40, "d")] {
        assert_eq!(m.insert(k, v), None);
    }
    assert_eq!(
        m.keys().copied().collect::<Vec<_>>(),
        [10, 20, 30, 40],
        "iteration is sorted, independent of insertion order"
    );
    assert_eq!(m.first_key_value(), Some((&10, &"a")));
    assert_eq!(m.last_key_value(), Some((&40, &"d")));
    assert_eq!(m.pop_first(), Some((10, "a")));
    assert_eq!(m.pop_last(), Some((40, "d")));
    // first_entry: inspect the minimum and remove it only if it qualifies (one search)
    if let Some(e) = m.first_entry() {
        if *e.key() <= 25 {
            assert_eq!(e.remove(), "b");
        }
    }
    assert_eq!((m.get(&30), m.contains_key(&20)), (Some(&"c"), false));
    if let Some(v) = m.get_mut(&30) {
        *v = "C";
    }
    assert_eq!(m.remove(&30), Some("C"));
    assert!(m.is_empty() && m.first_key_value().is_none() && m.pop_first().is_none());

    // entry works as on HashMap
    let mut by_minute: BTreeMap<u64, Vec<&str>> = BTreeMap::new();
    for (t, ev) in [(125, "b"), (61, "a"), (130, "c")] {
        by_minute.entry(t / 60).or_default().push(ev);
    }
    assert_eq!(
        by_minute,
        BTreeMap::from([(1, vec!["a"]), (2, vec!["b", "c"])])
    );

    // split_off(&k): `self` keeps keys < k, the returned map takes keys >= k
    let mut log: BTreeMap<u32, u32> = (1..=10).map(|i| (i, i * i)).collect();
    let recent = log.split_off(&7);
    assert_eq!(log.keys().last(), Some(&6));
    assert_eq!(recent.keys().next(), Some(&7));
    // expiry: drop everything before `cutoff` by keeping the upper half
    let mut recent = recent;
    let kept = recent.split_off(&9);
    assert_eq!((recent.len(), kept.len()), (2, 2));

    // append moves everything; on a duplicate key the VALUE comes from the argument
    let mut defaults = BTreeMap::from([("port", 80), ("workers", 4)]);
    let mut overrides = BTreeMap::from([("port", 8080), ("debug", 1)]);
    defaults.append(&mut overrides);
    assert!(overrides.is_empty());
    assert_eq!(
        defaults,
        BTreeMap::from([("debug", 1), ("port", 8080), ("workers", 4)])
    );
    defaults.extend([("debug", 0)]);
    assert_eq!(defaults["debug"], 0);
    assert_eq!(defaults.values().copied().collect::<Vec<_>>(), [0, 8080, 4]); // values come out in KEY order

    // retain visits in key order; extract_if (1.91) takes a RANGE as well as a predicate and yields what it removes
    let mut m: BTreeMap<u32, u32> = (0..10).map(|i| (i, i)).collect();
    let mut visited = Vec::new();
    m.retain(|k, _| {
        visited.push(*k);
        k % 5 != 0
    });
    assert_eq!(visited, (0..10).collect::<Vec<_>>());
    let removed: Vec<_> = m
        .extract_if(3..8, |k, _| k % 2 == 0)
        .map(|(k, _)| k)
        .collect();
    assert_eq!(removed, [4, 6]);
    assert_eq!(m.keys().copied().collect::<Vec<_>>(), [1, 2, 3, 7, 8, 9]);
    // it is lazy: dropping the iterator early leaves the unvisited elements in place
    assert_eq!(m.extract_if(.., |_, _| true).next(), Some((1, 1)));
    assert_eq!(m.len(), 5);
    println!("2. ends / split / merge ok");
}

// ------------------------------------------------------------ 3. BTreeSet
fn s03_btreeset() {
    let a = BTreeSet::from([1, 3, 5, 7]);
    let b = BTreeSet::from([3, 4, 5]);
    let v = |it: &mut dyn Iterator<Item = &i32>| it.copied().collect::<Vec<_>>();
    // set operations yield SORTED output
    assert_eq!(v(&mut a.union(&b)), [1, 3, 4, 5, 7]);
    assert_eq!(v(&mut a.intersection(&b)), [3, 5]);
    assert_eq!(v(&mut a.difference(&b)), [1, 7]);
    assert_eq!(v(&mut a.symmetric_difference(&b)), [1, 4, 7]);
    assert!(BTreeSet::from([3, 5]).is_subset(&a) && !b.is_subset(&a));
    assert_eq!((a.first(), a.last()), (Some(&1), Some(&7)));
    assert_eq!(a.range(2..6).copied().collect::<Vec<_>>(), [3, 5]);
    let mut s = a.clone();
    assert!(s.insert(2) && !s.insert(2));
    assert!(s.contains(&2));
    assert_eq!(s.pop_first(), Some(1));
    let high = s.split_off(&5);
    assert_eq!(
        (v(&mut s.iter()), v(&mut high.iter())),
        (vec![2, 3], vec![5, 7])
    );

    // replace vs insert: they differ when equal values are distinguishable
    #[derive(Debug)]
    struct ByKey(u32, &'static str);
    impl PartialEq for ByKey {
        fn eq(&self, o: &Self) -> bool {
            self.0 == o.0
        }
    }
    impl Eq for ByKey {}
    impl PartialOrd for ByKey {
        fn partial_cmp(&self, o: &Self) -> Option<Ordering> {
            Some(self.cmp(o))
        }
    }
    impl Ord for ByKey {
        fn cmp(&self, o: &Self) -> Ordering {
            self.0.cmp(&o.0)
        }
    }
    let mut s = BTreeSet::from([ByKey(1, "old")]);
    assert!(!s.insert(ByKey(1, "new")));
    assert_eq!(s.first().unwrap().1, "old", "insert keeps the original");
    assert_eq!(s.replace(ByKey(1, "new")).unwrap().1, "old");
    assert_eq!(s.first().unwrap().1, "new");
    assert_eq!(s.take(&ByKey(1, "")).unwrap().1, "new"); // take returns the stored value
    println!("3. btreeset ok");
}

// ------------------------------------------------------------ 4. Interval map: non-overlapping ranges -> value
/// start -> (end exclusive, value). Invariant: intervals are non-empty and never overlap.
struct IntervalMap<V: Clone>(BTreeMap<u64, (u64, V)>);
impl<V: Clone> IntervalMap<V> {
    fn get(&self, p: u64) -> Option<&V> {
        let (_, (end, v)) = self.0.range(..=p).next_back()?; // the only candidate is the floor
        (p < *end).then_some(v)
    }
    fn cut(&mut self, at: u64) {
        // if an interval strictly contains `at`, split it into two at that point
        if let Some((&s, (e, v))) = self.0.range(..at).next_back() {
            if at < *e {
                let (e, v) = (*e, v.clone());
                self.0.insert(at, (e, v));
                self.0.get_mut(&s).unwrap().0 = at;
            }
        }
    }
    /// Overwrite [start, end) with `v`.
    fn assign(&mut self, start: u64, end: u64, v: V) {
        assert!(start < end);
        self.cut(start);
        self.cut(end);
        let covered: Vec<u64> = self.0.range(start..end).map(|(k, _)| *k).collect();
        covered.iter().for_each(|k| {
            self.0.remove(k);
        });
        self.0.insert(start, (end, v));
    }
}

fn s04_interval_map() {
    let mut im = IntervalMap(BTreeMap::new());
    let mut oracle = [None::<u32>; 100];
    let mut rng = Rng(0x9E37_79B9);
    for step in 0..2_000u32 {
        let a = rng.below(100);
        let b = a + 1 + rng.below((100 - a).min(15));
        im.assign(a, b, step);
        oracle[a as usize..b as usize].fill(Some(step));
        for p in 0..100 {
            assert_eq!(
                im.get(p).copied(),
                oracle[p as usize],
                "step {step} point {p}"
            );
        }
        let spans: Vec<_> = im.0.iter().map(|(s, (e, _))| (*s, *e)).collect();
        assert!(spans.iter().all(|(s, e)| s < e) && spans.windows(2).all(|w| w[0].1 <= w[1].0));
    }
    println!("4. interval map ok (2000 random assignments against a point-wise oracle)");
}

// ------------------------------------------------------------ 5. Leaderboard: two indexes kept in step
#[derive(Default)]
struct Leaderboard {
    score_of: HashMap<u32, u64>,
    ranked: BTreeSet<(Reverse<u64>, u32)>, // best score first, ties by lower id: a TOTAL order, so no entry is ever lost
}
impl Leaderboard {
    fn set(&mut self, user: u32, score: u64) {
        if let Some(old) = self.score_of.insert(user, score) {
            assert!(
                self.ranked.remove(&(Reverse(old), user)),
                "indexes out of step"
            );
        }
        self.ranked.insert((Reverse(score), user));
    }
    fn top(&self, k: usize) -> Vec<(u32, u64)> {
        self.ranked
            .iter()
            .take(k)
            .map(|(Reverse(s), u)| (*u, *s))
            .collect()
    }
    fn rank(&self, user: u32) -> Option<usize> {
        let key = (Reverse(*self.score_of.get(&user)?), user);
        // O(rank): std's B-tree keeps no subtree sizes, so there is no O(log n) rank query
        Some(self.ranked.range(..key).count() + 1)
    }
}

fn s05_leaderboard() {
    let mut lb = Leaderboard::default();
    for (u, s) in [(1, 50), (2, 70), (3, 70), (4, 10)] {
        lb.set(u, s);
    }
    assert_eq!(lb.top(3), [(2, 70), (3, 70), (1, 50)]);
    lb.set(4, 90);
    lb.set(2, 5);
    assert_eq!(lb.top(4), [(4, 90), (3, 70), (1, 50), (2, 5)]);
    assert_eq!(
        (lb.rank(1), lb.rank(2), lb.rank(99)),
        (Some(3), Some(4), None)
    );
    println!("5. leaderboard ok");
}

// ------------------------------------------------------------ 6. BinaryHeap basics
fn s06_heap_basics() {
    let mut h = BinaryHeap::with_capacity(8);
    for x in [3, 9, 1, 9, 4] {
        h.push(x);
    }
    assert_eq!((h.len(), h.peek()), (5, Some(&9))); // MAX-heap
    assert_eq!(
        h.as_slice()[0],
        9,
        "slot 0 is the maximum; the rest of the layout is NOT sorted"
    );
    assert_eq!(h.iter().sum::<i32>(), 26); // iter: arbitrary order
    assert_eq!(
        std::iter::from_fn(|| h.pop()).collect::<Vec<_>>(),
        [9, 9, 4, 3, 1]
    );
    assert_eq!((h.pop(), h.peek()), (None, None));

    // From<Vec> heapifies in O(n); into_sorted_vec is ASCENDING; into_vec is arbitrary order
    let h = BinaryHeap::from(vec![3, 9, 1, 4]);
    assert_eq!(h.clone().into_sorted_vec(), [1, 3, 4, 9]);
    let mut raw = h.into_vec();
    raw.sort();
    assert_eq!(raw, [1, 3, 4, 9]);

    // min-heap with Reverse
    let mut min: BinaryHeap<Reverse<u32>> = [5, 2, 8].into_iter().map(Reverse).collect();
    assert_eq!(min.pop(), Some(Reverse(2)));

    // peek_mut: modify the top in place; the heap re-sifts when the guard drops
    let mut h = BinaryHeap::from([10, 7, 3]);
    if let Some(mut top) = h.peek_mut() {
        *top -= 8;
    }
    assert_eq!(h.peek(), Some(&7));
    // PeekMut::pop: "remove the top only if it qualifies" with one lookup
    if let Some(top) = h.peek_mut() {
        if *top > 5 {
            assert_eq!(PeekMut::pop(top), 7);
        }
    }
    assert_eq!(h.len(), 2);

    let mut other = BinaryHeap::from([100, 0]);
    h.append(&mut other);
    h.extend([50]);
    assert!(other.is_empty());
    h.retain(|x| *x % 2 == 0); // 1.70: bulk cancel, then the heap is rebuilt
    assert_eq!(h.clone().into_sorted_vec(), [0, 2, 50, 100]);
    let mut drained: Vec<_> = h.drain().collect(); // arbitrary order
    drained.sort();
    assert!(h.is_empty() && drained == [0, 2, 50, 100]);
    println!("6. binaryheap basics ok");
}

// ------------------------------------------------------------ 7. Custom Ord: timer queue
#[derive(Debug, PartialEq, Eq)]
struct Timer {
    at: u64,
    seq: u64, // insertion counter: makes equal times FIFO, which the heap alone does not promise
    name: &'static str,
}
impl Ord for Timer {
    fn cmp(&self, other: &Self) -> Ordering {
        // reversed on purpose: the EARLIEST timer must be the greatest. `name` is last so that Ord agrees with Eq.
        other
            .at
            .cmp(&self.at)
            .then_with(|| other.seq.cmp(&self.seq))
            .then_with(|| other.name.cmp(self.name))
    }
}
impl PartialOrd for Timer {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

#[derive(Default)]
struct TimerQueue {
    heap: BinaryHeap<Timer>,
    next_seq: u64,
}
impl TimerQueue {
    fn schedule(&mut self, at: u64, name: &'static str) {
        self.next_seq += 1;
        self.heap.push(Timer {
            at,
            seq: self.next_seq,
            name,
        });
    }
    fn fire_due(&mut self, now: u64) -> Vec<&'static str> {
        let mut out = Vec::new();
        while let Some(top) = self.heap.peek_mut() {
            if top.at > now {
                break;
            }
            out.push(PeekMut::pop(top).name);
        }
        out
    }
}

fn s07_timers() {
    let mut q = TimerQueue::default();
    for (at, n) in [
        (50, "flush"),
        (10, "ping-1"),
        (10, "ping-2"),
        (99, "report"),
        (10, "ping-3"),
    ] {
        q.schedule(at, n);
    }
    assert_eq!(q.fire_due(9), Vec::<&str>::new());
    assert_eq!(q.fire_due(10), ["ping-1", "ping-2", "ping-3"]);
    q.schedule(20, "late-add");
    assert_eq!(q.fire_due(60), ["late-add", "flush"]);
    assert_eq!(q.heap.peek().map(|t| t.at), Some(99));
    println!("7. timer queue ok");
}

// ------------------------------------------------------------ 8. Floats, top-k
#[derive(Debug, Clone, Copy)]
struct TotalF64(f64);
impl Ord for TotalF64 {
    fn cmp(&self, o: &Self) -> Ordering {
        self.0.total_cmp(&o.0) // IEEE totalOrder: -NaN < -inf < .. < -0.0 < 0.0 < .. < inf < NaN
    }
}
impl PartialOrd for TotalF64 {
    fn partial_cmp(&self, o: &Self) -> Option<Ordering> {
        Some(self.cmp(o))
    }
}
impl PartialEq for TotalF64 {
    fn eq(&self, o: &Self) -> bool {
        self.cmp(o) == Ordering::Equal // must agree with Ord, so NOT the derived float ==
    }
}
impl Eq for TotalF64 {}

/// Largest k items, descending. Memory O(k), time O(n log k): the heap's top is the weakest of the current best.
fn top_k<T: Ord>(items: impl IntoIterator<Item = T>, k: usize) -> Vec<T> {
    let mut h: BinaryHeap<Reverse<T>> = BinaryHeap::with_capacity(k);
    for x in items {
        if h.len() < k {
            h.push(Reverse(x));
        } else if let Some(mut weakest) = h.peek_mut() {
            if x > weakest.0 {
                *weakest = Reverse(x); // one sift instead of pop + push
            }
        }
    }
    h.into_sorted_vec()
        .into_iter()
        .map(|Reverse(x)| x)
        .collect() // ascending Reverse = descending T
}

fn s08_floats_topk() {
    let h: BinaryHeap<TotalF64> = [2.5, -1.0, f64::INFINITY, 0.0, -0.0]
        .map(TotalF64)
        .into_iter()
        .collect();
    let asc: Vec<u64> = h.into_sorted_vec().iter().map(|x| x.0.to_bits()).collect();
    assert_eq!(
        asc,
        [-1.0f64, -0.0, 0.0, 2.5, f64::INFINITY].map(f64::to_bits)
    );
    assert_eq!(
        TotalF64(f64::NAN),
        TotalF64(f64::NAN),
        "reflexive, as Eq requires"
    );
    assert!(
        TotalF64(f64::NAN) > TotalF64(f64::INFINITY),
        "a positive NaN sorts above everything: filter NaN first if that is wrong for you"
    );
    assert!(TotalF64(-0.0) < TotalF64(0.0));

    let mut rng = Rng(7);
    for case in 0..500 {
        let n = rng.below(60) as usize;
        let k = rng.below(12) as usize;
        let data: Vec<u64> = (0..n).map(|_| rng.below(40)).collect();
        let mut oracle = data.clone();
        oracle.sort_by(|a, b| b.cmp(a));
        oracle.truncate(k);
        assert_eq!(top_k(data, k), oracle, "case {case} k={k}");
    }
    println!("8. floats and top-k ok");
}

// ------------------------------------------------------------ 9. Dijkstra: lazy deletion instead of decrease-key
fn shortest(adj: &[Vec<(usize, u64)>], src: usize) -> Vec<Option<u64>> {
    let mut dist: Vec<Option<u64>> = vec![None; adj.len()];
    let mut heap = BinaryHeap::from([Reverse((0u64, src))]); // tuples order lexicographically: distance first
    dist[src] = Some(0);
    while let Some(Reverse((d, u))) = heap.pop() {
        if dist[u].is_some_and(|best| d > best) {
            continue; // stale entry: a shorter route was pushed after this one
        }
        for &(v, w) in &adj[u] {
            let nd = d + w;
            if dist[v].is_none_or(|best| nd < best) {
                dist[v] = Some(nd);
                heap.push(Reverse((nd, v)));
            }
        }
    }
    dist
}

fn s09_dijkstra() {
    let mut rng = Rng(11);
    for case in 0..300 {
        let n = 1 + rng.below(12) as usize;
        let mut adj = vec![Vec::new(); n];
        for _ in 0..rng.below(30) {
            let (a, b) = (rng.below(n as u64) as usize, rng.below(n as u64) as usize);
            adj[a].push((b, rng.below(20)));
        }
        // oracle: Bellman-Ford style relaxation until nothing changes
        let mut want: Vec<Option<u64>> = vec![None; n];
        want[0] = Some(0);
        for _ in 0..n {
            for u in 0..n {
                let Some(du) = want[u] else { continue };
                for &(v, w) in &adj[u] {
                    if want[v].is_none_or(|x| du + w < x) {
                        want[v] = Some(du + w);
                    }
                }
            }
        }
        assert_eq!(shortest(&adj, 0), want, "case {case}");
    }
    println!("9. dijkstra with Reverse + stale-entry skip ok");
}

// ------------------------------------------------------------ 10. Sliding-window median: two heaps + lazy deletion
/// `low` is a max-heap of the smaller half, `high` a min-heap of the larger half. Items are (value, index), so all are distinct.
/// An item whose index left the window is dead; it is removed only when it reaches the top.
fn sliding_median(data: &[i64], w: usize) -> Vec<i64> {
    assert!(w % 2 == 1, "odd window keeps the median a single element");
    let mut low: BinaryHeap<(i64, usize)> = BinaryHeap::new();
    let mut high: BinaryHeap<Reverse<(i64, usize)>> = BinaryHeap::new();
    let mut in_low = vec![false; data.len()];
    let (mut live_low, mut live_high) = (0usize, 0usize);
    let mut out = Vec::new();
    for i in 0..data.len() {
        let oldest = (i + 1).saturating_sub(w); // first index still inside the window ending at i
        if i >= w {
            if in_low[i - w] {
                live_low -= 1
            } else {
                live_high -= 1
            }
        }
        while low.peek().is_some_and(|&(_, j)| j < oldest) {
            low.pop();
        }
        // insert into the correct side
        if low.peek().is_none_or(|&top| (data[i], i) <= top) {
            low.push((data[i], i));
            in_low[i] = true;
            live_low += 1;
        } else {
            high.push(Reverse((data[i], i)));
            live_high += 1;
        }
        // rebalance so that low holds ceil(size/2) live items; only ever move LIVE tops
        loop {
            while low.peek().is_some_and(|&(_, j)| j < oldest) {
                low.pop();
            }
            while high.peek().is_some_and(|&Reverse((_, j))| j < oldest) {
                high.pop();
            }
            if live_low > live_high + 1 {
                let (v, j) = low.pop().unwrap();
                high.push(Reverse((v, j)));
                in_low[j] = false;
                (live_low, live_high) = (live_low - 1, live_high + 1);
            } else if live_high > live_low {
                let Reverse((v, j)) = high.pop().unwrap();
                low.push((v, j));
                in_low[j] = true;
                (live_low, live_high) = (live_low + 1, live_high - 1);
            } else {
                break;
            }
        }
        if i + 1 >= w {
            out.push(low.peek().unwrap().0);
        }
    }
    out
}

fn s10_sliding_median() {
    assert_eq!(
        sliding_median(&[1, 3, -1, -3, 5, 3, 6, 7], 3),
        [1, -1, -1, 3, 5, 6]
    );
    let mut rng = Rng(2024);
    for case in 0..1_000 {
        let n = rng.below(50) as usize;
        let w = 1 + 2 * rng.below(5) as usize;
        let data: Vec<i64> = (0..n).map(|_| rng.below(15) as i64 - 7).collect();
        let want: Vec<i64> = data
            .windows(w)
            .map(|s| {
                let mut s = s.to_vec();
                s.sort();
                s[w / 2]
            })
            .collect();
        assert_eq!(sliding_median(&data, w), want, "case {case} w={w} {data:?}");
    }
    println!("10. sliding median ok");
}

// ------------------------------------------------------------ 11. Differential test: BTreeMap vs a sorted Vec
fn s11_differential() {
    let mut rng = Rng(0xABCD);
    let mut m = BTreeMap::new();
    let mut v: Vec<(u64, u64)> = Vec::new();
    for step in 0..20_000u64 {
        let k = rng.below(200);
        let at = v.binary_search_by_key(&k, |p| p.0);
        match rng.below(6) {
            0 | 1 => {
                let old = match at {
                    Ok(i) => Some(std::mem::replace(&mut v[i].1, step)),
                    Err(i) => {
                        v.insert(i, (k, step));
                        None
                    }
                };
                assert_eq!(m.insert(k, step), old);
            }
            2 => assert_eq!(m.remove(&k), at.ok().map(|i| v.remove(i).1)),
            3 => assert_eq!(
                m.range(..=k).next_back().map(|(a, b)| (*a, *b)),
                v.iter().rev().find(|p| p.0 <= k).copied()
            ),
            4 => assert_eq!(m.pop_first(), (!v.is_empty()).then(|| v.remove(0))),
            _ => assert_eq!(m.pop_last(), v.pop()),
        }
        assert_eq!(m.len(), v.len(), "step {step}");
    }
    assert!(m.iter().map(|(a, b)| (*a, *b)).eq(v.iter().copied()));
    println!("11. differential test ok");
}

fn main() {
    s01_ranges();
    s02_ends_split_merge();
    s03_btreeset();
    s04_interval_map();
    s05_leaderboard();
    s06_heap_basics();
    s07_timers();
    s08_floats_topk();
    s09_dijkstra();
    s10_sliding_median();
    s11_differential();
    println!("all ordered-collection sections passed");
}
