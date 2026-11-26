//! RECIPE 831 (algorithms): greedy algorithms and interval problems, each with its proof technique AND a counterexample where the tempting greedy fails.
//! LEVEL: intermediate
//! Use in: schedulers, calendars and booking systems, compression, network design, resource allocation, interview "intervals" and "greedy" tags.   Run: cargo run --release --example greedy_and_interval_algorithms
//! COMMON USES: meeting-room and CPU/job scheduling, calendar double-booking checks (booking systems), merging time ranges and log windows, Huffman/prefix codes (DEFLATE, JPEG),
//! minimum spanning trees in network layout and clustering, shortest paths (routing), set-cover style test-suite minimisation, rendering skylines and union-of-rectangles (GIS, UI damage regions).
#![allow(dead_code)]
//!
//! WHAT: a greedy algorithm makes the locally best choice and never revisits it. That is only correct when you can PROVE it. Two proof templates cover almost everything:
//!   STAYS AHEAD: show by induction that after each step the greedy partial solution is at least as good as any other partial solution (interval scheduling by earliest finish:
//!                after k picks greedy's k-th finish time is <= OPT's k-th finish time, so greedy can fit at least as many).
//!   EXCHANGE:    take any optimal solution and swap its first differing choice for greedy's without making it worse; repeat until it IS greedy's (Huffman, EDF scheduling, Kruskal, fractional knapsack).
//! Every algorithm below is checked against a brute-force or DP optimum on hundreds to thousands of seeded random instances, and for each classic trap the plausible-but-wrong greedy is run
//! on the same random instances so the program FINDS a counterexample (and asserts that the correct greedy never loses). Seeing the failure is the point: "greedy works here" is a theorem, not a feeling.
//! Matroid view: when feasible sets form a matroid (forests of a graph, schedulable unit-time jobs), the plain "sort by weight, add if still feasible" greedy is optimal; that is why Kruskal and
//! job sequencing work while 0/1 knapsack (not a matroid) needs DP.
//! Conventions: intervals are HALF-OPEN [s, e) unless a section says closed; [1,2) and [2,3) do not overlap. Mixing the two conventions is the commonest interval bug, section 3 tests both.
//! Related files: examples/algorithms/bit_tricks.rs is unrelated; see examples/interview/intervals_greedy_bit_questions.rs for the interview-phrased versions of these problems,
//! examples/algorithms/heuristics_zoo.rs for approximation ratios (set cover appears there too; here it is checked against the exact optimum with the H(d) bound asserted),
//! examples/interview/stacks_queues_heaps_questions.rs (Q19 meeting rooms II, Q18 task scheduler) for the heap-centred writeups.
//! Read: Kleinberg and Tardos, Algorithm Design, chapter 4 (greedy): https://www.cs.princeton.edu/~wayne/kleinberg-tardos/pearson/04GreedyAlgorithms-2x2.pdf
//! Jeff Erickson, Algorithms, chapter 4 (greedy): https://jeffe.cs.illinois.edu/teaching/algorithms/book/04-greedy.pdf   https://en.wikipedia.org/wiki/Matroid   https://en.wikipedia.org/wiki/Greedy_algorithm
//! (Kozen and Zaks on canonical coin systems, and Chvatal's greedy set-cover bound, are cited by title only: not opened here.)
//!
//! TRY NEXT:
//!   - [S] In `earliest_finish`, change the sort key from end time to start time and run section 1: how many of the 1000 random instances lose, and what is the smallest losing instance (print it)?
//!   - [S] Predict, then measure: in `is_canonical`, how often does a random coin system of 3-5 coins (always containing 1) turn out NON-canonical? Count over 10,000 systems and compare with your guess.
//!   - [M] Make `merge_intervals` generic over a `Closed`/`HalfOpen` marker type (zero-sized struct implementing a trait with `overlaps`) so the two conventions cannot be mixed by accident; keep the union-find oracle.
//!   - [M] Add a failure mode to `dijkstra`: give `allow_reopen = true` a graph with a negative CYCLE and count how many relaxations happen before you cap it; then detect the cycle with `bellman_ford` and report it.
//!   - [A] PROJECT: build a booking service: `MyCalendar` from section 5 as the core, keep the bookings in a skip-list or B-tree from examples/data_structures (pick the one you like), shard rooms with `schedule_rooms`, and fuzz it against the naive list oracle with 100k random requests.

use std::cmp::Reverse;
use std::collections::{BTreeMap, BinaryHeap, HashMap, HashSet};

// =====================================================================================================================
// Plumbing.
// =====================================================================================================================
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
    fn range(&mut self, lo: i64, hi: i64) -> i64 {
        lo + (self.next() % (hi - lo + 1) as u64) as i64
    }
    fn shuffle<T>(&mut self, v: &mut [T]) {
        for i in (1..v.len()).rev() {
            v.swap(i, self.below(i as u64 + 1) as usize);
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
struct Iv {
    s: i32,
    e: i32,
}
fn iv(s: i32, e: i32) -> Iv {
    Iv { s, e }
}
/// Half-open overlap: [1,2) and [2,3) do NOT overlap.
fn overlaps_open(a: Iv, b: Iv) -> bool {
    a.s < b.e && b.s < a.e
}
/// Closed overlap: [1,2] and [2,3] share the point 2.
fn overlaps_closed(a: Iv, b: Iv) -> bool {
    a.s <= b.e && b.s <= a.e
}
fn rand_ivs(r: &mut Rng, n: usize, max_start: i64, max_len: i64, min_len: i64) -> Vec<Iv> {
    (0..n)
        .map(|_| {
            let s = r.range(0, max_start) as i32;
            iv(s, s + r.range(min_len, max_len) as i32)
        })
        .collect()
}

struct Dsu {
    p: Vec<usize>,
}
impl Dsu {
    fn new(n: usize) -> Self {
        Dsu {
            p: (0..n).collect(),
        }
    }
    fn find(&mut self, x: usize) -> usize {
        if self.p[x] != x {
            let root = self.find(self.p[x]);
            self.p[x] = root; // path compression
        }
        self.p[x]
    }
    fn union(&mut self, a: usize, b: usize) -> bool {
        let (a, b) = (self.find(a), self.find(b));
        if a == b {
            return false;
        }
        self.p[a] = b;
        true
    }
}

// =====================================================================================================================
// 1. Interval scheduling (activity selection): maximum number of pairwise non-overlapping intervals.
//    PROOF (stays ahead): let greedy pick g1 < g2 < ... and OPT pick o1 < o2 < ...; by induction finish(g_k) <= finish(o_k), because g_k is the earliest-finishing interval
//    compatible with g_{k-1}, and o_k is compatible with o_{k-1}, which finishes no earlier. So greedy never runs out of candidates before OPT does.
//    COUNTEREXAMPLES: earliest START (one long interval blocks many), SHORTEST first (a short one straddles two long compatible ones). Weighted version: greedy by weight fails, DP wins.
// =====================================================================================================================
fn earliest_finish(v: &[Iv]) -> usize {
    let mut v = v.to_vec();
    v.sort_by_key(|x| (x.e, x.s));
    let mut last_end = i32::MIN;
    let mut n = 0;
    for x in v {
        if x.s >= last_end {
            last_end = x.e;
            n += 1;
        }
    }
    n
}
fn earliest_start(v: &[Iv]) -> usize {
    let mut v = v.to_vec();
    v.sort_by_key(|x| (x.s, x.e));
    let mut last_end = i32::MIN;
    let mut n = 0;
    for x in v {
        if x.s >= last_end {
            last_end = x.e;
            n += 1;
        }
    }
    n
}
fn shortest_first(v: &[Iv]) -> usize {
    let mut v = v.to_vec();
    v.sort_by_key(|x| (x.e - x.s, x.s));
    let mut chosen: Vec<Iv> = Vec::new();
    for x in v {
        if chosen.iter().all(|&c| !overlaps_open(c, x)) {
            chosen.push(x);
        }
    }
    chosen.len()
}
/// Brute force over all subsets (n <= 10).
fn max_nonoverlapping_brute(v: &[Iv]) -> usize {
    let n = v.len();
    (0u32..1 << n)
        .filter(|&m| {
            let sel: Vec<Iv> = (0..n).filter(|&i| m >> i & 1 == 1).map(|i| v[i]).collect();
            sel.iter()
                .enumerate()
                .all(|(i, &a)| sel[i + 1..].iter().all(|&b| !overlaps_open(a, b)))
        })
        .map(|m| m.count_ones() as usize)
        .max()
        .unwrap()
}

#[derive(Clone, Copy, Debug)]
struct Wiv {
    s: i32,
    e: i32,
    w: i64,
}
/// Weighted interval scheduling: sort by end; dp[i] = best using the first i intervals; p(i) = how many intervals end at or before s_i (binary search).
fn weighted_dp(v: &[Wiv]) -> i64 {
    let mut v = v.to_vec();
    v.sort_by_key(|x| x.e);
    let ends: Vec<i32> = v.iter().map(|x| x.e).collect();
    let mut dp = vec![0i64; v.len() + 1];
    for (i, x) in v.iter().enumerate() {
        let p = ends.partition_point(|&e| e <= x.s); // intervals 0..p all end by x.s (half-open: touching is fine)
        dp[i + 1] = dp[i].max(x.w + dp[p.min(i)]);
    }
    dp[v.len()]
}
fn weighted_greedy_by_weight(v: &[Wiv]) -> i64 {
    let mut v = v.to_vec();
    v.sort_by_key(|x| Reverse(x.w));
    let mut chosen: Vec<Wiv> = Vec::new();
    for x in v {
        if chosen
            .iter()
            .all(|c| !overlaps_open(iv(c.s, c.e), iv(x.s, x.e)))
        {
            chosen.push(x);
        }
    }
    chosen.iter().map(|c| c.w).sum()
}
fn weighted_brute(v: &[Wiv]) -> i64 {
    let n = v.len();
    (0u32..1 << n)
        .filter_map(|m| {
            let sel: Vec<Wiv> = (0..n).filter(|&i| m >> i & 1 == 1).map(|i| v[i]).collect();
            sel.iter()
                .enumerate()
                .all(|(i, a)| {
                    sel[i + 1..]
                        .iter()
                        .all(|b| !overlaps_open(iv(a.s, a.e), iv(b.s, b.e)))
                })
                .then(|| sel.iter().map(|x| x.w).sum())
        })
        .max()
        .unwrap()
}

fn section1_scheduling() {
    let mut r = Rng(0x5C4ED);
    let (mut lose_start, mut lose_short, mut lose_weight) = (0, 0, 0);
    let mut example_start = None;
    for _ in 0..1500 {
        let n = 1 + r.below(9) as usize;
        let v = rand_ivs(&mut r, n, 14, 7, 1);
        let opt = max_nonoverlapping_brute(&v);
        assert_eq!(earliest_finish(&v), opt, "{v:?}");
        if earliest_start(&v) < opt {
            lose_start += 1;
            example_start.get_or_insert_with(|| v.clone());
        }
        assert!(earliest_start(&v) <= opt && shortest_first(&v) <= opt);
        if shortest_first(&v) < opt {
            lose_short += 1;
        }
    }
    assert!(
        lose_start > 0 && lose_short > 0,
        "both tempting greedies must be shown to fail"
    );
    // Weighted: DP equals brute force; greedy by weight loses on some instances.
    for _ in 0..1000 {
        let n = 1 + r.below(9) as usize;
        let v: Vec<Wiv> = rand_ivs(&mut r, n, 12, 8, 1)
            .into_iter()
            .map(|x| Wiv {
                s: x.s,
                e: x.e,
                w: r.range(1, 20),
            })
            .collect();
        let opt = weighted_brute(&v);
        assert_eq!(weighted_dp(&v), opt);
        assert!(weighted_greedy_by_weight(&v) <= opt);
        if weighted_greedy_by_weight(&v) < opt {
            lose_weight += 1;
        }
    }
    assert!(lose_weight > 0);
    // Hand-made minimal counterexample: the heaviest interval blocks two that sum to more.
    let hand = [
        Wiv { s: 0, e: 10, w: 10 },
        Wiv { s: 0, e: 5, w: 6 },
        Wiv { s: 5, e: 10, w: 6 },
    ];
    assert_eq!(
        (weighted_greedy_by_weight(&hand), weighted_dp(&hand)),
        (10, 12)
    );
    println!(
        "1. scheduling: earliest-finish optimal on 1500 instances; earliest-start lost {lose_start}x, shortest-first {lose_short}x; weight-greedy lost {lose_weight}/1000 to DP; e.g. {:?}",
        example_start.unwrap()
    );
}

// =====================================================================================================================
// 2. Interval partitioning / minimum meeting rooms. Answer = maximum depth (most intervals alive at one instant).
//    PROOF: depth is a lower bound (they all need different rooms at that instant); processing by start time and giving each interval any free room never needs more than depth rooms,
//    because when interval k starts and finds all rooms busy, those rooms plus k are alive at once, so depth >= rooms used.
// =====================================================================================================================
fn rooms_sweep(v: &[Iv]) -> usize {
    // Events: (time, delta). Sorting puts -1 (end) before +1 (start) at equal time, which is exactly the half-open rule.
    let mut ev: Vec<(i32, i32)> = v.iter().flat_map(|x| [(x.s, 1), (x.e, -1)]).collect();
    ev.sort();
    let (mut cur, mut best) = (0, 0);
    for (_, d) in ev {
        cur += d;
        best = best.max(cur);
    }
    best as usize
}
/// Heap version that also produces the assignment: min-heap of (end time, room id).
fn schedule_rooms(v: &[Iv]) -> Vec<usize> {
    let mut order: Vec<usize> = (0..v.len()).collect();
    order.sort_by_key(|&i| v[i].s);
    let mut heap: BinaryHeap<Reverse<(i32, usize)>> = BinaryHeap::new();
    let mut room_of = vec![0; v.len()];
    let mut rooms = 0;
    for i in order {
        let room = match heap.peek() {
            Some(&Reverse((end, room))) if end <= v[i].s => {
                heap.pop();
                room
            }
            _ => {
                rooms += 1;
                rooms - 1
            }
        };
        room_of[i] = room;
        heap.push(Reverse((v[i].e, room)));
    }
    room_of
}
fn depth_brute(v: &[Iv]) -> usize {
    v.iter()
        .map(|p| v.iter().filter(|x| x.s <= p.s && p.s < x.e).count())
        .max()
        .unwrap_or(0)
}

fn section2_rooms() {
    let mut r = Rng(0x600D);
    for _ in 0..1500 {
        let n = r.below(40) as usize;
        let v = rand_ivs(&mut r, n, 30, 10, 1);
        let depth = depth_brute(&v);
        assert_eq!(rooms_sweep(&v), depth);
        let assign = schedule_rooms(&v);
        let used = assign.iter().max().map_or(0, |m| m + 1);
        assert_eq!(used, depth, "heap schedule uses exactly depth rooms");
        for i in 0..v.len() {
            for j in i + 1..v.len() {
                assert!(
                    assign[i] != assign[j] || !overlaps_open(v[i], v[j]),
                    "two overlapping meetings share a room"
                );
            }
        }
    }
    assert_eq!(
        rooms_sweep(&[iv(0, 5), iv(5, 10)]),
        1,
        "touching half-open intervals share a room"
    );
    println!(
        "2. rooms: sweep and heap agree with max-depth brute force; assignments conflict-free on 1500 instances"
    );
}

// =====================================================================================================================
// 3. Merge, insert, intersect. `touching` selects the convention: true = closed intervals (or half-open you want coalesced),
//    false = half-open and touching intervals stay separate.
// =====================================================================================================================
fn merge_intervals(v: &[Iv], touching: bool) -> Vec<Iv> {
    let mut v = v.to_vec();
    v.sort(); // by start, then end. Never subtract in a comparator (i32 overflow): derive Ord or use sort_by_key.
    let mut out: Vec<Iv> = Vec::new();
    for x in v {
        match out.last_mut() {
            Some(last) if x.s < last.e || (touching && x.s == last.e) => last.e = last.e.max(x.e),
            _ => out.push(x),
        }
    }
    out
}
/// Oracle for merge: connected components of the "overlaps" graph, found with union-find over all O(n^2) pairs.
fn merge_oracle(v: &[Iv], touching: bool) -> Vec<Iv> {
    let mut d = Dsu::new(v.len());
    for i in 0..v.len() {
        for j in i + 1..v.len() {
            if if touching {
                overlaps_closed(v[i], v[j])
            } else {
                overlaps_open(v[i], v[j])
            } {
                d.union(i, j);
            }
        }
    }
    let mut comp: HashMap<usize, Iv> = HashMap::new();
    for i in 0..v.len() {
        let root = d.find(i);
        let e = comp.entry(root).or_insert(v[i]);
        e.s = e.s.min(v[i].s);
        e.e = e.e.max(v[i].e);
    }
    let mut out: Vec<Iv> = comp.into_values().collect();
    out.sort();
    out
}
/// Insert into a sorted disjoint list (closed, merges touching): copy the left part, absorb overlaps, copy the right part. O(n) and no re-sort.
fn insert_interval(list: &[Iv], new: Iv) -> Vec<Iv> {
    let mut out = Vec::with_capacity(list.len() + 1);
    let mut cur = new;
    let mut i = 0;
    while i < list.len() && list[i].e < cur.s {
        out.push(list[i]);
        i += 1;
    }
    while i < list.len() && list[i].s <= cur.e {
        cur = iv(cur.s.min(list[i].s), cur.e.max(list[i].e));
        i += 1;
    }
    out.push(cur);
    out.extend_from_slice(&list[i..]);
    out
}
/// Intersection of two sorted disjoint CLOSED lists: two pointers, advance the one that ends first.
fn intersect_lists(a: &[Iv], b: &[Iv]) -> Vec<Iv> {
    let (mut i, mut j, mut out) = (0, 0, Vec::new());
    while i < a.len() && j < b.len() {
        let (lo, hi) = (a[i].s.max(b[j].s), a[i].e.min(b[j].e));
        if lo <= hi {
            out.push(iv(lo, hi));
        }
        if a[i].e < b[j].e { i += 1 } else { j += 1 }
    }
    out
}
/// Length of the union of 1-D intervals: sum of merged lengths. Oracle: count covered unit cells.
fn union_length(v: &[Iv]) -> i64 {
    merge_intervals(v, false)
        .iter()
        .map(|x| (x.e - x.s) as i64)
        .sum()
}

fn section3_merge() {
    let mut r = Rng(0x3E46E);
    for _ in 0..1500 {
        let n = r.below(14) as usize;
        let closed = rand_ivs(&mut r, n, 25, 6, 0); // zero-length allowed: closed point intervals
        assert_eq!(merge_intervals(&closed, true), merge_oracle(&closed, true));
        let open = rand_ivs(&mut r, n, 25, 6, 1);
        assert_eq!(merge_intervals(&open, false), merge_oracle(&open, false));
        let cells: HashSet<i32> = open.iter().flat_map(|x| x.s..x.e).collect();
        assert_eq!(union_length(&open), cells.len() as i64);
        // insert: equals re-merging everything.
        let base = merge_intervals(&closed, true);
        let new = rand_ivs(&mut r, 1, 25, 6, 0)[0];
        let mut all = base.clone();
        all.push(new);
        assert_eq!(insert_interval(&base, new), merge_intervals(&all, true));
        // intersection vs all pairs.
        let other = merge_intervals(&rand_ivs(&mut r, n, 25, 6, 0), true);
        let mut brute: Vec<Iv> = base
            .iter()
            .flat_map(|&a| {
                other.iter().filter_map(move |&b| {
                    let (lo, hi) = (a.s.max(b.s), a.e.min(b.e));
                    (lo <= hi).then(|| iv(lo, hi))
                })
            })
            .collect();
        brute.sort();
        assert_eq!(intersect_lists(&base, &other), brute);
    }
    // The convention matters: the same input, two different correct answers.
    let touch = [iv(1, 2), iv(2, 3)];
    assert_eq!(merge_intervals(&touch, true), [iv(1, 3)]);
    assert_eq!(merge_intervals(&touch, false), [iv(1, 2), iv(2, 3)]);
    assert_eq!(merge_intervals(&[], true), []);
    assert_eq!(
        merge_intervals(&[iv(1, 10), iv(2, 3), iv(4, 5)], false),
        [iv(1, 10)],
        "a contained interval must not shrink the end (use max)"
    );
    // The comparator trap: `a.s - b.s` overflows for extreme coordinates; Ord on the struct (or cmp) does not.
    let extreme = [iv(i32::MAX - 1, i32::MAX), iv(i32::MIN, i32::MIN + 1)];
    assert_eq!(
        merge_intervals(&extreme, false),
        [iv(i32::MIN, i32::MIN + 1), iv(i32::MAX - 1, i32::MAX)]
    );
    println!(
        "3. merge/insert/intersect verified against union-find components and all-pairs on 1500 closed and half-open instances"
    );
}

// =====================================================================================================================
// 4. Stabbing: minimum points hitting every CLOSED interval (= minimum arrows for balloons).
//    PROOF (exchange): take the interval with the smallest right end e*. Any solution must hit it with some point p <= e*. Moving p to e* still hits everything p hit that
//    OPT needs (each such interval has right end >= e* and left end <= p <= e*), so some optimal solution uses e*. Remove all intervals hit by e*, recurse.
// =====================================================================================================================
fn min_points(v: &[Iv]) -> usize {
    let mut v = v.to_vec();
    v.sort_by_key(|x| x.e);
    let (mut last, mut n) = (None::<i32>, 0);
    for x in v {
        if last.is_none_or(|p| p < x.s) {
            last = Some(x.e);
            n += 1;
        }
    }
    n
}
/// Oracle: coordinates are 0..=12; try every subset of those 13 candidate points.
fn min_points_brute(v: &[Iv]) -> usize {
    (0u32..1 << 13)
        .filter(|&m| {
            v.iter()
                .all(|x| m & ((1u32 << (x.e + 1)) - (1 << x.s)) != 0)
        })
        .map(|m| m.count_ones() as usize)
        .min()
        .unwrap()
}
fn section4_stabbing() {
    let mut r = Rng(0xA880);
    for _ in 0..1500 {
        let n = 1 + r.below(10) as usize;
        let v: Vec<Iv> = (0..n)
            .map(|_| {
                let s = r.range(0, 12) as i32;
                iv(s, (s + r.range(0, 5) as i32).min(12))
            })
            .collect();
        assert_eq!(min_points(&v), min_points_brute(&v), "{v:?}");
    }
    // Balloons that only touch at a point share an arrow (closed), unlike rooms where touching meetings are separate.
    assert_eq!(min_points(&[iv(1, 2), iv(2, 3), iv(3, 4), iv(4, 5)]), 2);
    // Extreme coordinates: sorting by `end` with key (not a subtraction comparator) is safe.
    assert_eq!(
        min_points(&[iv(i32::MIN, i32::MAX), iv(i32::MAX - 1, i32::MAX)]),
        1
    );
    println!("4. stabbing: min arrows equals brute force on 1500 instances");
}

// =====================================================================================================================
// 5. Calendar booking. My Calendar I: no double booking (BTreeMap neighbour check). II: no TRIPLE booking. III: report the max overlap after each booking.
// =====================================================================================================================
#[derive(Default)]
struct MyCalendar1 {
    by_start: BTreeMap<i32, i32>, // start -> end, always pairwise disjoint
}
impl MyCalendar1 {
    fn book(&mut self, s: i32, e: i32) -> bool {
        // Only two neighbours can conflict in a disjoint set: the last booking starting at or before s, and the first starting after s.
        if let Some((_, &pe)) = self.by_start.range(..=s).next_back() {
            if pe > s {
                return false;
            }
        }
        if let Some((&ns, _)) = self.by_start.range(s..).next() {
            if ns < e {
                return false;
            }
        }
        self.by_start.insert(s, e);
        true
    }
}
#[derive(Default)]
struct MyCalendar2 {
    singles: Vec<Iv>,
    doubles: Vec<Iv>,
}
impl MyCalendar2 {
    fn book(&mut self, s: i32, e: i32) -> bool {
        let new = iv(s, e);
        if self.doubles.iter().any(|&d| overlaps_open(d, new)) {
            return false;
        }
        for &x in &self.singles {
            if overlaps_open(x, new) {
                self.doubles.push(iv(x.s.max(s), x.e.min(e)));
            }
        }
        self.singles.push(new);
        true
    }
}
#[derive(Default)]
struct MyCalendar3 {
    delta: BTreeMap<i32, i32>,
}
impl MyCalendar3 {
    fn book(&mut self, s: i32, e: i32) -> usize {
        *self.delta.entry(s).or_default() += 1;
        *self.delta.entry(e).or_default() -= 1;
        let (mut cur, mut best) = (0, 0);
        for d in self.delta.values() {
            cur += d; // BTreeMap iterates in key order; at one key start and end deltas are already netted, which is the half-open rule
            best = best.max(cur);
        }
        best as usize
    }
}
fn section5_calendar() {
    let mut r = Rng(0xCA1);
    for _ in 0..400 {
        let (mut c1, mut c2, mut c3) = (
            MyCalendar1::default(),
            MyCalendar2::default(),
            MyCalendar3::default(),
        );
        let (mut list1, mut list2, mut list3): (Vec<Iv>, Vec<Iv>, Vec<Iv>) =
            (vec![], vec![], vec![]);
        for _ in 0..40 {
            let s = r.range(0, 40) as i32;
            let e = s + r.range(1, 8) as i32;
            // I: naive list check
            let ok1 = list1.iter().all(|&x| !overlaps_open(x, iv(s, e)));
            assert_eq!(c1.book(s, e), ok1);
            if ok1 {
                list1.push(iv(s, e));
            }
            // II: allowed iff max depth stays <= 2
            let mut trial = list2.clone();
            trial.push(iv(s, e));
            let ok2 = depth_brute(&trial) <= 2;
            assert_eq!(c2.book(s, e), ok2);
            if ok2 {
                list2.push(iv(s, e));
            }
            // III: max depth of everything booked so far
            list3.push(iv(s, e));
            assert_eq!(c3.book(s, e), depth_brute(&list3));
        }
    }
    let mut c = MyCalendar1::default();
    assert!(
        c.book(10, 20) && !c.book(15, 25) && c.book(20, 30),
        "touching bookings are fine"
    );
    println!("5. calendars I/II/III match naive recomputation on 400 sessions of 40 bookings");
}

// =====================================================================================================================
// 6. Sweep lines: skyline (max-heap with lazy deletion) and union area of rectangles (compressed-y coverage counts).
// =====================================================================================================================
fn skyline(b: &[(i32, i32, i32)]) -> Vec<(i32, i32)> {
    let mut b = b.to_vec();
    b.sort(); // by left edge
    let mut xs: Vec<i32> = b.iter().flat_map(|&(l, r, _)| [l, r]).collect();
    xs.sort();
    xs.dedup();
    let mut heap: BinaryHeap<(i32, i32)> = BinaryHeap::new(); // (height, right edge)
    let (mut i, mut out, mut last_h) = (0, Vec::new(), 0);
    for x in xs {
        while i < b.len() && b[i].0 <= x {
            heap.push((b[i].2, b[i].1));
            i += 1;
        }
        // Lazy deletion: a building is removed only when it reaches the top and has already ended. Cheaper than a multiset with O(log n) removals.
        while heap.peek().is_some_and(|&(_, right)| right <= x) {
            heap.pop();
        }
        let h = heap.peek().map_or(0, |&(h, _)| h);
        if h != last_h {
            out.push((x, h));
            last_h = h;
        }
    }
    out
}
fn skyline_oracle(b: &[(i32, i32, i32)]) -> Vec<(i32, i32)> {
    let height = |x: i32| {
        b.iter()
            .filter(|&&(l, r, _)| l <= x && x < r)
            .map(|&(_, _, h)| h)
            .max()
            .unwrap_or(0)
    };
    let mut out = Vec::new();
    let mut prev = 0;
    for x in 0..=40 {
        let h = height(x);
        if h != prev {
            out.push((x, h));
            prev = h;
        }
    }
    out
}
fn union_area(rects: &[(i32, i32, i32, i32)]) -> i64 {
    // (x1, y1, x2, y2). Events at x; between consecutive event x's the covered y-length is constant.
    let mut ys: Vec<i32> = rects.iter().flat_map(|&(_, y1, _, y2)| [y1, y2]).collect();
    ys.sort();
    ys.dedup();
    let mut events: Vec<(i32, i32, usize, usize)> = Vec::new(); // (x, +1/-1, y1 idx, y2 idx)
    for &(x1, y1, x2, y2) in rects {
        let (a, b) = (
            ys.binary_search(&y1).unwrap(),
            ys.binary_search(&y2).unwrap(),
        );
        events.push((x1, 1, a, b));
        events.push((x2, -1, a, b));
    }
    events.sort();
    let mut cover = vec![0i32; ys.len()];
    let (mut area, mut prev_x) = (0i64, 0);
    for (x, d, a, b) in events {
        let covered: i64 = (0..ys.len().saturating_sub(1))
            .filter(|&k| cover[k] > 0)
            .map(|k| (ys[k + 1] - ys[k]) as i64)
            .sum();
        area += covered * (x - prev_x) as i64;
        prev_x = x;
        for c in &mut cover[a..b] {
            *c += d;
        }
    }
    area
}
fn section6_sweep() {
    let mut r = Rng(0x5B71);
    for _ in 0..800 {
        let n = r.below(8) as usize;
        let b: Vec<(i32, i32, i32)> = (0..n)
            .map(|_| {
                let l = r.range(0, 30) as i32;
                (l, l + r.range(1, 10) as i32, r.range(1, 15) as i32)
            })
            .collect();
        assert_eq!(skyline(&b), skyline_oracle(&b), "{b:?}");
        let rects: Vec<(i32, i32, i32, i32)> = (0..n)
            .map(|_| {
                let (x, y) = (r.range(0, 12) as i32, r.range(0, 12) as i32);
                (x, y, x + r.range(1, 6) as i32, y + r.range(1, 6) as i32)
            })
            .collect();
        let cells: HashSet<(i32, i32)> = rects
            .iter()
            .flat_map(|&(x1, y1, x2, y2)| (x1..x2).flat_map(move |x| (y1..y2).map(move |y| (x, y))))
            .collect();
        assert_eq!(union_area(&rects), cells.len() as i64);
    }
    assert_eq!(
        skyline(&[
            (2, 9, 10),
            (3, 7, 15),
            (5, 12, 12),
            (15, 20, 10),
            (19, 24, 8)
        ]),
        [
            (2, 10),
            (3, 15),
            (7, 12),
            (12, 0),
            (15, 10),
            (20, 8),
            (24, 0)
        ],
        "the classic LeetCode 218 example"
    );
    println!("6. sweep: skyline and rectangle-union area match brute-force grids on 800 instances");
}

// =====================================================================================================================
// 7. Huffman coding. EXCHANGE PROOF: the two rarest symbols can be assumed to be siblings at the deepest level (swap them down; moving a rarer symbol deeper never costs more),
//    then merging them into one symbol of their summed weight reduces the problem by one symbol; repeat.
// =====================================================================================================================
fn huffman_lengths(w: &[u64]) -> (Vec<u32>, u64) {
    if w.len() == 1 {
        return (vec![0], 0);
    }
    let mut heap: BinaryHeap<Reverse<(u64, usize)>> = w
        .iter()
        .enumerate()
        .map(|(i, &x)| Reverse((x, i)))
        .collect();
    let mut members: Vec<Vec<usize>> = (0..w.len()).map(|i| vec![i]).collect(); // group id -> leaves
    let (mut len, mut cost) = (vec![0u32; w.len()], 0);
    while heap.len() > 1 {
        let Reverse((wa, a)) = heap.pop().unwrap();
        let Reverse((wb, b)) = heap.pop().unwrap();
        let mut merged = std::mem::take(&mut members[a]);
        merged.extend(std::mem::take(&mut members[b]));
        for &leaf in &merged {
            len[leaf] += 1; // every leaf under the new node gets one level deeper
        }
        cost += wa + wb; // total cost = sum of all internal node weights
        members.push(merged);
        heap.push(Reverse((wa + wb, members.len() - 1)));
    }
    (len, cost)
}
/// Canonical prefix codes from lengths (the DEFLATE scheme): shorter codes first, equal lengths in index order, each code = previous + 1, shifted when the length grows.
fn canonical_codes(len: &[u32]) -> Vec<(u64, u32)> {
    let mut idx: Vec<usize> = (0..len.len()).collect();
    idx.sort_by_key(|&i| (len[i], i));
    let mut codes = vec![(0, 0); len.len()];
    let (mut code, mut prev_len) = (0u64, 0u32);
    for (k, &i) in idx.iter().enumerate() {
        if k > 0 {
            code += 1;
        }
        code <<= len[i] - prev_len;
        prev_len = len[i];
        codes[i] = (code, len[i]);
    }
    codes
}
/// Oracle: the optimal prefix code corresponds to the cheapest sequence of pair merges; try every pair (memoised on the sorted multiset).
fn optimal_cost_brute(w: &[u64], memo: &mut HashMap<Vec<u64>, u64>) -> u64 {
    if w.len() <= 1 {
        return 0;
    }
    let mut key = w.to_vec();
    key.sort();
    if let Some(&c) = memo.get(&key) {
        return c;
    }
    let mut best = u64::MAX;
    for i in 0..key.len() {
        for j in i + 1..key.len() {
            let mut next: Vec<u64> = key
                .iter()
                .enumerate()
                .filter(|&(k, _)| k != i && k != j)
                .map(|(_, &x)| x)
                .collect();
            next.push(key[i] + key[j]);
            best = best.min(key[i] + key[j] + optimal_cost_brute(&next, memo));
        }
    }
    memo.insert(key, best);
    best
}
fn section7_huffman() {
    let mut r = Rng(0x40FF);
    let mut memo = HashMap::new();
    for _ in 0..600 {
        let n = 1 + r.below(7) as usize;
        let w: Vec<u64> = (0..n).map(|_| r.range(1, 30) as u64).collect();
        let (len, cost) = huffman_lengths(&w);
        assert_eq!(cost, optimal_cost_brute(&w, &mut memo), "{w:?}");
        assert_eq!(
            cost,
            w.iter().zip(&len).map(|(&x, &l)| x * l as u64).sum::<u64>()
        );
        if n > 1 {
            // Kraft equality: a FULL binary tree has sum 2^-len == 1 exactly.
            let max = *len.iter().max().unwrap();
            assert_eq!(
                len.iter().map(|&l| 1u64 << (max - l)).sum::<u64>(),
                1 << max
            );
            // Prefix-free: no code is a prefix of another.
            let codes = canonical_codes(&len);
            for (i, &(ci, li)) in codes.iter().enumerate() {
                for (j, &(cj, lj)) in codes.iter().enumerate() {
                    assert!(
                        i == j || li > lj || (cj >> (lj - li)) != ci,
                        "code {i} is a prefix of {j}"
                    );
                }
            }
            // Fixed-length coding is never better.
            let fixed = (usize::BITS - (n - 1).leading_zeros()) as u64;
            assert!(cost <= w.iter().sum::<u64>() * fixed);
        }
    }
    assert_eq!(
        huffman_lengths(&[1, 1, 2, 4, 8]).1,
        2 + 4 + 8 + 16,
        "Fibonacci-like weights give a maximally skewed tree"
    );
    println!(
        "7. huffman: cost equals the brute-force optimal merge order on 600 alphabets; Kraft equality and prefix-freeness hold"
    );
}

// =====================================================================================================================
// 8. Knapsack: FRACTIONAL (greedy by value/weight is optimal; exchange argument) vs 0/1 (greedy fails; DP; and the max(prefix, best single item) >= OPT/2 trick).
// =====================================================================================================================
fn fractional_knapsack(items: &[(u32, u32)], cap: u32) -> f64 {
    let mut v = items.to_vec();
    // Compare ratios by cross-multiplication in u64: no floating point in the comparator, no division by zero.
    v.sort_by(|a, b| (b.1 as u64 * a.0 as u64).cmp(&(a.1 as u64 * b.0 as u64)));
    let (mut left, mut total) = (cap, 0.0);
    for (w, val) in v {
        let take = w.min(left);
        total += val as f64 * take as f64 / w as f64;
        left -= take;
    }
    total
}
/// Oracle: with integer weights and integer capacity the fractional optimum only ever splits at unit boundaries, so it equals a 0/1 DP over unit pieces of value v/w.
fn fractional_oracle(items: &[(u32, u32)], cap: u32) -> f64 {
    let pieces: Vec<f64> = items
        .iter()
        .flat_map(|&(w, v)| std::iter::repeat_n(v as f64 / w as f64, w as usize))
        .collect();
    let mut dp = vec![0.0f64; cap as usize + 1];
    for p in pieces {
        for c in (1..=cap as usize).rev() {
            dp[c] = dp[c].max(dp[c - 1] + p);
        }
    }
    dp[cap as usize]
}
fn knapsack_01(items: &[(u32, u32)], cap: u32) -> u64 {
    let mut dp = vec![0u64; cap as usize + 1];
    for &(w, v) in items {
        for c in (w as usize..=cap as usize).rev() {
            dp[c] = dp[c].max(dp[c - w as usize] + v as u64);
        }
    }
    dp[cap as usize]
}
fn greedy_01_by_ratio(items: &[(u32, u32)], cap: u32) -> u64 {
    let mut v = items.to_vec();
    v.sort_by(|a, b| (b.1 as u64 * a.0 as u64).cmp(&(a.1 as u64 * b.0 as u64)));
    let (mut left, mut total) = (cap, 0u64);
    for (w, val) in v {
        if w <= left {
            left -= w;
            total += val as u64;
        }
    }
    total
}
/// 1/2-approximation: best of (ratio-greedy that STOPS at the first item that does not fit) and (the single most valuable item that fits).
fn half_approx(items: &[(u32, u32)], cap: u32) -> u64 {
    let mut v = items.to_vec();
    v.sort_by(|a, b| (b.1 as u64 * a.0 as u64).cmp(&(a.1 as u64 * b.0 as u64)));
    let (mut left, mut prefix) = (cap, 0u64);
    for &(w, val) in &v {
        if w > left {
            break;
        }
        left -= w;
        prefix += val as u64;
    }
    let single = items
        .iter()
        .filter(|&&(w, _)| w <= cap)
        .map(|&(_, v)| v as u64)
        .max()
        .unwrap_or(0);
    prefix.max(single)
}
fn section8_knapsack() {
    let mut r = Rng(0x4A9);
    let mut fails = 0;
    for _ in 0..1000 {
        let n = 1 + r.below(7) as usize;
        let items: Vec<(u32, u32)> = (0..n)
            .map(|_| (r.range(1, 9) as u32, r.range(1, 30) as u32))
            .collect();
        let cap = r.range(1, 25) as u32;
        let (f, o) = (
            fractional_knapsack(&items, cap),
            fractional_oracle(&items, cap),
        );
        assert!((f - o).abs() < 1e-6, "{items:?} cap {cap}: {f} vs {o}");
        let opt = knapsack_01(&items, cap);
        assert!(
            opt as f64 <= f + 1e-9,
            "fractional is a relaxation: an upper bound"
        );
        assert!(greedy_01_by_ratio(&items, cap) <= opt);
        fails += (greedy_01_by_ratio(&items, cap) < opt) as u32;
        assert!(
            2 * half_approx(&items, cap) >= opt,
            "1/2-approximation bound"
        );
    }
    assert!(fails > 0);
    // Hand-made: capacity 50, items (10,60), (20,100), (30,120): ratio greedy takes 160, optimum is 220.
    let items = [(10, 60), (20, 100), (30, 120)];
    assert_eq!(
        (greedy_01_by_ratio(&items, 50), knapsack_01(&items, 50)),
        (160, 220)
    );
    assert!((fractional_knapsack(&items, 50) - 240.0).abs() < 1e-9);
    println!(
        "8. knapsack: fractional greedy == unit-piece DP; ratio greedy lost to the 0/1 DP on {fails}/1000; half-approx bound held"
    );
}

// =====================================================================================================================
// 9. Coin change: greedy (largest coin first) is optimal only for CANONICAL systems. A system is canonical iff greedy is optimal for every amount;
//    a counterexample, if any exists, is below the sum of the two largest coins (Kozen and Zaks), so a DP check up to that bound decides it.
// =====================================================================================================================
fn greedy_coins(coins: &[u32], mut amount: u32) -> u32 {
    let mut n = 0;
    for &c in coins.iter().rev() {
        n += amount / c;
        amount %= c;
    }
    n
}
fn dp_coins(coins: &[u32], upto: u32) -> Vec<u32> {
    let mut dp = vec![u32::MAX; upto as usize + 1];
    dp[0] = 0;
    for a in 1..=upto as usize {
        for &c in coins {
            if c as usize <= a && dp[a - c as usize] != u32::MAX {
                dp[a] = dp[a].min(dp[a - c as usize] + 1);
            }
        }
    }
    dp
}
fn first_counterexample(coins: &[u32], bound: u32) -> Option<u32> {
    let dp = dp_coins(coins, bound);
    (1..=bound).find(|&a| greedy_coins(coins, a) != dp[a as usize])
}
fn is_canonical(coins: &[u32]) -> bool {
    let n = coins.len();
    if n < 3 {
        return n < 2 || coins[0] == 1; // two coins {1, c}: always canonical; without a 1 amounts may be unreachable
    }
    first_counterexample(coins, coins[n - 1] + coins[n - 2]).is_none()
}
fn section9_coins() {
    assert!(is_canonical(&[1, 5, 10, 25]) && is_canonical(&[1, 2, 5, 10, 20, 50, 100, 200]));
    assert_eq!(
        first_counterexample(&[1, 3, 4], 100),
        Some(6),
        "6 = 3 + 3, greedy says 4 + 1 + 1"
    );
    let mut r = Rng(0xC014);
    let (mut non_canon, total) = (0, 3000);
    for _ in 0..total {
        let k = 3 + r.below(3) as usize;
        let mut coins: Vec<u32> = std::iter::once(1)
            .chain((1..k).map(|_| r.range(2, 40) as u32))
            .collect();
        coins.sort();
        coins.dedup();
        if coins.len() < 3 {
            continue;
        }
        let truth = first_counterexample(&coins, 600).is_none(); // far beyond the Kozen-Zaks bound (max coin 40, bound 80)
        assert_eq!(is_canonical(&coins), truth, "{coins:?}");
        non_canon += !truth as u32;
    }
    println!(
        "9. coins: Kozen-Zaks bound check agrees with a 600-amount check on random systems ({non_canon} of ~{total} were non-canonical)"
    );
}

// =====================================================================================================================
// 10. Job sequencing with deadlines (unit-time jobs, profit, deadline d: must finish by slot d). Feasible sets form a MATROID, so "sort by profit, keep if still feasible" is optimal.
//     Feasibility test via union-find: assign each job the LATEST free slot <= its deadline; find(t) returns that slot (0 = none).
// =====================================================================================================================
fn job_sequencing(jobs: &[(usize, u32)]) -> u32 {
    let max_d = jobs.iter().map(|j| j.0).max().unwrap_or(0);
    let mut slot = Dsu::new(max_d + 1); // slot[t] points to the nearest free slot at or before t
    let mut order: Vec<(usize, u32)> = jobs.to_vec();
    order.sort_by_key(|j| Reverse(j.1));
    let mut total = 0;
    for (d, p) in order {
        let free = slot.find(d);
        if free > 0 {
            slot.p[free] = free - 1; // occupy: future lookups skip to the previous slot
            total += p;
        }
    }
    total
}
fn job_sequencing_brute(jobs: &[(usize, u32)]) -> u32 {
    let n = jobs.len();
    (0u32..1 << n)
        .filter_map(|m| {
            let mut ds: Vec<usize> = (0..n)
                .filter(|&i| m >> i & 1 == 1)
                .map(|i| jobs[i].0)
                .collect();
            ds.sort();
            ds.iter()
                .enumerate()
                .all(|(i, &d)| d >= i + 1)
                .then(|| (0..n).filter(|&i| m >> i & 1 == 1).map(|i| jobs[i].1).sum())
        })
        .max()
        .unwrap()
}
fn section10_jobs() {
    let mut r = Rng(0x10B5);
    for _ in 0..800 {
        let n = 1 + r.below(10) as usize;
        let jobs: Vec<(usize, u32)> = (0..n)
            .map(|_| (r.range(1, 6) as usize, r.range(1, 50) as u32))
            .collect();
        assert_eq!(
            job_sequencing(&jobs),
            job_sequencing_brute(&jobs),
            "{jobs:?}"
        );
    }
    println!(
        "10. job sequencing: DSU slot assignment equals the subset brute force on 800 instances"
    );
}

// =====================================================================================================================
// 11. Minimise maximum lateness (EDF) and the task scheduler with cooldown.
//     EDF EXCHANGE PROOF: if two adjacent jobs are out of deadline order (inversion), swapping them does not increase the maximum lateness; sorting by deadline removes all inversions.
// =====================================================================================================================
fn max_lateness(jobs: &[(i64, i64)]) -> i64 {
    // Back to back from time 0 in the given order; lateness = finish - due (may be negative, which means early).
    let mut t = 0;
    jobs.iter()
        .map(|&(len, due)| {
            t += len;
            t - due
        })
        .max()
        .unwrap_or(0)
}
fn edf(jobs: &[(i64, i64)]) -> i64 {
    let mut v = jobs.to_vec();
    v.sort_by_key(|j| j.1);
    max_lateness(&v)
}
fn shortest_job_first(jobs: &[(i64, i64)]) -> i64 {
    let mut v = jobs.to_vec();
    v.sort_by_key(|j| j.0);
    max_lateness(&v)
}
fn permutations(n: usize) -> Vec<Vec<usize>> {
    fn rec(cur: &mut Vec<usize>, used: &mut Vec<bool>, out: &mut Vec<Vec<usize>>) {
        if cur.len() == used.len() {
            out.push(cur.clone());
            return;
        }
        for i in 0..used.len() {
            if !used[i] {
                used[i] = true;
                cur.push(i);
                rec(cur, used, out);
                cur.pop();
                used[i] = false;
            }
        }
    }
    let mut out = Vec::new();
    rec(&mut Vec::new(), &mut vec![false; n], &mut out);
    out
}
/// Task scheduler with cooldown n between equal tasks: closed form (max_freq - 1) * (n + 1) + (tasks with max_freq), but never less than the task count.
fn task_scheduler(counts: &[u32], n: u32) -> u32 {
    let total: u32 = counts.iter().sum();
    let maxf = counts.iter().copied().max().unwrap_or(0);
    let ties = counts.iter().filter(|&&c| c == maxf).count() as u32;
    total.max(maxf.saturating_sub(1) * (n + 1) + ties)
}
/// Oracle: BFS over (remaining counts, cooldown left per type); idle is allowed.
fn task_scheduler_bfs(counts: &[u32], n: u32) -> u32 {
    type State = (Vec<u32>, Vec<u32>);
    let k = counts.len();
    let mut layer: HashSet<State> = HashSet::from([(counts.to_vec(), vec![0; k])]);
    let mut steps = 0;
    loop {
        if layer.iter().any(|(rem, _)| rem.iter().all(|&c| c == 0)) {
            return steps;
        }
        let mut next = HashSet::new();
        for (rem, cd) in &layer {
            let tick =
                |cd: &Vec<u32>| -> Vec<u32> { cd.iter().map(|&c| c.saturating_sub(1)).collect() };
            next.insert((rem.clone(), tick(cd))); // idle
            for i in 0..k {
                if rem[i] > 0 && cd[i] == 0 {
                    let (mut rem2, mut cd2) = (rem.clone(), tick(cd));
                    rem2[i] -= 1;
                    cd2[i] = n;
                    next.insert((rem2, cd2));
                }
            }
        }
        layer = next;
        steps += 1;
    }
}
fn section11_lateness_cooldown() {
    let mut r = Rng(0xED0);
    let mut sjf_loses = 0;
    for _ in 0..600 {
        let n = 1 + r.below(6) as usize;
        let jobs: Vec<(i64, i64)> = (0..n).map(|_| (r.range(1, 8), r.range(1, 20))).collect();
        let best = permutations(n)
            .iter()
            .map(|p| max_lateness(&p.iter().map(|&i| jobs[i]).collect::<Vec<_>>()))
            .min()
            .unwrap();
        assert_eq!(edf(&jobs), best, "{jobs:?}");
        assert!(shortest_job_first(&jobs) >= best);
        sjf_loses += (shortest_job_first(&jobs) > best) as u32;
    }
    assert!(
        sjf_loses > 0,
        "shortest-job-first is the wrong greedy for lateness"
    );
    for _ in 0..300 {
        let k = 1 + r.below(4) as usize;
        let counts: Vec<u32> = (0..k).map(|_| r.range(1, 3) as u32).collect();
        let n = r.below(4) as u32;
        assert_eq!(
            task_scheduler(&counts, n),
            task_scheduler_bfs(&counts, n),
            "{counts:?} n={n}"
        );
    }
    assert_eq!(task_scheduler(&[3, 3], 2), 8, "A B _ A B _ A B");
    println!(
        "11. EDF equals the best of all permutations on 600 instances (SJF lost {sjf_loses}x); cooldown formula equals BFS on 300"
    );
}

// =====================================================================================================================
// 12. Array greedies: gas station, jump game I/II, candy, assign cookies, queue reconstruction by height.
// =====================================================================================================================
/// If total gas >= total cost a start exists; whenever the tank goes negative at i, no start in (last_start..=i) works (each had a non-negative prefix when it reached i), so restart at i + 1.
fn gas_station(gas: &[i32], cost: &[i32]) -> Option<usize> {
    let (mut total, mut tank, mut start) = (0i64, 0i64, 0);
    for i in 0..gas.len() {
        let d = (gas[i] - cost[i]) as i64;
        total += d;
        tank += d;
        if tank < 0 {
            start = i + 1;
            tank = 0;
        }
    }
    (total >= 0 && start < gas.len()).then_some(start)
}
fn gas_valid_starts(gas: &[i32], cost: &[i32]) -> Vec<usize> {
    let n = gas.len();
    (0..n)
        .filter(|&s| {
            let mut tank = 0i64;
            (0..n).all(|k| {
                let i = (s + k) % n;
                tank += (gas[i] - cost[i]) as i64;
                tank >= 0
            })
        })
        .collect()
}
fn can_jump(a: &[usize]) -> bool {
    let mut far = 0;
    for (i, &x) in a.iter().enumerate() {
        if i > far {
            return false;
        }
        far = far.max(i + x);
    }
    true
}
fn can_jump_dp(a: &[usize]) -> bool {
    let mut ok = vec![false; a.len()];
    ok[0] = true;
    for i in 0..a.len() {
        if ok[i] {
            for j in i + 1..(i + a[i] + 1).min(a.len()) {
                ok[j] = true;
            }
        }
    }
    ok[a.len() - 1]
}
/// Jump game II as implicit BFS: `end` is the reach of the current number of jumps, `far` the reach of one more.
fn min_jumps(a: &[usize]) -> Option<usize> {
    let (mut jumps, mut end, mut far) = (0, 0, 0);
    for i in 0..a.len() - 1 {
        far = far.max(i + a[i]);
        if i == end {
            if far == i {
                return None;
            }
            jumps += 1;
            end = far;
            if end >= a.len() - 1 {
                break;
            }
        }
    }
    Some(jumps)
}
fn min_jumps_dp(a: &[usize]) -> Option<usize> {
    let n = a.len();
    let mut dp = vec![usize::MAX; n];
    dp[0] = 0;
    for i in 0..n {
        if dp[i] == usize::MAX {
            continue;
        }
        for j in i + 1..(i + a[i] + 1).min(n) {
            dp[j] = dp[j].min(dp[i] + 1);
        }
    }
    (dp[n - 1] != usize::MAX).then_some(dp[n - 1])
}
/// Candy: left-to-right pass enforces "higher than left neighbour", right-to-left enforces the right one; take the max. Each pass is a one-sided greedy.
fn candy(rating: &[i32]) -> i64 {
    let n = rating.len();
    let mut c = vec![1i64; n];
    for i in 1..n {
        if rating[i] > rating[i - 1] {
            c[i] = c[i - 1] + 1;
        }
    }
    for i in (0..n.saturating_sub(1)).rev() {
        if rating[i] > rating[i + 1] {
            c[i] = c[i].max(c[i + 1] + 1);
        }
    }
    c.iter().sum()
}
/// Oracle: start with 1 each and keep raising violators until stable; the fixpoint is the pointwise-minimal valid assignment.
fn candy_fixpoint(rating: &[i32]) -> i64 {
    let n = rating.len();
    let mut c = vec![1i64; n];
    loop {
        let mut changed = false;
        for i in 0..n {
            for j in [i.wrapping_sub(1), i + 1] {
                if j < n && rating[i] > rating[j] && c[i] <= c[j] {
                    c[i] = c[j] + 1;
                    changed = true;
                }
            }
        }
        if !changed {
            return c.iter().sum();
        }
    }
}
fn assign_cookies(greed: &[u32], size: &[u32]) -> usize {
    let (mut g, mut s) = (greed.to_vec(), size.to_vec());
    g.sort();
    s.sort();
    let (mut i, mut j) = (0, 0);
    while i < g.len() && j < s.len() {
        if s[j] >= g[i] {
            i += 1; // smallest cookie that satisfies the least greedy child: using a bigger one could only waste it
        }
        j += 1;
    }
    i
}
fn cookies_brute(greed: &[u32], size: &[u32], i: usize, used: u32) -> usize {
    if i == greed.len() {
        return 0;
    }
    let mut best = cookies_brute(greed, size, i + 1, used); // child i gets nothing
    for j in 0..size.len() {
        if used >> j & 1 == 0 && size[j] >= greed[i] {
            best = best.max(1 + cookies_brute(greed, size, i + 1, used | 1 << j));
        }
    }
    best
}
/// People (height, k): k = number of people in front who are at least as tall. Place the tallest first (they are unaffected by shorter people), each at index k.
fn reconstruct_queue(mut p: Vec<(u32, usize)>) -> Vec<(u32, usize)> {
    p.sort_by_key(|&(h, k)| (Reverse(h), k));
    let mut q = Vec::with_capacity(p.len());
    for x in p {
        q.insert(x.1, x);
    }
    q
}
fn section12_arrays() {
    let mut r = Rng(0xA44A);
    for _ in 0..1500 {
        let n = 1 + r.below(7) as usize;
        let gas: Vec<i32> = (0..n).map(|_| r.range(0, 6) as i32).collect();
        let cost: Vec<i32> = (0..n).map(|_| r.range(0, 6) as i32).collect();
        let valid = gas_valid_starts(&gas, &cost);
        match gas_station(&gas, &cost) {
            Some(s) => assert!(valid.contains(&s), "{gas:?} {cost:?}"),
            None => assert!(valid.is_empty(), "{gas:?} {cost:?} {valid:?}"),
        }
        let a: Vec<usize> = (0..n + 1).map(|_| r.below(4) as usize).collect();
        assert_eq!(can_jump(&a), can_jump_dp(&a), "{a:?}");
        assert_eq!(min_jumps(&a), min_jumps_dp(&a), "{a:?}");
        let ratings: Vec<i32> = (0..n + 2).map(|_| r.range(0, 5) as i32).collect();
        assert_eq!(candy(&ratings), candy_fixpoint(&ratings), "{ratings:?}");
        let g: Vec<u32> = (0..r.below(6)).map(|_| r.range(1, 6) as u32).collect();
        let s: Vec<u32> = (0..r.below(6)).map(|_| r.range(1, 6) as u32).collect();
        assert_eq!(
            assign_cookies(&g, &s),
            cookies_brute(&g, &s, 0, 0),
            "{g:?} {s:?}"
        );
        // Queue: build a true queue, derive k's, shuffle, reconstruct, check every constraint.
        let heights: Vec<u32> = (0..n + 3).map(|_| r.range(1, 5) as u32).collect();
        let people: Vec<(u32, usize)> = heights
            .iter()
            .enumerate()
            .map(|(i, &h)| (h, heights[..i].iter().filter(|&&x| x >= h).count()))
            .collect();
        let mut shuffled = people.clone();
        r.shuffle(&mut shuffled);
        let q = reconstruct_queue(shuffled.clone());
        for (i, &(h, k)) in q.iter().enumerate() {
            assert_eq!(q[..i].iter().filter(|&&(x, _)| x >= h).count(), k);
        }
        let (mut a1, mut a2) = (q.clone(), shuffled);
        a1.sort();
        a2.sort();
        assert_eq!(a1, a2, "same people");
    }
    assert_eq!(gas_station(&[1, 2, 3, 4, 5], &[3, 4, 5, 1, 2]), Some(3));
    assert_eq!(gas_station(&[2, 3, 4], &[3, 4, 3]), None);
    assert_eq!(candy(&[1, 0, 2]), 5);
    println!(
        "12. gas station, jump I/II, candy, cookies, queue reconstruction agree with DP/brute-force/fixpoint oracles on 1500 instances"
    );
}

// =====================================================================================================================
// 13. Smallest subsequence of length m with a monotonic stack (also "remove k digits"). Pop while the top is larger than the incoming
//     element AND enough elements remain to still reach length m. EXCHANGE: a smaller earlier element always beats a larger one.
// =====================================================================================================================
fn smallest_subsequence(s: &[u8], m: usize) -> Vec<u8> {
    let mut st: Vec<u8> = Vec::with_capacity(m);
    let n = s.len();
    for (i, &c) in s.iter().enumerate() {
        while st.last().is_some_and(|&t| t > c) && st.len() - 1 + (n - i) >= m {
            st.pop();
        }
        if st.len() < m {
            st.push(c);
        }
    }
    st
}
fn remove_k_digits(num: &str, k: usize) -> String {
    let kept = smallest_subsequence(num.as_bytes(), num.len().saturating_sub(k));
    let s: String = kept.iter().map(|&b| b as char).collect();
    let t = s.trim_start_matches('0');
    if t.is_empty() {
        "0".to_string()
    } else {
        t.to_string()
    }
}
fn smallest_subseq_brute(s: &[u8], m: usize) -> Vec<u8> {
    let n = s.len();
    (0u32..1 << n)
        .filter(|x| x.count_ones() as usize == m)
        .map(|x| {
            (0..n)
                .filter(|&i| x >> i & 1 == 1)
                .map(|i| s[i])
                .collect::<Vec<u8>>()
        })
        .min()
        .unwrap()
}
fn section13_monotonic() {
    let mut r = Rng(0x5E0);
    for _ in 0..1000 {
        let n = 1 + r.below(10) as usize;
        let s: Vec<u8> = (0..n).map(|_| b'0' + r.below(4) as u8).collect();
        let m = r.below(n as u64 + 1) as usize;
        assert_eq!(
            smallest_subsequence(&s, m),
            smallest_subseq_brute(&s, m),
            "{:?} {m}",
            String::from_utf8_lossy(&s)
        );
        let k = n - m;
        let digits = String::from_utf8(s.clone()).unwrap();
        let brute: u64 = smallest_subseq_brute(&s, m)
            .iter()
            .fold(0, |a, &b| a * 10 + (b - b'0') as u64);
        assert_eq!(remove_k_digits(&digits, k).parse::<u64>().unwrap(), brute);
    }
    assert_eq!(remove_k_digits("1432219", 3), "1219");
    assert_eq!(remove_k_digits("10200", 1), "200", "leading zeros stripped");
    assert_eq!(
        remove_k_digits("10", 2),
        "0",
        "removing everything yields 0"
    );
    println!(
        "13. monotonic stack: smallest subsequence / remove-k-digits equal the subset brute force on 1000 strings"
    );
}

// =====================================================================================================================
// 14. Spanning trees and shortest paths. KRUSKAL: sort edges, add if it joins two components. PROOF (cut property / matroid exchange): the lightest edge crossing any cut is in some MST.
//     PRIM grows one tree with a heap. DIJKSTRA is greedy "settle the closest unsettled node": correct ONLY with non-negative weights (a later negative edge could undercut a settled node).
// =====================================================================================================================
type Edge = (usize, usize, u32);
fn kruskal(n: usize, edges: &[Edge]) -> Option<u64> {
    let mut e = edges.to_vec();
    e.sort_by_key(|x| x.2);
    let mut d = Dsu::new(n);
    let (mut total, mut used) = (0u64, 0);
    for (a, b, w) in e {
        if d.union(a, b) {
            total += w as u64;
            used += 1;
        }
    }
    (used == n - 1).then_some(total)
}
fn prim(n: usize, edges: &[Edge]) -> Option<u64> {
    let mut adj = vec![Vec::new(); n];
    for &(a, b, w) in edges {
        adj[a].push((b, w));
        adj[b].push((a, w));
    }
    let (mut seen, mut heap, mut total, mut count) = (vec![false; n], BinaryHeap::new(), 0u64, 0);
    heap.push(Reverse((0u32, 0usize)));
    while let Some(Reverse((w, u))) = heap.pop() {
        if seen[u] {
            continue;
        }
        seen[u] = true;
        total += w as u64;
        count += 1;
        for &(v, wv) in &adj[u] {
            if !seen[v] {
                heap.push(Reverse((wv, v)));
            }
        }
    }
    (count == n).then_some(total)
}
fn mst_brute(n: usize, edges: &[Edge]) -> Option<u64> {
    (0u32..1 << edges.len())
        .filter(|m| m.count_ones() as usize == n - 1)
        .filter_map(|m| {
            let mut d = Dsu::new(n);
            let mut w = 0u64;
            let tree = (0..edges.len()).filter(|&i| m >> i & 1 == 1).all(|i| {
                w += edges[i].2 as u64;
                d.union(edges[i].0, edges[i].1)
            });
            tree.then_some(w)
        })
        .min()
}
/// Dijkstra with a switch. `allow_reopen = false` is the textbook version: a settled node is final. `true` re-processes improved nodes (label-correcting; survives negative edges
/// but can take exponential time in the worst case, and loops forever on a negative cycle).
fn dijkstra(adj: &[Vec<(usize, i64)>], src: usize, allow_reopen: bool) -> Vec<i64> {
    let n = adj.len();
    let (mut dist, mut settled) = (vec![i64::MAX; n], vec![false; n]);
    let mut heap = BinaryHeap::new();
    dist[src] = 0;
    heap.push(Reverse((0i64, src)));
    while let Some(Reverse((d, u))) = heap.pop() {
        if d > dist[u] || (!allow_reopen && settled[u]) {
            continue;
        }
        settled[u] = true;
        for &(v, w) in &adj[u] {
            if !allow_reopen && settled[v] {
                continue; // a settled distance is FINAL in the textbook algorithm
            }
            if dist[u] + w < dist[v] {
                dist[v] = dist[u] + w;
                heap.push(Reverse((dist[v], v)));
            }
        }
    }
    dist
}
fn bellman_ford(adj: &[Vec<(usize, i64)>], src: usize) -> Vec<i64> {
    let n = adj.len();
    let mut dist = vec![i64::MAX; n];
    dist[src] = 0;
    for _ in 0..n {
        for u in 0..n {
            if dist[u] == i64::MAX {
                continue;
            }
            for &(v, w) in &adj[u] {
                dist[v] = dist[v].min(dist[u] + w);
            }
        }
    }
    dist
}
fn section14_graphs() {
    let mut r = Rng(0x6EA9);
    for _ in 0..600 {
        let n = 2 + r.below(5) as usize;
        let mut edges: Vec<Edge> = (1..n)
            .map(|i| (r.below(i as u64) as usize, i, r.range(1, 20) as u32))
            .collect(); // spanning tree => connected
        while edges.len() < 10 && r.below(4) != 0 {
            let (a, b) = (r.below(n as u64) as usize, r.below(n as u64) as usize);
            if a != b {
                edges.push((a, b, r.range(1, 20) as u32));
            }
        }
        let brute = mst_brute(n, &edges);
        assert_eq!(kruskal(n, &edges), brute);
        assert_eq!(prim(n, &edges), brute);
    }
    let mut big: Vec<Edge> = (1..200)
        .map(|i| (r.below(i as u64) as usize, i, r.range(1, 1000) as u32))
        .collect();
    for _ in 0..800 {
        let (a, b) = (r.below(200) as usize, r.below(200) as usize);
        if a != b {
            big.push((a, b, r.range(1, 1000) as u32));
        }
    }
    assert_eq!(
        kruskal(200, &big),
        prim(200, &big),
        "two different greedy algorithms, one answer"
    );
    // Dijkstra vs Bellman-Ford on non-negative graphs.
    for _ in 0..500 {
        let n = 2 + r.below(8) as usize;
        let mut adj = vec![Vec::new(); n];
        for _ in 0..r.below(25) {
            adj[r.below(n as u64) as usize].push((r.below(n as u64) as usize, r.range(0, 20)));
        }
        assert_eq!(dijkstra(&adj, 0, false), bellman_ford(&adj, 0));
    }
    // One negative edge breaks the settle-once version: s->a 1, s->b 5, b->a -10. True dist(a) = -5.
    let mut adj = vec![Vec::new(); 3]; // 0 = s, 1 = a, 2 = b
    adj[0].push((1, 1));
    adj[0].push((2, 5));
    adj[2].push((1, -10));
    assert_eq!(bellman_ford(&adj, 0)[1], -5);
    assert_eq!(
        dijkstra(&adj, 0, false)[1],
        1,
        "greedy settled `a` too early"
    );
    assert_eq!(
        dijkstra(&adj, 0, true)[1],
        -5,
        "re-opening repairs it (at a cost)"
    );
    println!(
        "14. Kruskal == Prim == brute-force MST (600 small, one n=200); Dijkstra == Bellman-Ford; negative edge counterexample shown"
    );
}

// =====================================================================================================================
// 15. Set cover: greedy (pick the set covering the most uncovered elements) is a H(d)-approximation, d = largest set size, H(d) = 1 + 1/2 + ... + 1/d ~ ln d + 1.
//     PROOF SKETCH (Chvatal): charge each newly covered element 1/(number newly covered by the chosen set); OPT's i-th set S has total charge <= H(|S|) because charges to its
//     elements are non-decreasing as they are covered one at a time; summing over OPT's sets bounds greedy's size by H(d) * OPT.
// =====================================================================================================================
fn greedy_set_cover(universe: u32, sets: &[u32]) -> usize {
    let (mut covered, mut picked) = (0u32, 0);
    while covered & universe != universe {
        let best = sets
            .iter()
            .max_by_key(|&&s| (s & !covered).count_ones())
            .unwrap();
        assert!(best & !covered != 0, "universe must be coverable");
        covered |= best;
        picked += 1;
    }
    picked
}
fn optimal_set_cover(universe: u32, sets: &[u32]) -> usize {
    (0u32..1 << sets.len())
        .filter(|&m| {
            (0..sets.len())
                .filter(|&i| m >> i & 1 == 1)
                .fold(0, |a, i| a | sets[i])
                & universe
                == universe
        })
        .map(|m| m.count_ones() as usize)
        .min()
        .unwrap()
}
fn harmonic(d: u32) -> f64 {
    (1..=d).map(|i| 1.0 / i as f64).sum()
}
fn section15_set_cover() {
    let mut r = Rng(0x5E7C);
    let (mut worst_ratio, mut suboptimal) = (1.0f64, 0);
    for _ in 0..1500 {
        let n_elems = 4 + r.below(9) as u32; // universe of 4..=12 elements
        let universe = (1u32 << n_elems) - 1;
        let mut sets: Vec<u32> = (0..3 + r.below(8))
            .map(|_| (r.next() as u32 & r.next() as u32 | 1 << r.below(n_elems as u64)) & universe)
            .collect();
        let union = sets.iter().fold(0, |a, &s| a | s);
        if union & universe != universe {
            sets.push(universe & !union); // make the instance coverable
        }
        let (g, o) = (
            greedy_set_cover(universe, &sets),
            optimal_set_cover(universe, &sets),
        );
        let d = sets.iter().map(|s| s.count_ones()).max().unwrap();
        assert!(g >= o);
        assert!(
            g as f64 <= harmonic(d) * o as f64 + 1e-9,
            "greedy {g} opt {o} d {d}"
        );
        worst_ratio = worst_ratio.max(g as f64 / o as f64);
        suboptimal += (g > o) as u32;
    }
    // Classic tight-ish instance: universe 1..=6, greedy takes the big middle set (4 elements) then needs 2 more; optimum is the 2 disjoint halves.
    let universe = 0b111111;
    let sets = [0b001111, 0b110000, 0b000111, 0b111000, 0b001110];
    assert_eq!(optimal_set_cover(universe, &sets), 2);
    assert!(greedy_set_cover(universe, &sets) >= 2);
    println!(
        "15. set cover: greedy within H(d) * OPT on 1500 instances (worst observed ratio {worst_ratio:.2}, suboptimal {suboptimal}x)"
    );
}

fn main() {
    section1_scheduling();
    section2_rooms();
    section3_merge();
    section4_stabbing();
    section5_calendar();
    section6_sweep();
    section7_huffman();
    section8_knapsack();
    section9_coins();
    section10_jobs();
    section11_lateness_cooldown();
    section12_arrays();
    section13_monotonic();
    section14_graphs();
    section15_set_cover();
    println!("all greedy and interval sections verified");
}
