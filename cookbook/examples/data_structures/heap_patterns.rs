//! RECIPE 160 (data-structures): heaps from scratch (binary, d-ary, indexed) and the classic heap patterns.
//! LEVEL: intermediate
//! Use in: priority queues, shortest paths, streaming statistics, scheduling, merging sorted data.   Run: cargo run --release --example heap_patterns
#![allow(dead_code)]
//!
//! WHAT: a heap keeps the smallest (or largest) element at index 0 of a plain `Vec`. The tree is implicit:
//! children of i are 2i+1 and 2i+2 (d-ary: d*i+1 ..= d*i+d). WHY a Vec: no pointers, great cache behaviour.
//! HOW: `push` appends and sifts UP; `pop` swaps the last element to the root and sifts DOWN; `heapify`
//! sifts down every internal node from the last parent to the root, which is O(n) (not O(n log n)) because
//! most nodes sit near the leaves and barely move. Heapsort = heapify, then repeatedly swap max to the end.
//! A d-ary heap is shallower (log_d n levels): cheaper push, costlier pop; often faster in practice.
//! An INDEXED heap also stores each id's position, so `decrease_key(id, new)` is O(log n): that is what
//! Dijkstra wants (dijkstra instead pushes duplicates and skips stale entries, "lazy deletion").
//! Patterns below each have a brute-force oracle fed with seeded random data.
//! COMMON USES: OS and job schedulers, event-driven simulation, Dijkstra/A*/Prim, top-k leaderboards,
//!   log-merging (k sorted streams), running percentiles/medians, rate limiters and timer wheels' fallbacks.
//! Read: https://en.wikipedia.org/wiki/Binary_heap and https://doc.rust-lang.org/std/collections/struct.BinaryHeap.html
use std::cmp::Reverse;
use std::collections::{BinaryHeap, VecDeque};

struct Rng(u64);

impl Rng {
    fn new(seed: u64) -> Self {
        Rng(seed.wrapping_mul(0x9E37_79B9_7F4A_7C15) | 1)
    }
    fn next(&mut self) -> u64 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        self.0
    }
    fn below(&mut self, n: u64) -> u64 {
        self.next() % n
    }
    fn vec(&mut self, len: usize, max: u64) -> Vec<i64> {
        (0..len)
            .map(|_| self.below(max) as i64 - (max / 2) as i64)
            .collect()
    }
}

// ---------- 1. d-ary min-heap (D = 2 is the classic binary heap) ----------
struct DHeap<T: Ord, const D: usize> {
    v: Vec<T>,
}

impl<T: Ord, const D: usize> DHeap<T, D> {
    fn new() -> Self {
        DHeap { v: Vec::new() }
    }

    /// O(n): sift down every internal node, last parent first
    fn heapify(v: Vec<T>) -> Self {
        let mut h = DHeap { v };
        if h.v.len() > 1 {
            for i in (0..=(h.v.len() - 2) / D).rev() {
                h.sift_down(i);
            }
        }
        h
    }

    fn len(&self) -> usize {
        self.v.len()
    }

    fn peek(&self) -> Option<&T> {
        self.v.first()
    }

    fn push(&mut self, x: T) {
        self.v.push(x);
        self.sift_up(self.v.len() - 1);
    }

    fn pop(&mut self) -> Option<T> {
        if self.v.is_empty() {
            return None;
        }
        let last = self.v.len() - 1;
        self.v.swap(0, last); // move the last leaf to the root, then repair downwards
        let out = self.v.pop();
        if !self.v.is_empty() {
            self.sift_down(0);
        }
        out
    }

    fn sift_up(&mut self, mut i: usize) {
        while i > 0 {
            let parent = (i - 1) / D;
            if self.v[i] >= self.v[parent] {
                break;
            }
            self.v.swap(i, parent);
            i = parent;
        }
    }

    fn sift_down(&mut self, mut i: usize) {
        loop {
            let first = D * i + 1;
            if first >= self.v.len() {
                return;
            }
            let last = (first + D).min(self.v.len());
            let smallest_child = (first..last)
                .min_by(|&a, &b| self.v[a].cmp(&self.v[b]))
                .unwrap();
            if self.v[smallest_child] >= self.v[i] {
                return;
            }
            self.v.swap(i, smallest_child);
            i = smallest_child;
        }
    }
}

// ---------- 2. in-place heapsort (max-heap on the slice itself) ----------
fn sift_down_max<T: Ord>(v: &mut [T], mut i: usize, end: usize) {
    loop {
        let mut big = 2 * i + 1;
        if big >= end {
            return;
        }
        if big + 1 < end && v[big + 1] > v[big] {
            big += 1;
        }
        if v[big] <= v[i] {
            return;
        }
        v.swap(i, big);
        i = big;
    }
}

fn heapsort<T: Ord>(v: &mut [T]) {
    let n = v.len();
    for i in (0..n / 2).rev() {
        sift_down_max(v, i, n); // build max-heap in O(n)
    }
    for end in (1..n).rev() {
        v.swap(0, end); // current max goes to its final slot
        sift_down_max(v, 0, end);
    }
}

// ---------- 3. indexed min-heap with decrease-key ----------
struct IndexedHeap {
    heap: Vec<usize>,        // ids in heap order
    pos: Vec<Option<usize>>, // pos[id] = index in `heap`
    key: Vec<u64>,
}

impl IndexedHeap {
    fn new(n: usize) -> Self {
        IndexedHeap {
            heap: vec![],
            pos: vec![None; n],
            key: vec![u64::MAX; n],
        }
    }

    fn is_empty(&self) -> bool {
        self.heap.is_empty()
    }

    /// insert `id`, or lower its key; ignores attempts that would raise it
    fn push_or_decrease(&mut self, id: usize, key: u64) {
        match self.pos[id] {
            Some(i) if key < self.key[id] => {
                self.key[id] = key;
                self.sift_up(i);
            }
            Some(_) => {}
            None => {
                self.key[id] = key;
                self.heap.push(id);
                self.pos[id] = Some(self.heap.len() - 1);
                self.sift_up(self.heap.len() - 1);
            }
        }
    }

    fn pop_min(&mut self) -> Option<(usize, u64)> {
        let top = *self.heap.first()?;
        let last = self.heap.len() - 1;
        self.swap(0, last);
        self.heap.pop();
        self.pos[top] = None;
        if !self.heap.is_empty() {
            self.sift_down(0);
        }
        Some((top, self.key[top]))
    }

    fn swap(&mut self, a: usize, b: usize) {
        self.heap.swap(a, b);
        self.pos[self.heap[a]] = Some(a); // the position map must follow every move
        self.pos[self.heap[b]] = Some(b);
    }

    fn sift_up(&mut self, mut i: usize) {
        while i > 0 && self.key[self.heap[i]] < self.key[self.heap[(i - 1) / 2]] {
            self.swap(i, (i - 1) / 2);
            i = (i - 1) / 2;
        }
    }

    fn sift_down(&mut self, mut i: usize) {
        loop {
            let (l, r) = (2 * i + 1, 2 * i + 2);
            let mut m = i;
            for c in [l, r] {
                if c < self.heap.len() && self.key[self.heap[c]] < self.key[self.heap[m]] {
                    m = c;
                }
            }
            if m == i {
                return;
            }
            self.swap(i, m);
            i = m;
        }
    }
}

// ---------- Dijkstra three ways ----------
type Graph = Vec<Vec<(usize, u64)>>;

fn random_graph(rng: &mut Rng, n: usize, m: usize) -> Graph {
    let mut g: Graph = vec![vec![]; n];
    for _ in 0..m {
        let (a, b, w) = (
            rng.below(n as u64) as usize,
            rng.below(n as u64) as usize,
            1 + rng.below(50),
        );
        g[a].push((b, w));
    }
    g
}

fn dijkstra_lazy(g: &Graph, src: usize) -> Vec<Option<u64>> {
    let mut dist = vec![None; g.len()];
    let mut h = BinaryHeap::new();
    dist[src] = Some(0);
    h.push(Reverse((0u64, src)));
    while let Some(Reverse((d, u))) = h.pop() {
        if dist[u].is_some_and(|b| d > b) {
            continue; // stale duplicate
        }
        for &(v, w) in &g[u] {
            if dist[v].is_none_or(|c| d + w < c) {
                dist[v] = Some(d + w);
                h.push(Reverse((d + w, v)));
            }
        }
    }
    dist
}

fn dijkstra_dary(g: &Graph, src: usize) -> Vec<Option<u64>> {
    let mut dist = vec![None; g.len()];
    let mut h: DHeap<(u64, usize), 4> = DHeap::new();
    dist[src] = Some(0);
    h.push((0, src));
    while let Some((d, u)) = h.pop() {
        if dist[u].is_some_and(|b| d > b) {
            continue;
        }
        for &(v, w) in &g[u] {
            if dist[v].is_none_or(|c| d + w < c) {
                dist[v] = Some(d + w);
                h.push((d + w, v));
            }
        }
    }
    dist
}

fn dijkstra_indexed(g: &Graph, src: usize) -> Vec<Option<u64>> {
    let mut dist = vec![None; g.len()];
    let mut h = IndexedHeap::new(g.len());
    h.push_or_decrease(src, 0);
    while let Some((u, d)) = h.pop_min() {
        dist[u] = Some(d); // popped exactly once: final
        for &(v, w) in &g[u] {
            if dist[v].is_none() {
                h.push_or_decrease(v, d + w); // no duplicates, heap size <= n
            }
        }
    }
    dist
}

// ---------- patterns ----------
/// k largest of a stream in O(n log k) time and O(k) memory: a MIN-heap of the current winners
fn top_k_stream(items: &[i64], k: usize) -> Vec<i64> {
    let mut h = BinaryHeap::new();
    for &x in items {
        h.push(Reverse(x));
        if h.len() > k {
            h.pop(); // evict the smallest winner
        }
    }
    let mut out: Vec<i64> = h.into_iter().map(|Reverse(x)| x).collect();
    out.sort_unstable_by(|a, b| b.cmp(a));
    out
}

/// merge k sorted lists: the heap always holds one "cursor" per list
fn k_way_merge(lists: &[Vec<i64>]) -> Vec<i64> {
    let mut h = BinaryHeap::new();
    for (li, l) in lists.iter().enumerate() {
        if let Some(&x) = l.first() {
            h.push(Reverse((x, li, 0usize)));
        }
    }
    let mut out = vec![];
    while let Some(Reverse((x, li, i))) = h.pop() {
        out.push(x);
        if let Some(&next) = lists[li].get(i + 1) {
            h.push(Reverse((next, li, i + 1)));
        }
    }
    out
}

/// lower median after each insertion: `lo` is a max-heap of the smaller half, `hi` a min-heap of the larger
fn running_median(items: &[i64]) -> Vec<i64> {
    let (mut lo, mut hi) = (BinaryHeap::new(), BinaryHeap::new());
    let mut out = vec![];
    for &x in items {
        if lo.peek().is_none_or(|&m| x <= m) {
            lo.push(x);
        } else {
            hi.push(Reverse(x));
        }
        if lo.len() > hi.len() + 1 {
            hi.push(Reverse(lo.pop().unwrap())); // rebalance: sizes differ by at most one, lo >= hi
        } else if hi.len() > lo.len() {
            lo.push(hi.pop().unwrap().0);
        }
        out.push(*lo.peek().unwrap());
    }
    out
}

/// max of every window of size w: a deque of indices whose values are decreasing; front = window max
fn sliding_max(v: &[i64], w: usize) -> Vec<i64> {
    let mut dq: VecDeque<usize> = VecDeque::new();
    let mut out = vec![];
    for i in 0..v.len() {
        while dq.back().is_some_and(|&b| v[b] <= v[i]) {
            dq.pop_back(); // dominated: older AND smaller can never be a max again
        }
        dq.push_back(i);
        if dq[0] + w <= i {
            dq.pop_front(); // fell out of the window
        }
        if i + 1 >= w {
            out.push(v[dq[0]]);
        }
    }
    out
}

/// merge half-open intervals that overlap or touch
fn merge_intervals(mut iv: Vec<(i64, i64)>) -> Vec<(i64, i64)> {
    iv.sort_unstable();
    let mut out: Vec<(i64, i64)> = vec![];
    for (s, e) in iv {
        match out.last_mut() {
            Some(last) if s <= last.1 => last.1 = last.1.max(e),
            _ => out.push((s, e)),
        }
    }
    out
}

/// minimum rooms: sort by start; a min-heap of end times; reuse a room if the earliest end <= new start
fn min_rooms(mut iv: Vec<(i64, i64)>) -> usize {
    iv.sort_unstable();
    let mut ends = BinaryHeap::new();
    for (s, e) in iv {
        if ends.peek().is_some_and(|&Reverse(end)| end <= s) {
            ends.pop();
        }
        ends.push(Reverse(e));
    }
    ends.len()
}

/// k closest points to the origin: max-heap of the best k so far, keyed (dist2, x, y) for deterministic ties
fn k_closest(points: &[(i64, i64)], k: usize) -> Vec<(i64, i64)> {
    let mut h = BinaryHeap::new();
    for &(x, y) in points {
        h.push((x * x + y * y, x, y));
        if h.len() > k {
            h.pop(); // drop the farthest
        }
    }
    let mut v = h.into_sorted_vec();
    v.truncate(k);
    v.into_iter().map(|(_, x, y)| (x, y)).collect()
}

fn random_intervals(rng: &mut Rng, n: usize) -> Vec<(i64, i64)> {
    (0..n)
        .map(|_| {
            let s = rng.below(60) as i64;
            (s, s + 1 + rng.below(12) as i64)
        })
        .collect()
}

fn main() {
    let mut rng = Rng::new(160);

    // 1. binary + d-ary heaps and heapsort against std sort
    for round in 0..200 {
        let v = rng.vec(round, 40);
        let mut want = v.clone();
        want.sort_unstable();
        let (mut b2, mut b4, mut b8) = (
            DHeap::<i64, 2>::heapify(v.clone()),
            DHeap::<i64, 4>::new(),
            DHeap::<i64, 8>::new(),
        );
        v.iter().for_each(|&x| b4.push(x));
        v.iter().for_each(|&x| b8.push(x));
        assert_eq!(b2.peek(), want.first());
        let drain2: Vec<i64> = std::iter::from_fn(|| b2.pop()).collect();
        let drain4: Vec<i64> = std::iter::from_fn(|| b4.pop()).collect();
        let drain8: Vec<i64> = std::iter::from_fn(|| b8.pop()).collect();
        assert!(drain2 == want && drain4 == want && drain8 == want);
        let mut hs = v.clone();
        heapsort(&mut hs);
        assert_eq!(hs, want);
    }
    println!(
        "binary / 4-ary / 8-ary heaps, heapify and heapsort agree with sort (200 random sizes)"
    );

    // 2. Dijkstra: lazy BinaryHeap (as dijkstra) vs d-ary vs indexed decrease-key
    for seed in 0..30 {
        let mut r = Rng::new(1000 + seed);
        let g = random_graph(&mut r, 300, 1500);
        let src = r.below(300) as usize;
        let lazy = dijkstra_lazy(&g, src);
        assert_eq!(lazy, dijkstra_dary(&g, src));
        assert_eq!(lazy, dijkstra_indexed(&g, src));
    }
    println!(
        "Dijkstra: lazy-deletion, 4-ary and indexed decrease-key give identical distances (30 graphs)"
    );

    // 3. top-k stream
    for k in [1, 3, 10] {
        let data = rng.vec(500, 1000);
        let mut want = data.clone();
        want.sort_unstable_by(|a, b| b.cmp(a));
        want.truncate(k);
        assert_eq!(top_k_stream(&data, k), want);
    }
    println!("top-k stream ok");

    // 4. k-way merge
    for _ in 0..50 {
        let lists: Vec<Vec<i64>> = (0..1 + rng.below(8))
            .map(|_| {
                let len = rng.below(30) as usize;
                let mut l = rng.vec(len, 100);
                l.sort_unstable();
                l
            })
            .collect();
        let mut want: Vec<i64> = lists.concat();
        want.sort_unstable();
        assert_eq!(k_way_merge(&lists), want);
    }
    println!("k-way merge ok");

    // 5. running median vs sorting every prefix
    let data = rng.vec(400, 200);
    let want: Vec<i64> = (1..=data.len())
        .map(|n| {
            let mut p = data[..n].to_vec();
            p.sort_unstable();
            p[(n - 1) / 2]
        })
        .collect();
    assert_eq!(running_median(&data), want);
    println!("running median ok; first 8 medians: {:?}", &want[..8]);

    // 6. sliding window maximum
    for w in [1, 2, 5, 17] {
        let data = rng.vec(300, 50); // small range: many ties
        let want: Vec<i64> = data
            .windows(w)
            .map(|win| *win.iter().max().unwrap())
            .collect();
        assert_eq!(sliding_max(&data, w), want);
    }
    println!("sliding-window maximum ok");

    // 7. merge intervals and meeting rooms vs a per-minute occupancy array
    for _ in 0..100 {
        let n = 1 + rng.below(25) as usize;
        let iv = random_intervals(&mut rng, n);
        let mut busy = [0usize; 80];
        for &(s, e) in &iv {
            (s..e).for_each(|t| busy[t as usize] += 1);
        }
        assert_eq!(min_rooms(iv.clone()), *busy.iter().max().unwrap());
        let mut runs = vec![];
        let mut t = 0;
        while t < 80 {
            if busy[t] > 0 {
                let s = t;
                while t < 80 && busy[t] > 0 {
                    t += 1;
                }
                runs.push((s as i64, t as i64));
            } else {
                t += 1;
            }
        }
        assert_eq!(merge_intervals(iv), runs);
    }
    println!("merge intervals / meeting rooms ok");

    // 8. k closest points
    for k in [1, 4, 9] {
        let pts: Vec<(i64, i64)> = (0..200)
            .map(|_| (rng.below(41) as i64 - 20, rng.below(41) as i64 - 20))
            .collect();
        let mut want = pts.clone();
        want.sort_by_key(|&(x, y)| (x * x + y * y, x, y));
        want.truncate(k);
        assert_eq!(k_closest(&pts, k), want);
    }
    println!("k closest points ok");
    println!("all heap pattern checks passed");
}
