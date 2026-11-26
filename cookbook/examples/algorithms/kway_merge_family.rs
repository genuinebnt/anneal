//! RECIPE 887 (algorithms): the k-way merge family: heap, loser tree, lazy iterator, LSM compaction, merge path.
//! LEVEL: advanced
//! Use in: external sort, LSM-tree compaction, merging sorted shards/logs, top-k, parallel merge.   Run: cargo run --release --example kway_merge_family
//! COMMON USES: external merge sort passes in databases, LSM compaction (newest wins, tombstones), merging per-shard
//!   sorted query results, log aggregation, "k-th smallest" interview problems, Timsort's galloping merge, parallel merge.
//! Course: CMU 15-445 (external merge sort, LSM trees) for the database uses.
#![allow(dead_code)]
//!
//! WHAT: one idea (repeatedly take the smallest head among k sorted sources) and its many costumes. Part 1 compares
//! a binary-heap merge with a loser (tournament) tree by COUNTING key comparisons: the heap needs about 2 log k per
//! output (compare the two children, then the better child with the parent), the loser tree needs about log k because
//! each node remembers who LOST the match, so after a source advances it replays only its own leaf-to-root path.
//! Part 2 is a lazy `Iterator` adapter generic over `Ord`, stable on ties by source index (the tie-break is part of the
//! heap key). Part 3 applies it: external-sort passes with block buffers and IO counts, LSM compaction with newest-wins
//! dedupe and tombstones, k sorted linked lists. Part 4 is the interview classics that are all "k-way merge in disguise":
//! smallest range covering one element of each list, k-th smallest in a sorted matrix, k smallest pair sums, k-th ugly
//! number, top-k from a stream, median of k sorted arrays. Part 5 is two-way merge variants: galloping (Timsort), branchless,
//! and merge path (split the output by anti-diagonals so p threads merge independent slices).
//! WHY the stable tie-break matters: LSM compaction must prefer the newest run, a database sort-merge join must keep
//! input order, and parallel merge must give the same bytes as the sequential one.
//! Every piece is checked against a "concatenate and sort" oracle on 2000 seeded random inputs including empty lists,
//! many duplicates and all-equal keys. Comparison counts are asserted (loser tree total <= heap total); timings are not.
//! Read: https://en.wikipedia.org/wiki/K-way_merge_algorithm and https://en.wikipedia.org/wiki/Tournament_sort and
//!   https://github.com/python/cpython/blob/main/Objects/listsort.txt (galloping mode, section "Galloping").
//!   Merge path: Green, McColl, Bader, "GPU Merge Path" (ICS 2012), cited by title only (not re-read here).
//! TRY NEXT:
//!   - [S] Change `loser_tree_merge` to count the sentinel (exhausted-source) comparisons too; does the total still stay <= `heap_merge`? Predict, then print both sums.
//!   - [S] In `gallop_merge` change MIN_GALLOP from 7 to 1 and to 64; which input shape in `main` gets worse for each, and by how many comparisons?
//!   - [M] Replace the `BinaryHeap` inside `KMerge` with your own d-ary heap (d = 4); measure heap comparisons vs d for k = 64 and explain the trend.
//!   - [M] Make `lsm_compact` streaming: take `Vec<Box<dyn Iterator<Item = Kv>>>` and write to a `Write` sink, keeping memory O(k); test against the current oracle.
//!   - [A] PROJECT: combine this with `sorting_networks_external_sort`: build a full external sort (run generation with a replacement-selection tournament, then multi-pass merge with fan-in F) and check IO counts against 2*N/B*(1+passes).
//!   - [A] PROJECT: combine with `thread_pool_work_stealing`: run `merge_path_parallel` as tasks on the pool and compare against the scoped-thread version.
use std::cell::Cell;
use std::cmp::{Ordering, Reverse};
use std::collections::{BinaryHeap, HashMap};

struct Rng(u64);
impl Rng {
    fn next(&mut self) -> u64 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        self.0
    }
    fn below(&mut self, n: u64) -> u64 {
        self.next() % n.max(1)
    }
}

/// k sorted lists; shapes cycle through: wide values, many duplicates, all equal. Lists may be empty.
fn gen_lists(r: &mut Rng, round: usize) -> Vec<Vec<u32>> {
    let k = 1 + r.below(9) as usize;
    let maxv = match round % 3 {
        0 => 1000,
        1 => 4,
        _ => 1, // all equal
    };
    (0..k)
        .map(|_| {
            let len = if r.below(5) == 0 {
                0
            } else {
                r.below(12) as usize
            };
            let mut v: Vec<u32> = (0..len).map(|_| r.below(maxv) as u32).collect();
            v.sort();
            v
        })
        .collect()
}

fn oracle(lists: &[Vec<u32>]) -> Vec<u32> {
    let mut all: Vec<u32> = lists.iter().flatten().copied().collect();
    all.sort();
    all
}

// ---------------------------------------------------------------------------------------------
// Part 1: heap vs loser tree, counting comparisons between two REAL keys.
// ---------------------------------------------------------------------------------------------

fn less(a: &(u32, usize), b: &(u32, usize), c: &mut u64) -> bool {
    *c += 1;
    a < b // (value, source index): ties go to the lower source, so the merge is stable
}

fn sift_down(h: &mut [(u32, usize)], mut i: usize, c: &mut u64) {
    loop {
        let l = 2 * i + 1;
        if l >= h.len() {
            break;
        }
        let mut m = l;
        if l + 1 < h.len() && less(&h[l + 1], &h[l], c) {
            m = l + 1;
        }
        if less(&h[m], &h[i], c) {
            h.swap(m, i);
            i = m;
        } else {
            break;
        }
    }
}

/// Binary-heap merge with "replace top" (one sift per output, not pop+push). Returns (output, comparisons).
fn heap_merge(lists: &[Vec<u32>]) -> (Vec<u32>, u64) {
    let mut c = 0;
    let mut pos = vec![0usize; lists.len()];
    let mut heap: Vec<(u32, usize)> = lists
        .iter()
        .enumerate()
        .filter(|(_, l)| !l.is_empty())
        .map(|(i, l)| (l[0], i))
        .collect();
    for i in (0..heap.len() / 2).rev() {
        sift_down(&mut heap, i, &mut c);
    }
    let mut out = Vec::new();
    while !heap.is_empty() {
        let (v, s) = heap[0];
        out.push(v);
        pos[s] += 1;
        if pos[s] < lists[s].len() {
            heap[0] = (lists[s][pos[s]], s);
        } else {
            let last = heap.pop().unwrap();
            if !heap.is_empty() {
                heap[0] = last;
            }
        }
        sift_down(&mut heap, 0, &mut c);
    }
    (out, c)
}

type Head = Option<(u32, usize)>; // None = exhausted = +infinity

fn beats(h: &[Head], a: usize, b: usize, c: &mut u64) -> bool {
    match (h[a], h[b]) {
        (Some(x), Some(y)) => {
            *c += 1;
            x < y
        }
        (Some(_), None) => true, // sentinel: free, no real key compared
        _ => false,
    }
}

/// Loser tree over K = next_pow2(k) leaves. `tree[node]` holds the LOSER of the match at that node; the overall winner
/// is carried separately. After the winner's source advances, only its path to the root is replayed.
struct LoserTree {
    heads: Vec<Head>,
    tree: Vec<usize>,
    winner: usize,
    k: usize,
}

impl LoserTree {
    fn new(lists: &[Vec<u32>], c: &mut u64) -> Self {
        let k = lists.len().next_power_of_two().max(1);
        let mut heads: Vec<Head> = vec![None; k];
        for (i, l) in lists.iter().enumerate() {
            heads[i] = l.first().map(|&v| (v, i));
        }
        let mut t = LoserTree {
            heads,
            tree: vec![0; k],
            winner: 0,
            k,
        };
        t.winner = t.build(1, c);
        t
    }
    fn build(&mut self, node: usize, c: &mut u64) -> usize {
        if node >= self.k {
            return node - self.k;
        }
        let (a, b) = (self.build(2 * node, c), self.build(2 * node + 1, c));
        if beats(&self.heads, a, b, c) {
            self.tree[node] = b;
            a
        } else {
            self.tree[node] = a;
            b
        }
    }
    /// Replace the winner's head with `next` and replay its root path.
    fn replace(&mut self, next: Head, c: &mut u64) {
        self.heads[self.winner] = next;
        let mut cur = self.winner;
        let mut node = (self.k + cur) / 2;
        while node >= 1 {
            if beats(&self.heads, self.tree[node], cur, c) {
                std::mem::swap(&mut self.tree[node], &mut cur);
            }
            node /= 2;
        }
        self.winner = cur;
    }
}

fn loser_tree_merge(lists: &[Vec<u32>]) -> (Vec<u32>, u64) {
    let mut c = 0;
    let mut t = LoserTree::new(lists, &mut c);
    let mut pos = vec![0usize; lists.len()];
    let mut out = Vec::new();
    while let Some((v, s)) = t.heads[t.winner] {
        out.push(v);
        pos[s] += 1;
        let next = lists[s].get(pos[s]).map(|&x| (x, s));
        t.replace(next, &mut c);
    }
    (out, c)
}

// ---------------------------------------------------------------------------------------------
// Part 2: lazy iterator adapter, generic over Ord, stable by source index.
// ---------------------------------------------------------------------------------------------

struct Entry<T>(T, usize);
impl<T: Ord> PartialEq for Entry<T> {
    fn eq(&self, o: &Self) -> bool {
        self.cmp(o) == Ordering::Equal
    }
}
impl<T: Ord> Eq for Entry<T> {}
impl<T: Ord> PartialOrd for Entry<T> {
    fn partial_cmp(&self, o: &Self) -> Option<Ordering> {
        Some(self.cmp(o))
    }
}
impl<T: Ord> Ord for Entry<T> {
    fn cmp(&self, o: &Self) -> Ordering {
        self.0.cmp(&o.0).then(self.1.cmp(&o.1)) // key first, then source index = stability
    }
}

/// Lazy k-way merge. Memory is O(k) no matter how long the sources are.
struct KMerge<I: Iterator>
where
    I::Item: Ord,
{
    iters: Vec<I>,
    heap: BinaryHeap<Reverse<Entry<I::Item>>>,
}

fn kmerge<I>(sources: impl IntoIterator<Item = I>) -> KMerge<I>
where
    I: Iterator,
    I::Item: Ord,
{
    let mut iters: Vec<I> = sources.into_iter().collect();
    let mut heap = BinaryHeap::new();
    for (i, it) in iters.iter_mut().enumerate() {
        if let Some(x) = it.next() {
            heap.push(Reverse(Entry(x, i)));
        }
    }
    KMerge { iters, heap }
}

impl<I: Iterator> Iterator for KMerge<I>
where
    I::Item: Ord,
{
    type Item = I::Item;
    fn next(&mut self) -> Option<I::Item> {
        let Reverse(Entry(item, s)) = self.heap.pop()?;
        if let Some(n) = self.iters[s].next() {
            self.heap.push(Reverse(Entry(n, s)));
        }
        Some(item)
    }
}

/// Key-only ordering with a payload: lets tests observe tie order.
#[derive(Clone, Copy, Debug)]
struct Tagged {
    key: u32,
    tag: u32,
}
impl PartialEq for Tagged {
    fn eq(&self, o: &Self) -> bool {
        self.key == o.key
    }
}
impl Eq for Tagged {}
impl PartialOrd for Tagged {
    fn partial_cmp(&self, o: &Self) -> Option<Ordering> {
        Some(self.cmp(o))
    }
}
impl Ord for Tagged {
    fn cmp(&self, o: &Self) -> Ordering {
        self.key.cmp(&o.key)
    }
}

// ---------------------------------------------------------------------------------------------
// Part 3: applications.
// ---------------------------------------------------------------------------------------------

/// A run on "disk": we count block reads when the reader's buffer of `b` elements runs dry.
struct RunReader<'a> {
    data: &'a [u32],
    pos: usize,
    buffered_until: usize,
    b: usize,
    reads: u64,
}
impl<'a> RunReader<'a> {
    fn new(data: &'a [u32], b: usize) -> Self {
        RunReader {
            data,
            pos: 0,
            buffered_until: 0,
            b,
            reads: 0,
        }
    }
    fn peek(&mut self) -> Option<u32> {
        if self.pos >= self.data.len() {
            return None;
        }
        if self.pos == self.buffered_until {
            self.buffered_until = (self.pos + self.b).min(self.data.len());
            self.reads += 1; // one block IO
        }
        Some(self.data[self.pos])
    }
}

/// One merge pass of up to `fan_in` runs at a time. Returns (new runs, block reads, block writes).
fn merge_pass(runs: &[Vec<u32>], fan_in: usize, b: usize) -> (Vec<Vec<u32>>, u64, u64) {
    let (mut reads, mut writes) = (0, 0);
    let mut next = Vec::new();
    for group in runs.chunks(fan_in) {
        let mut rd: Vec<RunReader> = group.iter().map(|r| RunReader::new(r, b)).collect();
        let mut out = Vec::new();
        loop {
            let mut best: Option<(u32, usize)> = None;
            for (i, r) in rd.iter_mut().enumerate() {
                if let Some(v) = r.peek() {
                    if best.is_none_or(|(bv, _)| v < bv) {
                        best = Some((v, i));
                    }
                }
            }
            let Some((v, i)) = best else { break };
            rd[i].pos += 1;
            out.push(v);
        }
        reads += rd.iter().map(|r| r.reads).sum::<u64>();
        writes += out.len().div_ceil(b) as u64; // output buffer flushed block by block
        next.push(out);
    }
    (next, reads, writes)
}

fn external_sort(mut runs: Vec<Vec<u32>>, fan_in: usize, b: usize) -> (Vec<u32>, usize, u64) {
    let (mut passes, mut io) = (0, 0);
    while runs.len() > 1 {
        let (n, r, w) = merge_pass(&runs, fan_in, b);
        runs = n;
        passes += 1;
        io += r + w;
    }
    (runs.pop().unwrap_or_default(), passes, io)
}

type Kv = (u32, Option<u32>); // key, value (None = tombstone)

/// `levels[0]` is the NEWEST run; each run sorted by unique key. Newest wins; tombstones are dropped only when
/// compacting into the bottom level (otherwise they must survive to shadow older data in deeper levels).
fn lsm_compact(levels: &[Vec<Kv>], bottom: bool) -> Vec<Kv> {
    #[derive(Clone, Copy)]
    struct ByKey(Kv);
    impl PartialEq for ByKey {
        fn eq(&self, o: &Self) -> bool {
            self.0.0 == o.0.0
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
            self.0.0.cmp(&o.0.0)
        }
    }
    let mut out: Vec<Kv> = Vec::new();
    for ByKey(kv) in kmerge(levels.iter().map(|l| l.iter().map(|&x| ByKey(x)))) {
        if out.last().is_some_and(|l| l.0 == kv.0) {
            continue; // an equal key from an older (higher index) run: shadowed
        }
        out.push(kv);
    }
    if bottom {
        out.retain(|kv| kv.1.is_some());
    }
    out
}

fn lsm_oracle(levels: &[Vec<Kv>], bottom: bool) -> Vec<Kv> {
    let mut m: HashMap<u32, Option<u32>> = HashMap::new();
    for l in levels.iter().rev() {
        for &(k, v) in l {
            m.insert(k, v); // later (newer) insert overwrites
        }
    }
    let mut v: Vec<Kv> = m
        .into_iter()
        .filter(|kv| !bottom || kv.1.is_some())
        .collect();
    v.sort();
    v
}

type List = Option<Box<Node>>;
struct Node {
    val: u32,
    next: List,
}
fn to_list(v: &[u32]) -> List {
    v.iter()
        .rev()
        .fold(None, |next, &val| Some(Box::new(Node { val, next })))
}
fn from_list(mut l: &List) -> Vec<u32> {
    let mut v = Vec::new();
    while let Some(n) = l {
        v.push(n.val);
        l = &n.next;
    }
    v
}
fn merge2(mut a: List, mut b: List) -> List {
    let mut head = None;
    let mut tail = &mut head;
    loop {
        let take_a = match (&a, &b) {
            (None, None) => break,
            (Some(_), None) => true,
            (None, Some(_)) => false,
            (Some(x), Some(y)) => x.val <= y.val,
        };
        let src = if take_a { &mut a } else { &mut b };
        let mut node = src.take().unwrap();
        *src = node.next.take();
        *tail = Some(node);
        tail = &mut tail.as_mut().unwrap().next;
    }
    head
}
/// Divide and conquer: log2(k) rounds of pairwise merges = O(N log k) like the heap, but relinks nodes in place.
fn merge_k_lists(mut lists: Vec<List>) -> List {
    while lists.len() > 1 {
        let mut next = Vec::new();
        let mut it = lists.into_iter();
        while let Some(a) = it.next() {
            next.push(merge2(a, it.next().unwrap_or(None)));
        }
        lists = next;
    }
    lists.pop().unwrap_or(None)
}

// ---------------------------------------------------------------------------------------------
// Part 4: classics.
// ---------------------------------------------------------------------------------------------

/// Smallest [lo, hi] containing at least one element of every list (ties: smallest lo). None if any list is empty.
fn smallest_range(lists: &[Vec<i64>]) -> Option<(i64, i64)> {
    if lists.is_empty() || lists.iter().any(|l| l.is_empty()) {
        return None;
    }
    let mut heap = BinaryHeap::new();
    let mut cur_max = i64::MIN;
    for (i, l) in lists.iter().enumerate() {
        heap.push(Reverse((l[0], i, 0usize)));
        cur_max = cur_max.max(l[0]);
    }
    let mut best = (heap.peek().unwrap().0.0, cur_max);
    while let Some(Reverse((v, i, j))) = heap.pop() {
        if cur_max - v < best.1 - best.0 {
            best = (v, cur_max);
        }
        if j + 1 == lists[i].len() {
            break; // one list exhausted: no window can cover all lists any more
        }
        let n = lists[i][j + 1];
        cur_max = cur_max.max(n);
        heap.push(Reverse((n, i, j + 1)));
    }
    Some(best)
}

fn smallest_range_brute(lists: &[Vec<i64>]) -> Option<(i64, i64)> {
    if lists.is_empty() || lists.iter().any(|l| l.is_empty()) {
        return None;
    }
    let mut los: Vec<i64> = lists.iter().flatten().copied().collect();
    los.sort();
    let mut best: Option<(i64, i64)> = None;
    for lo in los {
        let hi = lists
            .iter()
            .map(|l| l.iter().copied().find(|&x| x >= lo))
            .collect::<Option<Vec<_>>>()
            .map(|v| v.into_iter().max().unwrap());
        if let Some(hi) = hi {
            if best.is_none_or(|b| hi - lo < b.1 - b.0) {
                best = Some((lo, hi));
            }
        }
    }
    best
}

/// k-th smallest (1-based) in a matrix whose ROWS are sorted: k-way merge over rows, stop after k pops.
fn kth_in_matrix(m: &[Vec<i64>], k: usize) -> i64 {
    let mut heap = BinaryHeap::new();
    for (i, r) in m.iter().enumerate() {
        heap.push(Reverse((r[0], i, 0usize)));
    }
    for _ in 1..k {
        let Reverse((_, i, j)) = heap.pop().unwrap();
        if j + 1 < m[i].len() {
            heap.push(Reverse((m[i][j + 1], i, j + 1)));
        }
    }
    heap.pop().unwrap().0.0
}

/// k smallest sums a[i] + b[j]: row i of the implicit matrix is sorted, so seed one entry per row.
fn k_smallest_pair_sums(a: &[i64], b: &[i64], k: usize) -> Vec<i64> {
    let mut heap = BinaryHeap::new();
    for i in 0..a.len().min(k) {
        heap.push(Reverse((a[i] + b[0], i, 0usize)));
    }
    let mut out = Vec::new();
    while out.len() < k {
        let Some(Reverse((s, i, j))) = heap.pop() else {
            break;
        };
        out.push(s);
        if j + 1 < b.len() {
            heap.push(Reverse((a[i] + b[j + 1], i, j + 1)));
        }
    }
    out
}

/// n-th (1-based) number whose only prime factors come from `primes`: merge the streams p * ugly[idx_p].
fn nth_ugly(n: usize, primes: &[u64]) -> u64 {
    let mut ugly = vec![1u64];
    let mut idx = vec![0usize; primes.len()];
    while ugly.len() < n {
        let next = primes
            .iter()
            .zip(&idx)
            .map(|(&p, &i)| p * ugly[i])
            .min()
            .unwrap();
        ugly.push(next);
        for (k, &p) in primes.iter().enumerate() {
            if p * ugly[idx[k]] == next {
                idx[k] += 1; // advance every stream that produced it: dedupes in the same step
            }
        }
    }
    ugly[n - 1]
}

fn is_ugly(mut x: u64, primes: &[u64]) -> bool {
    for &p in primes {
        while x % p == 0 {
            x /= p;
        }
    }
    x == 1
}

/// Top-k largest of a stream with a size-k min-heap: O(n log k), O(k) memory.
fn top_k(stream: impl Iterator<Item = i64>, k: usize) -> Vec<i64> {
    let mut h: BinaryHeap<Reverse<i64>> = BinaryHeap::new();
    for x in stream {
        if h.len() < k {
            h.push(Reverse(x));
        } else if k > 0 && x > h.peek().unwrap().0 {
            h.pop();
            h.push(Reverse(x));
        }
    }
    let mut v: Vec<i64> = h.into_iter().map(|r| r.0).collect();
    v.sort_by(|a, b| b.cmp(a));
    v
}

/// 0-based k-th smallest over several sorted arrays by binary search on the VALUE: O(k_arrays * log(range) * log n).
fn kth_of_arrays(arrs: &[Vec<i64>], k: usize) -> i64 {
    let (mut lo, mut hi) = (i64::MIN / 2, i64::MAX / 2);
    while lo < hi {
        let mid = lo + (hi - lo) / 2;
        let cnt: usize = arrs.iter().map(|a| a.partition_point(|&x| x <= mid)).sum();
        if cnt > k {
            hi = mid; // at least k+1 elements <= mid: answer <= mid
        } else {
            lo = mid + 1;
        }
    }
    lo
}

fn median_of_arrays(arrs: &[Vec<i64>]) -> (i64, i64) {
    let n: usize = arrs.iter().map(|a| a.len()).sum();
    (kth_of_arrays(arrs, (n - 1) / 2), kth_of_arrays(arrs, n / 2))
}

// ---------------------------------------------------------------------------------------------
// Part 5: two-way merge variants.
// ---------------------------------------------------------------------------------------------

fn linear_merge_count(a: &[u32], b: &[u32]) -> (Vec<u32>, u64) {
    let (mut i, mut j, mut c) = (0, 0, 0u64);
    let mut out = Vec::with_capacity(a.len() + b.len());
    while i < a.len() && j < b.len() {
        c += 1;
        if b[j] < a[i] {
            out.push(b[j]);
            j += 1;
        } else {
            out.push(a[i]);
            i += 1;
        }
    }
    out.extend_from_slice(&a[i..]);
    out.extend_from_slice(&b[j..]);
    (out, c)
}

/// Number of leading elements of `s` for which `pred` holds (pred monotone: true..true false..false),
/// found by exponential probing then binary search: O(log answer) instead of O(answer).
fn gallop_count(s: &[u32], pred: impl Fn(u32) -> bool, c: &Cell<u64>) -> usize {
    let test = |x: u32| {
        c.set(c.get() + 1);
        pred(x)
    };
    let (mut lo, mut hi) = (0usize, 1usize);
    while hi <= s.len() && test(s[hi - 1]) {
        lo = hi;
        hi *= 2;
    }
    let upper = if hi <= s.len() { hi - 1 } else { s.len() };
    let (mut l, mut h) = (lo, upper);
    while l < h {
        let mid = (l + h) / 2;
        if test(s[mid]) {
            l = mid + 1;
        } else {
            h = mid;
        }
    }
    l
}

const MIN_GALLOP: u32 = 7;

/// Timsort-style merge: after MIN_GALLOP consecutive wins by one side, switch to galloping that side in bulk.
fn gallop_merge(a: &[u32], b: &[u32]) -> (Vec<u32>, u64) {
    let c = Cell::new(0u64);
    let (mut i, mut j) = (0, 0);
    let (mut wa, mut wb) = (0u32, 0u32);
    let mut out = Vec::with_capacity(a.len() + b.len());
    while i < a.len() && j < b.len() {
        c.set(c.get() + 1);
        if b[j] < a[i] {
            out.push(b[j]);
            j += 1;
            wb += 1;
            wa = 0;
        } else {
            out.push(a[i]);
            i += 1;
            wa += 1;
            wb = 0;
        }
        if i >= a.len() || j >= b.len() {
            break;
        }
        if wa >= MIN_GALLOP {
            let bj = b[j];
            let n = gallop_count(&a[i..], |x| x <= bj, &c); // a-elements that precede b[j] (ties: a first = stable)
            out.extend_from_slice(&a[i..i + n]);
            i += n;
            wa = 0;
        } else if wb >= MIN_GALLOP {
            let ai = a[i];
            let n = gallop_count(&b[j..], |x| x < ai, &c);
            out.extend_from_slice(&b[j..j + n]);
            j += n;
            wb = 0;
        }
    }
    out.extend_from_slice(&a[i..]);
    out.extend_from_slice(&b[j..]);
    (out, c.get())
}

/// Branch-free merge using +infinity sentinels (keys must be < u32::MAX): the only data-dependent choice is a
/// `bool as usize`, which compiles to setcc/cmov, so random data causes no branch mispredictions.
fn merge_branchless(a: &[u32], b: &[u32]) -> Vec<u32> {
    let n = a.len() + b.len();
    let mut ae = a.to_vec();
    ae.push(u32::MAX);
    let mut be = b.to_vec();
    be.push(u32::MAX);
    let mut out = vec![0u32; n];
    let (mut i, mut j) = (0usize, 0usize);
    for o in out.iter_mut() {
        let (x, y) = (ae[i], be[j]);
        let t = (y < x) as usize;
        *o = if t == 1 { y } else { x };
        j += t;
        i += 1 - t;
    }
    out
}

/// Merge path: how many of the first `d` merged outputs come from `a` (ties favour `a`). Binary search along the
/// anti-diagonal i + j = d; the predicate "a[mid] is among the first d" is monotone in mid.
fn diag_split<T: Ord>(a: &[T], b: &[T], d: usize) -> usize {
    let mut lo = d.saturating_sub(b.len());
    let mut hi = d.min(a.len());
    while lo < hi {
        let mid = (lo + hi) / 2;
        if a[mid] <= b[d - 1 - mid] {
            lo = mid + 1;
        } else {
            hi = mid;
        }
    }
    lo
}

fn merge_seq<T: Ord + Copy>(a: &[T], b: &[T], out: &mut [T]) {
    let (mut i, mut j) = (0, 0);
    for o in out.iter_mut() {
        if j >= b.len() || (i < a.len() && a[i] <= b[j]) {
            *o = a[i];
            i += 1;
        } else {
            *o = b[j];
            j += 1;
        }
    }
}

/// p threads each merge an exactly equal share of the output, independent of the data distribution
/// (unlike splitting `a` into p pieces and binary searching `b`, which can be badly imbalanced).
fn merge_path_parallel<T: Ord + Copy + Send + Sync>(a: &[T], b: &[T], p: usize) -> Vec<T> {
    let n = a.len() + b.len();
    let mut out: Vec<T> = if n == 0 {
        Vec::new()
    } else {
        vec![if a.is_empty() { b[0] } else { a[0] }; n]
    };
    let cuts: Vec<(usize, usize)> = (0..=p)
        .map(|t| {
            let d = t * n / p;
            let i = if d == 0 { 0 } else { diag_split(a, b, d) };
            (i, d - i)
        })
        .collect();
    std::thread::scope(|s| {
        let mut rest = out.as_mut_slice();
        for w in cuts.windows(2) {
            let (lo, hi) = (w[0], w[1]);
            let (chunk, r) = rest.split_at_mut((hi.0 + hi.1) - (lo.0 + lo.1));
            rest = r;
            s.spawn(move || merge_seq(&a[lo.0..hi.0], &b[lo.1..hi.1], chunk));
        }
    });
    out
}

// ---------------------------------------------------------------------------------------------

fn main() {
    let mut r = Rng(0x887_C0FF_EE);
    const N: usize = 2000;

    // Part 1: heap vs loser tree.
    let (mut sum_heap, mut sum_loser) = (0u64, 0u64);
    for round in 0..N {
        let lists = gen_lists(&mut r, round);
        let want = oracle(&lists);
        let (h, hc) = heap_merge(&lists);
        let (l, lc) = loser_tree_merge(&lists);
        assert_eq!(h, want, "heap merge");
        assert_eq!(l, want, "loser tree merge");
        sum_heap += hc;
        sum_loser += lc;
    }
    assert!(
        sum_loser <= sum_heap,
        "loser tree {sum_loser} should not exceed heap {sum_heap}"
    );
    println!(
        "part 1: {N} instances: comparisons heap = {sum_heap}, loser tree = {sum_loser} (ratio {:.2})",
        sum_heap as f64 / sum_loser as f64
    );
    // one large instance shows the log k vs 2 log k gap clearly
    let big: Vec<Vec<u32>> = (0..64)
        .map(|_| {
            let mut v: Vec<u32> = (0..500).map(|_| r.below(1 << 20) as u32).collect();
            v.sort();
            v
        })
        .collect();
    let ((hb, hc), (lb, lc)) = (heap_merge(&big), loser_tree_merge(&big));
    assert_eq!(hb, lb);
    assert!(lc < hc);
    println!(
        "        k=64, 32000 keys: heap {hc} vs loser tree {lc} comparisons ({:.2} vs {:.2} per output)",
        hc as f64 / 32000.0,
        lc as f64 / 32000.0
    );

    // Part 2: lazy adapter, stability, laziness.
    for round in 0..N {
        let lists = gen_lists(&mut r, round);
        let tagged: Vec<Vec<Tagged>> = lists
            .iter()
            .enumerate()
            .map(|(i, l)| {
                l.iter()
                    .map(|&k| Tagged {
                        key: k,
                        tag: i as u32,
                    })
                    .collect()
            })
            .collect();
        let got: Vec<(u32, u32)> = kmerge(tagged.iter().map(|l| l.iter().copied()))
            .map(|t| (t.key, t.tag))
            .collect();
        let mut want: Vec<(u32, u32)> = tagged.iter().flatten().map(|t| (t.key, t.tag)).collect();
        want.sort_by_key(|x| x.0); // std sort is stable: concatenation order = source order
        assert_eq!(got, want, "stable on ties by source index");
    }
    let mut lazy = kmerge((0..3).map(|s| (0..).map(move |x| x * 3 + s as u64))); // infinite sources!
    let first: Vec<u64> = lazy.by_ref().take(7).collect();
    assert_eq!(first, vec![0, 1, 2, 3, 4, 5, 6]);
    println!("part 2: stable lazy KMerge ok; merged 3 infinite iterators: {first:?}");

    // Part 3a: external sort passes and IO counts.
    for round in 0..N {
        let nruns = 1 + r.below(12) as usize;
        let b = 1 + r.below(6) as usize;
        let fan_in = 2 + r.below(4) as usize;
        let runs: Vec<Vec<u32>> = (0..nruns)
            .map(|_| {
                let mut v: Vec<u32> = (0..r.below(30))
                    .map(|_| r.below(if round % 2 == 0 { 100 } else { 2 }) as u32)
                    .collect();
                v.sort();
                v
            })
            .collect();
        let want = oracle(&runs);
        let (got, passes, io) = external_sort(runs.clone(), fan_in, b);
        assert_eq!(got, want);
        let (mut left, mut expect_passes) = (nruns, 0);
        while left > 1 {
            left = left.div_ceil(fan_in);
            expect_passes += 1;
        }
        assert_eq!(passes, expect_passes, "passes = ceil(log_F runs)");
        if nruns > 1 {
            let (_, rd, wr) = merge_pass(&runs, fan_in, b);
            let exp_rd: u64 = runs.iter().map(|x| x.len().div_ceil(b) as u64).sum();
            assert_eq!(
                rd, exp_rd,
                "every block of every run is read exactly once per pass"
            );
            let exp_wr: u64 = runs
                .chunks(fan_in)
                .map(|g| g.iter().map(|x| x.len()).sum::<usize>().div_ceil(b) as u64)
                .sum();
            assert_eq!(wr, exp_wr);
            assert!(io >= rd + wr);
        }
    }
    println!(
        "part 3a: external merge sort: block reads/writes match ceil(len/B), passes = ceil(log_F runs)"
    );

    // Part 3b: LSM compaction.
    for round in 0..N {
        let nlev = 1 + r.below(5) as usize;
        let levels: Vec<Vec<Kv>> = (0..nlev)
            .map(|lv| {
                let mut m: std::collections::BTreeMap<u32, Option<u32>> = Default::default();
                for _ in 0..r.below(10) {
                    let k = r.below(12) as u32;
                    m.insert(
                        k,
                        if r.below(4) == 0 {
                            None
                        } else {
                            Some((lv * 100) as u32 + r.below(100) as u32)
                        },
                    );
                }
                m.into_iter().collect()
            })
            .collect();
        for bottom in [false, true] {
            assert_eq!(
                lsm_compact(&levels, bottom),
                lsm_oracle(&levels, bottom),
                "round {round} bottom={bottom}"
            );
        }
    }
    let levels = vec![
        vec![(1, None), (3, Some(30))],
        vec![(1, Some(10)), (2, Some(20))],
        vec![(2, Some(2)), (4, Some(4))],
    ];
    assert_eq!(
        lsm_compact(&levels, true),
        vec![(2, Some(20)), (3, Some(30)), (4, Some(4))]
    );
    assert_eq!(
        lsm_compact(&levels, false),
        vec![(1, None), (2, Some(20)), (3, Some(30)), (4, Some(4))]
    );
    println!(
        "part 3b: LSM compaction: newest wins; tombstone kept above the bottom level, dropped at the bottom"
    );

    // Part 3c: linked lists.
    for round in 0..N {
        let lists = gen_lists(&mut r, round);
        let merged = merge_k_lists(lists.iter().map(|l| to_list(l)).collect());
        assert_eq!(from_list(&merged), oracle(&lists));
    }
    println!("part 3c: k sorted linked lists merged by pairwise rounds: ok");

    // Part 4: classics.
    for round in 0..N {
        let k = 1 + r.below(5) as usize;
        let maxv = if round % 3 == 0 { 2 } else { 40 };
        let lists: Vec<Vec<i64>> = (0..k)
            .map(|_| {
                let mut v: Vec<i64> = (0..r.below(7)).map(|_| r.below(maxv) as i64).collect();
                v.sort();
                v
            })
            .collect();
        assert_eq!(
            smallest_range(&lists),
            smallest_range_brute(&lists),
            "{lists:?}"
        );

        let (rows, cols) = (1 + r.below(5) as usize, 1 + r.below(5) as usize);
        let m: Vec<Vec<i64>> = (0..rows)
            .map(|_| {
                let mut v: Vec<i64> = (0..cols).map(|_| r.below(30) as i64).collect();
                v.sort();
                v
            })
            .collect();
        let mut flat: Vec<i64> = m.iter().flatten().copied().collect();
        flat.sort();
        let kk = 1 + r.below(flat.len() as u64) as usize;
        assert_eq!(kth_in_matrix(&m, kk), flat[kk - 1]);

        let mk = |r: &mut Rng| {
            let mut v: Vec<i64> = (0..1 + r.below(6))
                .map(|_| r.below(20) as i64 - 5)
                .collect();
            v.sort();
            v
        };
        let (a, b) = (mk(&mut r), mk(&mut r));
        let kk = 1 + r.below(10) as usize;
        let mut sums: Vec<i64> = a
            .iter()
            .flat_map(|x| b.iter().map(move |y| x + y))
            .collect();
        sums.sort();
        sums.truncate(kk);
        assert_eq!(k_smallest_pair_sums(&a, &b, kk), sums);

        let stream: Vec<i64> = (0..r.below(30)).map(|_| r.below(15) as i64).collect();
        let kk = r.below(8) as usize;
        let mut want = stream.clone();
        want.sort_by(|x, y| y.cmp(x));
        want.truncate(kk);
        assert_eq!(top_k(stream.into_iter(), kk), want);

        let arrs: Vec<Vec<i64>> = (0..1 + r.below(5))
            .map(|_| {
                let mut v: Vec<i64> = (0..r.below(6)).map(|_| r.below(21) as i64 - 10).collect();
                v.sort();
                v
            })
            .collect();
        let mut all: Vec<i64> = arrs.iter().flatten().copied().collect();
        if !all.is_empty() {
            all.sort();
            assert_eq!(
                median_of_arrays(&arrs),
                (all[(all.len() - 1) / 2], all[all.len() / 2])
            );
        }
    }
    for primes in [&[2u64, 3, 5][..], &[2, 7, 13, 19], &[3, 5]] {
        let mut want = Vec::new();
        let mut x = 1;
        while want.len() < 60 {
            if is_ugly(x, primes) {
                want.push(x);
            }
            x += 1;
        }
        for (i, &w) in want.iter().enumerate() {
            assert_eq!(nth_ugly(i + 1, primes), w);
        }
    }
    println!(
        "part 4: smallest range, matrix k-th, pair sums, ugly numbers, top-k, median of k arrays: ok ({N} rounds)"
    );

    // Part 5: two-way variants.
    for round in 0..N {
        let mk = |r: &mut Rng| {
            let mut v: Vec<u32> = (0..r.below(40))
                .map(|_| r.below(if round % 3 == 0 { 3 } else { 500 }) as u32)
                .collect();
            v.sort();
            v
        };
        let (a, b) = (mk(&mut r), mk(&mut r));
        let (want, _) = linear_merge_count(&a, &b);
        assert_eq!(gallop_merge(&a, &b).0, want, "gallop");
        assert_eq!(merge_branchless(&a, &b), want, "branchless");
        for p in [1, 2, 3, 5, 8] {
            assert_eq!(merge_path_parallel(&a, &b, p), want, "merge path p={p}");
        }
        // stability of merge path: payloads must come out in the same order as the sequential stable merge
        let ta: Vec<Tagged> = a.iter().map(|&k| Tagged { key: k, tag: 0 }).collect();
        let tb: Vec<Tagged> = b.iter().map(|&k| Tagged { key: k, tag: 1 }).collect();
        let mut seq = vec![Tagged { key: 0, tag: 9 }; ta.len() + tb.len()];
        merge_seq(&ta, &tb, &mut seq);
        let par = merge_path_parallel(&ta, &tb, 4);
        assert!(
            seq.iter()
                .zip(&par)
                .all(|(x, y)| x.key == y.key && x.tag == y.tag)
        );
    }
    let a: Vec<u32> = (0..100_000).collect();
    let b: Vec<u32> = (100_000..200_000).collect();
    let (_, lin) = linear_merge_count(&a, &b);
    let (g, gal) = gallop_merge(&a, &b);
    assert!(g.windows(2).all(|w| w[0] <= w[1]));
    assert!(
        gal * 100 < lin,
        "galloping should need far fewer comparisons on block-shaped input"
    );
    let ai: Vec<u32> = (0..50_000).map(|x| x * 2).collect();
    let bi: Vec<u32> = (0..50_000).map(|x| x * 2 + 1).collect();
    let (_, lin2) = linear_merge_count(&ai, &bi);
    let (_, gal2) = gallop_merge(&ai, &bi);
    println!(
        "part 5: block-shaped input: linear {lin} vs galloping {gal} comparisons; interleaved: linear {lin2} vs galloping {gal2}"
    );
    // merge path balance: even when all of `a` precedes `b`, every thread gets n/p outputs
    let cuts: Vec<usize> = (0..=4)
        .map(|t| diag_split(&a, &b, t * 200_000 / 4))
        .collect();
    assert_eq!(cuts, vec![0, 50_000, 100_000, 100_000, 100_000]);
    println!(
        "        merge path cuts for a<b, p=4: i = {cuts:?} (each thread merges exactly 50000 outputs)"
    );
    println!("all kway_merge_family checks passed");
}
