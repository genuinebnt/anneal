//! RECIPE 888 (algorithms): algorithmic optimisation toolbox: naive vs optimised pairs with asserted operation counts.
//! LEVEL: advanced
//! Use in: any "it passes the small tests but times out" moment; choosing the right asymptotic idea before micro-tuning.   Run: cargo run --release --example algorithmic_optimization_toolbox
//! COMMON USES: query engines (sort+merge instead of nested loops, prefix sums for range aggregates), monitoring
//!   (sliding-window max/min), scheduling and partitioning DPs, path finding (A*), randomised selection in analytics,
//!   cache-friendly matrix code, and contest/interview problems.
//! Course: MIT 6.006 / 6.046 (amortised analysis, divide and conquer, DP), CMU 15-451 for randomised algorithms.
#![allow(dead_code)]
//!
//! WHAT: 12 sections of pairs (naive, optimised). For each pair: (1) both are run on seeded random inputs and must return the
//! same answer; (2) both increment an explicit OPERATION COUNTER (comparisons, cell updates, pointer moves, cache
//! misses of a simulated cache); (3) the asymptotic gap is ASSERTED on the counters, never on wall-clock time, so the
//! asserts hold on any machine. The counts are printed so you can see O(n^2) vs O(n log n) as actual numbers.
//! WHY: micro-optimisation (SIMD, unrolling; see `loop_optimization_ladder`) multiplies by a constant; the ideas here
//! change the exponent. The order of attack is: better algorithm, better data layout, then constant factors.
//! HOW: each section is one idea in ~20 lines. Ideas only described (not coded) are marked DESCRIBED:
//!   - SMAWK: row minima of a totally monotone matrix in O(n + m); generalises divide-and-conquer DP optimisation.
//!   - van Emde Boas layout: store a search tree recursively by halving height so any block size B is served at
//!     O(log_B n) misses without knowing B (cache-oblivious); section 12 shows the same trick on transposition.
//!   - HyperLogLog: approximate distinct counting in O(log log n) space; see the probabilistic data structure recipes.
//!
//! Mo's algorithm here keeps one counter for every value up to the largest one, so values must be small: a huge value
//! can overflow or exhaust memory. Compress the values first for real data.
//! LIMITS: these are teaching versions. Sums and squares must fit in their integer types (nothing checks for overflow, also not
//! `d * d` in `prime_6k` for n close to `u64::MAX`). The group-split and Knuth optimisations are only valid for non-negative
//! numbers, and the convex hull trick expects the heights in increasing order. The pair count of the two-pointer version
//! leaves out the sort. A random pivot does NOT save this quickselect from many equal values: its two-way partition then
//! takes a quadratic number of comparisons (a three-way partition avoids that).
//!
//! Read: https://en.wikipedia.org/wiki/Amortized_analysis and https://en.wikipedia.org/wiki/Cache-oblivious_algorithm
//!   and https://cp-algorithms.com/ (sections: sqrt decomposition, Mo's algorithm, convex hull trick, divide and conquer DP,
//!   Knuth optimisation; page names checked only at the site root, so search there).
//! TRY NEXT:
//!   - [S] In `sliding_max_deque`, change the pop condition from `<=` to `<`; which assert breaks and why does the answer stay correct on distinct values but the op bound move on equal values?
//!   - [S] Predict how `mo_distinct` moves scale if you shuffle the query order instead of sorting by (block, right); then remove the sort and print the moves.
//!   - [M] Add a Fenwick tree version of the sqrt-decomposition range-sum and compare the counted ops per query for n = 1e3, 1e5.
//!   - [M] Port `astar_grid` to weighted cells (cost 1..9) with the Manhattan heuristic scaled by the minimum cost; find a grid where an inconsistent heuristic makes expansions exceed BFS-like Dijkstra.
//!   - [A] PROJECT: combine with `kway_merge_family`: speed up `knuth_stones` style DPs by merging precomputed sorted runs, or use `k_smallest_pair_sums` to prune the meet-in-the-middle enumeration.
//!   - [A] PROJECT: combine with `cache_locality` (perf chapter): replace the simulated LRU cache in section 12 by measured wall-clock on real arrays at n = 4096 and check the ranking agrees.
use std::cmp::Reverse;
use std::collections::{BinaryHeap, HashMap, VecDeque};

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

fn vec_below(r: &mut Rng, len: usize, m: u64) -> Vec<i64> {
    (0..len).map(|_| r.below(m) as i64).collect()
}

const ROUNDS: usize = 300;

fn report(name: &str, naive: u64, opt: u64) {
    println!(
        "  {name:<34} naive ops {naive:>12}   optimised ops {opt:>10}   gap x{:.1}",
        naive as f64 / opt.max(1) as f64
    );
}

// 1. O(n^2) pair counting -> sort + two pointers (or hashing) -------------------------------------------------
fn pairs_naive(a: &[i64], t: i64, ops: &mut u64) -> u64 {
    let mut c = 0;
    for i in 0..a.len() {
        for j in i + 1..a.len() {
            *ops += 1;
            c += (a[i] + a[j] == t) as u64;
        }
    }
    c
}
fn pairs_two_pointers(a: &[i64], t: i64, ops: &mut u64) -> u64 {
    let mut s = a.to_vec();
    s.sort(); // O(n log n); the scan below is the part whose operations we count
    let (mut l, mut e, mut c) = (0usize, s.len(), 0u64); // right candidate is s[e - 1]
    while l + 1 < e {
        *ops += 1;
        let sum = s[l] + s[e - 1];
        if sum < t {
            l += 1;
        } else if sum > t {
            e -= 1;
        } else if s[l] == s[e - 1] {
            let m = (e - l) as u64;
            c += m * (m - 1) / 2; // every pair inside the equal block
            break;
        } else {
            let (lv, rv) = (s[l], s[e - 1]);
            let (mut lc, mut rc) = (0u64, 0u64);
            while s[l] == lv {
                l += 1;
                lc += 1;
                *ops += 1;
            }
            while s[e - 1] == rv {
                e -= 1;
                rc += 1;
                *ops += 1;
            }
            c += lc * rc;
        }
    }
    c
}
fn pairs_hash(a: &[i64], t: i64, ops: &mut u64) -> u64 {
    let mut seen: HashMap<i64, u64> = HashMap::new();
    let mut c = 0;
    for &x in a {
        *ops += 1;
        c += seen.get(&(t - x)).copied().unwrap_or(0);
        *seen.entry(x).or_default() += 1;
    }
    c
}

// 2. repeated range add -> difference array ---------------------------------------------------------------------
fn range_add_naive(n: usize, updates: &[(usize, usize, i64)], ops: &mut u64) -> Vec<i64> {
    let mut a = vec![0; n];
    for &(l, r, v) in updates {
        for x in &mut a[l..=r] {
            *x += v;
            *ops += 1;
        }
    }
    a
}
fn range_add_diff(n: usize, updates: &[(usize, usize, i64)], ops: &mut u64) -> Vec<i64> {
    let mut d = vec![0i64; n + 1];
    for &(l, r, v) in updates {
        d[l] += v;
        d[r + 1] -= v;
        *ops += 2;
    }
    let mut acc = 0;
    (0..n)
        .map(|i| {
            acc += d[i];
            *ops += 1;
            acc
        })
        .collect()
}

// 3. sliding window maximum -> monotonic deque -------------------------------------------------------------------
fn sliding_max_naive(a: &[i64], k: usize, ops: &mut u64) -> Vec<i64> {
    a.windows(k)
        .map(|w| {
            *ops += k as u64;
            *w.iter().max().unwrap()
        })
        .collect()
}
fn sliding_max_deque(a: &[i64], k: usize, ops: &mut u64) -> Vec<i64> {
    let mut dq: VecDeque<usize> = VecDeque::new(); // indices, values strictly decreasing from front to back
    let mut out = Vec::new();
    for i in 0..a.len() {
        while dq.back().is_some_and(|&j| a[j] <= a[i]) {
            dq.pop_back(); // j can never be a window maximum again: i is newer and at least as large
            *ops += 1;
        }
        dq.push_back(i);
        *ops += 1;
        if dq[0] + k <= i {
            dq.pop_front();
            *ops += 1;
        }
        if i + 1 >= k {
            out.push(a[dq[0]]);
        }
    }
    out
}

// 4. meet in the middle: count subsets with a given sum -----------------------------------------------------------
fn subsets_naive(a: &[i64], t: i64, ops: &mut u64) -> u64 {
    (0u32..1 << a.len())
        .filter(|&m| {
            *ops += 1;
            (0..a.len())
                .filter(|&i| m >> i & 1 == 1)
                .map(|i| a[i])
                .sum::<i64>()
                == t
        })
        .count() as u64
}
fn all_sums(a: &[i64], ops: &mut u64) -> Vec<i64> {
    let mut s = vec![0];
    for &x in a {
        let n = s.len();
        for i in 0..n {
            s.push(s[i] + x);
            *ops += 1;
        }
    }
    s
}
fn subsets_mitm(a: &[i64], t: i64, ops: &mut u64) -> u64 {
    let (l, r) = a.split_at(a.len() / 2);
    let (sl, sr) = (all_sums(l, ops), all_sums(r, ops));
    let mut m: HashMap<i64, u64> = HashMap::new();
    for &x in &sr {
        *m.entry(x).or_default() += 1;
    }
    sl.iter()
        .map(|&x| {
            *ops += 1;
            m.get(&(t - x)).copied().unwrap_or(0)
        })
        .sum()
}

// 5a. sqrt decomposition: point update, range sum --------------------------------------------------------------
struct Sqrt {
    a: Vec<i64>,
    blk: Vec<i64>,
    b: usize,
}
impl Sqrt {
    fn new(a: Vec<i64>) -> Self {
        let b = ((a.len() as f64).sqrt() as usize).max(1);
        let mut blk = vec![0; a.len().div_ceil(b)];
        for (i, &x) in a.iter().enumerate() {
            blk[i / b] += x;
        }
        Sqrt { a, blk, b }
    }
    fn update(&mut self, i: usize, v: i64) {
        self.blk[i / self.b] += v - self.a[i];
        self.a[i] = v;
    }
    /// sum of a[l..r): partial blocks element by element, full blocks via the summary: <= 2b + n/b ops.
    fn query(&self, l: usize, r: usize, ops: &mut u64) -> i64 {
        let mut s = 0;
        let mut i = l;
        while i < r {
            if i % self.b == 0 && i + self.b <= r {
                s += self.blk[i / self.b];
                i += self.b;
            } else {
                s += self.a[i];
                i += 1;
            }
            *ops += 1;
        }
        s
    }
}

// 5b. Mo's algorithm: offline range-distinct-count --------------------------------------------------------------
fn distinct_naive(a: &[usize], qs: &[(usize, usize)], ops: &mut u64) -> Vec<usize> {
    qs.iter()
        .map(|&(l, r)| {
            let mut s = std::collections::HashSet::new();
            for &x in &a[l..=r] {
                s.insert(x);
                *ops += 1;
            }
            s.len()
        })
        .collect()
}
fn mo_distinct(a: &[usize], qs: &[(usize, usize)], moves: &mut u64) -> (Vec<usize>, usize) {
    let q = qs.len().max(1);
    let b = ((a.len() as f64 / (q as f64).sqrt()) as usize).max(1);
    let mut order: Vec<usize> = (0..qs.len()).collect();
    order.sort_by_key(|&i| (qs[i].0 / b, qs[i].1));
    // indexed by VALUE, so sized by the largest value, not by the length of `a`
    let mut cnt = vec![0usize; a.iter().max().map_or(0, |&m| m + 1)];
    let (mut cl, mut cr, mut distinct) = (0usize, 0usize, 0usize); // window is [cl, cr)
    let mut ans = vec![0; qs.len()];
    let add = |x: usize, cnt: &mut Vec<usize>, d: &mut usize| {
        cnt[x] += 1;
        *d += (cnt[x] == 1) as usize;
    };
    for i in order {
        let (l, r) = (qs[i].0, qs[i].1 + 1);
        while cl > l {
            cl -= 1;
            add(a[cl], &mut cnt, &mut distinct);
            *moves += 1;
        }
        while cr < r {
            add(a[cr], &mut cnt, &mut distinct);
            cr += 1;
            *moves += 1;
        }
        while cl < l {
            cnt[a[cl]] -= 1;
            distinct -= (cnt[a[cl]] == 0) as usize;
            cl += 1;
            *moves += 1;
        }
        while cr > r {
            cr -= 1;
            cnt[a[cr]] -= 1;
            distinct -= (cnt[a[cr]] == 0) as usize;
            *moves += 1;
        }
        ans[i] = distinct;
    }
    (ans, b)
}

// 6a. divide-and-conquer DP optimisation: split into g groups minimising the sum of squared group sums --------
fn partition_naive(a: &[i64], g: usize, ops: &mut u64) -> i64 {
    let n = a.len();
    let pre: Vec<i64> = std::iter::once(0)
        .chain(a.iter().scan(0, |s, &x| {
            *s += x;
            Some(*s)
        }))
        .collect();
    let cost = |l: usize, r: usize| (pre[r] - pre[l]).pow(2);
    let mut prev: Vec<i64> = (0..=n)
        .map(|i| if i == 0 { 0 } else { cost(0, i) })
        .collect();
    for _ in 2..=g {
        let mut cur = vec![i64::MAX; n + 1];
        cur[0] = 0;
        for i in 1..=n {
            for j in 0..i {
                *ops += 1;
                cur[i] = cur[i].min(prev[j] + cost(j, i));
            }
        }
        prev = cur;
    }
    prev[n]
}
fn partition_dc(a: &[i64], g: usize, ops: &mut u64) -> i64 {
    let n = a.len();
    let pre: Vec<i64> = std::iter::once(0)
        .chain(a.iter().scan(0, |s, &x| {
            *s += x;
            Some(*s)
        }))
        .collect();
    let mut prev: Vec<i64> = (0..=n).map(|i| (pre[i]).pow(2)).collect();
    // opt(i) = best split point j for cur[i]; for convex (Monge) costs opt is monotone in i, so recurse on halves.
    #[allow(clippy::too_many_arguments)]
    fn solve(
        lo: usize,
        hi: usize,
        olo: usize,
        ohi: usize,
        prev: &[i64],
        cur: &mut [i64],
        pre: &[i64],
        ops: &mut u64,
    ) {
        if lo > hi {
            return;
        }
        let mid = (lo + hi) / 2;
        let (mut best, mut arg) = (i64::MAX, olo);
        for j in olo..=ohi.min(mid - 1) {
            *ops += 1;
            let v = prev[j] + (pre[mid] - pre[j]).pow(2);
            if v < best {
                best = v;
                arg = j;
            }
        }
        cur[mid] = best;
        if mid > lo {
            solve(lo, mid - 1, olo, arg, prev, cur, pre, ops);
        }
        solve(mid + 1, hi, arg, ohi, prev, cur, pre, ops);
    }
    for _ in 2..=g {
        let mut cur = vec![i64::MAX; n + 1];
        cur[0] = 0;
        if n >= 1 {
            solve(1, n, 0, n - 1, &prev, &mut cur, &pre, ops);
        }
        prev = cur;
    }
    prev[n]
}

// 6b. Knuth optimisation: merge adjacent piles at cost = total weight ---------------------------------------------
fn stones_cubic(w: &[i64], ops: &mut u64) -> i64 {
    let n = w.len();
    if n == 0 {
        return 0; // no piles, nothing to merge
    }
    let pre: Vec<i64> = std::iter::once(0)
        .chain(w.iter().scan(0, |s, &x| {
            *s += x;
            Some(*s)
        }))
        .collect();
    let mut dp = vec![vec![0i64; n]; n];
    for len in 2..=n {
        for i in 0..=n - len {
            let j = i + len - 1;
            dp[i][j] = (i..j)
                .map(|k| {
                    *ops += 1;
                    dp[i][k] + dp[k + 1][j]
                })
                .min()
                .unwrap()
                + pre[j + 1]
                - pre[i];
        }
    }
    dp[0][n - 1]
}
fn stones_knuth(w: &[i64], ops: &mut u64) -> i64 {
    let n = w.len();
    if n == 0 {
        return 0; // no piles, nothing to merge
    }
    let pre: Vec<i64> = std::iter::once(0)
        .chain(w.iter().scan(0, |s, &x| {
            *s += x;
            Some(*s)
        }))
        .collect();
    let mut dp = vec![vec![0i64; n]; n];
    let mut opt = vec![vec![0usize; n]; n];
    for i in 0..n {
        opt[i][i] = i;
    }
    for len in 2..=n {
        for i in 0..=n - len {
            let j = i + len - 1;
            dp[i][j] = i64::MAX;
            // opt[i][j-1] <= opt[i][j] <= opt[i+1][j]: the telescoping sum of these ranges is O(n) per diagonal
            for k in opt[i][j - 1]..=opt[i + 1][j].min(j - 1) {
                *ops += 1;
                let v = dp[i][k] + dp[k + 1][j];
                if v < dp[i][j] {
                    dp[i][j] = v;
                    opt[i][j] = k;
                }
            }
            dp[i][j] += pre[j + 1] - pre[i];
        }
    }
    dp[0][n - 1]
}

// 6c. convex hull trick: dp[i] = min_j dp[j] + (h[i]-h[j])^2 + c, h increasing -----------------------------------
fn frog_naive(h: &[i64], c: i64, ops: &mut u64) -> i64 {
    let mut dp = vec![0i64; h.len()];
    for i in 1..h.len() {
        dp[i] = (0..i)
            .map(|j| {
                *ops += 1;
                dp[j] + (h[i] - h[j]).pow(2) + c
            })
            .min()
            .unwrap();
    }
    dp.last().copied().unwrap_or(0)
}
fn frog_cht(h: &[i64], c: i64, ops: &mut u64) -> i64 {
    // line_j(x) = m*x + b with m = -2h[j], b = dp[j] + h[j]^2; query x = h[i] increasing, slopes decreasing.
    let mut dp = vec![0i64; h.len()];
    let mut dq: VecDeque<(i128, i128)> = VecDeque::new();
    let Some(&first) = h.first() else {
        return 0;
    };
    dq.push_back((-2 * first as i128, (first as i128).pow(2)));
    for i in 1..h.len() {
        let x = h[i] as i128;
        while dq.len() >= 2 && dq[0].0 * x + dq[0].1 >= dq[1].0 * x + dq[1].1 {
            dq.pop_front();
            *ops += 1;
        }
        *ops += 1;
        dp[i] = (dq[0].0 * x + dq[0].1 + x * x + c as i128) as i64;
        let nl = (-2 * h[i] as i128, dp[i] as i128 + (h[i] as i128).pow(2));
        // pop the back line if it is never strictly best once nl exists (intersection comparison, cross-multiplied)
        while dq.len() >= 2 {
            let (l1, l2) = (dq[dq.len() - 2], dq[dq.len() - 1]);
            if (nl.1 - l1.1) * (l1.0 - l2.0) <= (l2.1 - l1.1) * (l1.0 - nl.0) {
                dq.pop_back();
                *ops += 1;
            } else {
                break;
            }
        }
        dq.push_back(nl);
    }
    *dp.last().unwrap()
}

// 7. bitset acceleration: subset-sum reachability ---------------------------------------------------------------
fn subset_sum_bool(a: &[usize], cap: usize, ops: &mut u64) -> Vec<bool> {
    let mut dp = vec![false; cap + 1];
    dp[0] = true;
    for &w in a {
        for s in (w..=cap).rev() {
            *ops += 1;
            dp[s] |= dp[s - w];
        }
    }
    dp
}
fn shift_or(bits: &mut [u64], w: usize, ops: &mut u64) {
    let (ws, bs) = (w / 64, w % 64);
    for i in (ws..bits.len()).rev() {
        let mut v = bits[i - ws] << bs;
        if bs > 0 && i > ws {
            v |= bits[i - ws - 1] >> (64 - bs);
        }
        bits[i] |= v;
        *ops += 1; // one 64-bit word = 64 table cells
    }
}
fn subset_sum_bitset(a: &[usize], cap: usize, ops: &mut u64) -> Vec<bool> {
    let mut bits = vec![0u64; cap / 64 + 1];
    bits[0] = 1;
    for &w in a {
        shift_or(&mut bits, w, ops);
    }
    (0..=cap)
        .map(|s| bits[s / 64] >> (s % 64) & 1 == 1)
        .collect()
}

// 8. memoisation vs tabulation vs state compression: 0/1 knapsack value -------------------------------------------
fn knap_rec(i: usize, cap: usize, w: &[usize], v: &[i64], calls: &mut u64) -> i64 {
    *calls += 1;
    if i == w.len() {
        return 0;
    }
    let skip = knap_rec(i + 1, cap, w, v, calls);
    if w[i] <= cap {
        skip.max(v[i] + knap_rec(i + 1, cap - w[i], w, v, calls))
    } else {
        skip
    }
}
fn knap_memo(
    i: usize,
    cap: usize,
    w: &[usize],
    v: &[i64],
    memo: &mut HashMap<(usize, usize), i64>,
    calls: &mut u64,
) -> i64 {
    *calls += 1;
    if i == w.len() {
        return 0;
    }
    if let Some(&x) = memo.get(&(i, cap)) {
        return x;
    }
    let skip = knap_memo(i + 1, cap, w, v, memo, calls);
    let r = if w[i] <= cap {
        skip.max(v[i] + knap_memo(i + 1, cap - w[i], w, v, memo, calls))
    } else {
        skip
    };
    memo.insert((i, cap), r);
    r
}
/// Tabulation with the state compressed from n x (W+1) to one row: iterate capacity downwards so row i-1 is intact.
fn knap_1d(w: &[usize], v: &[i64], cap: usize) -> i64 {
    let mut dp = vec![0i64; cap + 1];
    for i in 0..w.len() {
        for c in (w[i]..=cap).rev() {
            dp[c] = dp[c].max(dp[c - w[i]] + v[i]);
        }
    }
    dp[cap]
}

// 9. amortised analysis: two-stack queue, doubling array, union-find -------------------------------------------
struct TwoStackQueue {
    inbox: Vec<u32>,
    outbox: Vec<u32>,
    moves: u64,
}
impl TwoStackQueue {
    fn push(&mut self, x: u32) {
        self.inbox.push(x);
    }
    fn pop(&mut self) -> Option<u32> {
        if self.outbox.is_empty() {
            while let Some(x) = self.inbox.pop() {
                self.outbox.push(x); // each element is moved at most once: total moves <= pushes
                self.moves += 1;
            }
        }
        self.outbox.pop()
    }
}
struct Dsu {
    p: Vec<usize>,
    sz: Vec<usize>,
    hops: u64,
    compress: bool,
    by_size: bool,
}
impl Dsu {
    fn new(n: usize, compress: bool, by_size: bool) -> Self {
        Dsu {
            p: (0..n).collect(),
            sz: vec![1; n],
            hops: 0,
            compress,
            by_size,
        }
    }
    fn find(&mut self, mut x: usize) -> usize {
        let mut root = x;
        while self.p[root] != root {
            root = self.p[root];
            self.hops += 1;
        }
        if self.compress {
            while self.p[x] != root {
                let nx = self.p[x];
                self.p[x] = root;
                x = nx;
            }
        }
        root
    }
    fn union(&mut self, a: usize, b: usize) {
        let (mut a, mut b) = (self.find(a), self.find(b));
        if a == b {
            return;
        }
        if self.by_size && self.sz[a] < self.sz[b] {
            std::mem::swap(&mut a, &mut b);
        }
        self.p[b] = a;
        self.sz[a] += self.sz[b];
    }
}

// 10. randomised: quickselect with random vs first-element pivot; reservoir sampling ----------------------------
fn quickselect(a: &mut [i64], k: usize, random: bool, r: &mut Rng, cmps: &mut u64) -> i64 {
    let (mut lo, mut hi) = (0usize, a.len() - 1);
    loop {
        if lo == hi {
            return a[lo];
        }
        let p = if random {
            lo + r.below((hi - lo + 1) as u64) as usize
        } else {
            lo
        };
        a.swap(p, hi);
        let pivot = a[hi];
        let mut store = lo;
        for i in lo..hi {
            *cmps += 1;
            if a[i] < pivot {
                a.swap(i, store);
                store += 1;
            }
        }
        a.swap(store, hi);
        match k.cmp(&store) {
            std::cmp::Ordering::Equal => return a[store],
            std::cmp::Ordering::Less => hi = store - 1,
            std::cmp::Ordering::Greater => lo = store + 1,
        }
        if lo > hi {
            return a[k];
        }
    }
}
fn reservoir(n: usize, k: usize, r: &mut Rng) -> Vec<usize> {
    let mut res: Vec<usize> = (0..k.min(n)).collect(); // a sample never holds more than there is
    for i in k..n {
        let j = r.below(i as u64 + 1) as usize;
        if j < k {
            res[j] = i;
        }
    }
    res
}

// 11. pruning and A* ------------------------------------------------------------------------------------------
fn bfs_grid(g: &[Vec<bool>], s: (usize, usize), t: (usize, usize)) -> (Option<u32>, u64) {
    let (h, w) = (g.len(), g[0].len());
    let mut dist = vec![vec![u32::MAX; w]; h];
    dist[s.0][s.1] = 0;
    let mut q = VecDeque::from([s]);
    let mut exp = 0;
    while let Some((y, x)) = q.pop_front() {
        exp += 1;
        if (y, x) == t {
            return (Some(dist[y][x]), exp);
        }
        for (ny, nx) in nbrs(y, x, h, w) {
            if !g[ny][nx] && dist[ny][nx] == u32::MAX {
                dist[ny][nx] = dist[y][x] + 1;
                q.push_back((ny, nx));
            }
        }
    }
    (None, exp)
}
fn nbrs(y: usize, x: usize, h: usize, w: usize) -> impl Iterator<Item = (usize, usize)> {
    [
        (y.wrapping_sub(1), x),
        (y + 1, x),
        (y, x.wrapping_sub(1)),
        (y, x + 1),
    ]
    .into_iter()
    .filter(move |&(a, b)| a < h && b < w)
}
fn astar_grid(g: &[Vec<bool>], s: (usize, usize), t: (usize, usize)) -> (Option<u32>, u64) {
    let (h, w) = (g.len(), g[0].len());
    let man = |p: (usize, usize)| (p.0.abs_diff(t.0) + p.1.abs_diff(t.1)) as u32; // consistent for unit-cost 4-moves
    let mut best = vec![vec![u32::MAX; w]; h];
    let mut closed = vec![vec![false; w]; h];
    best[s.0][s.1] = 0;
    let mut pq = BinaryHeap::new();
    pq.push(Reverse((man(s), 0u32, s)));
    let mut exp = 0;
    while let Some(Reverse((_, gc, (y, x)))) = pq.pop() {
        if closed[y][x] {
            continue;
        }
        closed[y][x] = true;
        exp += 1;
        if (y, x) == t {
            return (Some(gc), exp);
        }
        for (ny, nx) in nbrs(y, x, h, w) {
            if !g[ny][nx] && gc + 1 < best[ny][nx] {
                best[ny][nx] = gc + 1;
                pq.push(Reverse((gc + 1 + man((ny, nx)), gc + 1, (ny, nx))));
            }
        }
    }
    (None, exp)
}
fn prime_naive(n: u64, ops: &mut u64) -> bool {
    n >= 2
        && (2..n).all(|d| {
            *ops += 1;
            n % d != 0
        })
}
fn prime_6k(n: u64, ops: &mut u64) -> bool {
    if n < 4 {
        return n >= 2;
    }
    if n % 2 == 0 || n % 3 == 0 {
        return false;
    }
    let mut d = 5;
    while d * d <= n {
        *ops += 2;
        if n % d == 0 || n % (d + 2) == 0 {
            return false;
        } // early termination + sqrt bound + skip multiples of 2, 3
        d += 6;
    }
    true
}

// 12. cache-oblivious transpose vs naive, measured on a simulated fully associative LRU cache -------------------------
struct LruCache {
    lines: VecDeque<usize>,
    cap: usize,
    line_elems: usize,
    misses: u64,
}
impl LruCache {
    fn new(cap_lines: usize, line_elems: usize) -> Self {
        LruCache {
            lines: VecDeque::new(),
            cap: cap_lines,
            line_elems,
            misses: 0,
        }
    }
    fn touch(&mut self, addr: usize) {
        let l = addr / self.line_elems;
        if let Some(p) = self.lines.iter().position(|&x| x == l) {
            self.lines.remove(p);
        } else {
            self.misses += 1;
            if self.lines.len() == self.cap {
                self.lines.pop_back();
            }
        }
        self.lines.push_front(l);
    }
}
fn transpose_naive(src: &[i64], dst: &mut [i64], n: usize, c: &mut LruCache) {
    for i in 0..n {
        for j in 0..n {
            c.touch(i * n + j);
            c.touch(n * n + j * n + i);
            dst[j * n + i] = src[i * n + j];
        }
    }
}
/// Recursively split the larger dimension until the tile is tiny. At SOME depth a tile fits in cache whatever the
/// cache size is, and no code mentions the cache size: that is what "cache-oblivious" means.
fn transpose_rec(
    src: &[i64],
    dst: &mut [i64],
    n: usize,
    (r0, r1, c0, c1): (usize, usize, usize, usize),
    c: &mut LruCache,
) {
    let (h, w) = (r1 - r0, c1 - c0);
    if h <= 2 && w <= 2 {
        for i in r0..r1 {
            for j in c0..c1 {
                c.touch(i * n + j);
                c.touch(n * n + j * n + i);
                dst[j * n + i] = src[i * n + j];
            }
        }
    } else if h >= w {
        let m = r0 + h / 2;
        transpose_rec(src, dst, n, (r0, m, c0, c1), c);
        transpose_rec(src, dst, n, (m, r1, c0, c1), c);
    } else {
        let m = c0 + w / 2;
        transpose_rec(src, dst, n, (r0, r1, c0, m), c);
        transpose_rec(src, dst, n, (r0, r1, m, c1), c);
    }
}

fn main() {
    let mut r = Rng(0x888_FEED_BEEF);
    println!("algorithmic optimisation toolbox (op counts, never timings, are asserted)");

    println!("1. pair counting");
    let (mut cn, mut co, mut ch) = (0, 0, 0);
    for _ in 0..ROUNDS {
        let len = 5 + r.below(60) as usize;
        let a = vec_below(&mut r, len, 12);
        let t = r.below(24) as i64;
        let (x, y, z) = (
            pairs_naive(&a, t, &mut cn),
            pairs_two_pointers(&a, t, &mut co),
            pairs_hash(&a, t, &mut ch),
        );
        assert_eq!((x, y), (x, z), "hash vs naive");
        assert_eq!(x, y, "two pointers vs naive on {a:?} t={t}");
        assert!(ch as usize <= 65 * ROUNDS);
    }
    let big = vec_below(&mut r, 3000, 1_000_000);
    let (mut on, mut ot) = (0, 0);
    assert_eq!(
        pairs_naive(&big, 777_777, &mut on),
        pairs_two_pointers(&big, 777_777, &mut ot)
    );
    assert!(ot <= 2 * big.len() as u64 + 8 && on == 3000 * 2999 / 2);
    report("pairs (n=3000): nested / sorted 2-ptr", on, ot);

    println!("2. range add");
    for _ in 0..ROUNDS {
        let n = 1 + r.below(50) as usize;
        let ups: Vec<(usize, usize, i64)> = (0..r.below(30))
            .map(|_| {
                let l = r.below(n as u64) as usize;
                (
                    l,
                    l + r.below((n - l) as u64) as usize,
                    r.below(9) as i64 - 4,
                )
            })
            .collect();
        let (mut a, mut b) = (0, 0);
        assert_eq!(
            range_add_naive(n, &ups, &mut a),
            range_add_diff(n, &ups, &mut b)
        );
        assert_eq!(b, 2 * ups.len() as u64 + n as u64);
    }
    let ups: Vec<(usize, usize, i64)> = (0..2000)
        .map(|_| (r.below(1000) as usize, 1000 + r.below(9000) as usize, 1))
        .collect();
    let (mut a, mut b) = (0, 0);
    assert_eq!(
        range_add_naive(10_000, &ups, &mut a),
        range_add_diff(10_000, &ups, &mut b)
    );
    assert!(a > 50 * b);
    report("2000 range adds on n=10000", a, b);

    println!("3. sliding window max");
    for _ in 0..ROUNDS {
        let n = 1 + r.below(60) as usize;
        let a = vec_below(&mut r, n, 10);
        let k = 1 + r.below(n as u64) as usize;
        let (mut x, mut y) = (0, 0);
        assert_eq!(
            sliding_max_naive(&a, k, &mut x),
            sliding_max_deque(&a, k, &mut y)
        );
        assert!(
            y <= 3 * n as u64,
            "each index is pushed once, popped at most once: <= 2n (+1 front pops)"
        );
    }
    let a = vec_below(&mut r, 20_000, 1 << 30);
    let (mut x, mut y) = (0, 0);
    assert_eq!(
        sliding_max_naive(&a, 500, &mut x),
        sliding_max_deque(&a, 500, &mut y)
    );
    assert!(y <= 3 * 20_000);
    report("window max n=20000 k=500", x, y);

    println!("4. meet in the middle (subset sums)");
    for _ in 0..ROUNDS / 2 {
        let len = 2 + r.below(11) as usize;
        let a = vec_below(&mut r, len, 15);
        let t = r.below(40) as i64;
        let (mut x, mut y) = (0, 0);
        assert_eq!(subsets_naive(&a, t, &mut x), subsets_mitm(&a, t, &mut y));
    }
    let a = vec_below(&mut r, 20, 1000);
    let (mut x, mut y) = (0, 0);
    assert_eq!(
        subsets_naive(&a, 2500, &mut x),
        subsets_mitm(&a, 2500, &mut y)
    );
    assert!(y * 100 < x);
    report("n=20 subsets: 2^n vs 2*2^(n/2)", x, y);

    println!("5. sqrt decomposition and Mo's algorithm");
    let n = 10_000;
    let base = vec_below(&mut r, n, 100);
    let mut sq = Sqrt::new(base.clone());
    let mut cur = base;
    let (mut qn, mut qs, mut worst) = (0u64, 0u64, 0u64);
    for _ in 0..2000 {
        if r.below(2) == 0 {
            let (i, v) = (r.below(n as u64) as usize, r.below(100) as i64);
            sq.update(i, v);
            cur[i] = v;
        } else {
            let (l, rr) = (r.below(n as u64) as usize, r.below(n as u64) as usize);
            let (l, rr) = (l.min(rr), l.max(rr) + 1);
            let mut o = 0;
            assert_eq!(sq.query(l, rr, &mut o), cur[l..rr].iter().sum::<i64>());
            qs += o;
            worst = worst.max(o);
            qn += (rr - l) as u64;
        }
    }
    assert!(
        worst <= 3 * 100 + 2,
        "per query <= 2*sqrt(n) partial + sqrt(n) blocks"
    );
    report("range sums n=10000 (sqrt blocks)", qn, qs);
    for _ in 0..ROUNDS / 3 {
        let n = 50 + r.below(250) as usize;
        let a: Vec<usize> = (0..n).map(|_| r.below(15) as usize).collect();
        let qs_: Vec<(usize, usize)> = (0..50 + r.below(250))
            .map(|_| {
                let x = r.below(n as u64) as usize;
                let y = r.below(n as u64) as usize;
                (x.min(y), x.max(y))
            })
            .collect();
        let (mut o1, mut mv) = (0, 0);
        let want = distinct_naive(&a, &qs_, &mut o1);
        let (got, b) = mo_distinct(&a, &qs_, &mut mv);
        assert_eq!(got, want);
        let q = qs_.len() as u64;
        assert!(
            mv <= 2 * q * b as u64 + (n as u64) * (n as u64 / b as u64 + 1) + n as u64,
            "Mo bound: q*B + n*(n/B)"
        );
    }
    let a: Vec<usize> = (0..20_000).map(|_| r.below(500) as usize).collect();
    let qq: Vec<(usize, usize)> = (0..20_000)
        .map(|_| {
            let x = r.below(20_000) as usize;
            let y = r.below(20_000) as usize;
            (x.min(y), x.max(y))
        })
        .collect();
    let (mut o1, mut mv) = (0, 0);
    assert_eq!(
        distinct_naive(&a, &qq, &mut o1),
        mo_distinct(&a, &qq, &mut mv).0
    );
    assert!(mv * 5 < o1);
    report("Mo distinct n=q=20000", o1, mv);

    // values larger than the array is long
    let (a, qs_) = ([100, 7, 100], [(0, 2), (1, 1)]);
    assert_eq!(
        mo_distinct(&a, &qs_, &mut 0).0,
        distinct_naive(&a, &qs_, &mut 0)
    );
    assert!(mo_distinct(&[], &[], &mut 0).0.is_empty());

    println!("6. DP optimisations");
    assert_eq!(
        (stones_cubic(&[], &mut 0), stones_knuth(&[], &mut 0)),
        (0, 0)
    );
    assert_eq!(
        (frog_naive(&[], 1, &mut 0), frog_cht(&[], 1, &mut 0)),
        (0, 0)
    );
    for _ in 0..ROUNDS / 2 {
        let n = 2 + r.below(14) as usize;
        let a = vec_below(&mut r, n, 20);
        let g = 1 + r.below(n as u64 / 2 + 1) as usize;
        let (mut x, mut y) = (0, 0);
        assert_eq!(
            partition_naive(&a, g, &mut x),
            partition_dc(&a, g, &mut y),
            "{a:?} g={g}"
        );
        let (mut s1, mut s2) = (0, 0);
        assert_eq!(stones_cubic(&a, &mut s1), stones_knuth(&a, &mut s2));
        let mut hs = vec_below(&mut r, n, 30);
        hs.sort();
        hs.dedup();
        if hs.len() > 1 {
            let (mut f1, mut f2) = (0, 0);
            let c = r.below(50) as i64;
            assert_eq!(frog_naive(&hs, c, &mut f1), frog_cht(&hs, c, &mut f2));
        }
    }
    let a = vec_below(&mut r, 600, 50);
    let (mut x, mut y) = (0, 0);
    assert_eq!(partition_naive(&a, 4, &mut x), partition_dc(&a, 4, &mut y));
    assert!(y * 10 < x);
    report("D&C opt: n=600, 4 groups", x, y);
    let a = vec_below(&mut r, 300, 100);
    let (mut x, mut y) = (0, 0);
    assert_eq!(stones_cubic(&a, &mut x), stones_knuth(&a, &mut y));
    assert!(y <= 3 * 300 * 300 && y * 10 < x);
    report("Knuth: n=300 stones (n^3 vs n^2)", x, y);
    let mut hs: Vec<i64> = (0..3000).map(|i| i * 3 + r.below(3) as i64).collect();
    hs.dedup();
    let (mut x, mut y) = (0, 0);
    assert_eq!(frog_naive(&hs, 100, &mut x), frog_cht(&hs, 100, &mut y));
    assert!(y <= 3 * hs.len() as u64 && y * 100 < x);
    report("convex hull trick n=3000", x, y);

    println!("7. bitset subset-sum");
    for _ in 0..ROUNDS {
        let a: Vec<usize> = (0..1 + r.below(12))
            .map(|_| 1 + r.below(130) as usize)
            .collect();
        let cap = r.below(500) as usize;
        let (mut x, mut y) = (0, 0);
        assert_eq!(
            subset_sum_bool(&a, cap, &mut x),
            subset_sum_bitset(&a, cap, &mut y)
        );
    }
    let a: Vec<usize> = (0..200).map(|_| 1 + r.below(900) as usize).collect();
    let (mut x, mut y) = (0, 0);
    assert_eq!(
        subset_sum_bool(&a, 100_000, &mut x),
        subset_sum_bitset(&a, 100_000, &mut y)
    );
    assert!(y * 30 < x);
    report("subset sum 200 items cap 1e5", x, y);

    println!("8. memo / tabulation / state compression");
    let (mut tot_rec, mut tot_memo) = (0u64, 0u64);
    for _ in 0..ROUNDS {
        let n = 1 + r.below(13) as usize;
        let w: Vec<usize> = (0..n).map(|_| 1 + r.below(8) as usize).collect();
        let v = vec_below(&mut r, n, 50);
        let cap = r.below(30) as usize;
        let (mut c1, mut c2) = (0, 0);
        let a = knap_rec(0, cap, &w, &v, &mut c1);
        let b = knap_memo(0, cap, &w, &v, &mut HashMap::new(), &mut c2);
        assert_eq!((a, b), (a, knap_1d(&w, &v, cap)));
        assert_eq!(a, b);
        assert!(
            c2 <= 2 * (n as u64) * (cap as u64 + 1) + 1,
            "memo visits <= 2 calls per distinct state"
        );
        tot_rec += c1;
        tot_memo += c2;
    }
    assert!(tot_memo < tot_rec);
    report("knapsack recursion vs memo (n<=13)", tot_rec, tot_memo);
    let (w, v): (Vec<usize>, Vec<i64>) = (
        (0..20).map(|_| 1 + r.below(4) as usize).collect(),
        vec_below(&mut r, 20, 50),
    );
    let (mut c1, mut c2) = (0, 0);
    assert_eq!(
        knap_rec(0, 30, &w, &v, &mut c1),
        knap_memo(0, 30, &w, &v, &mut HashMap::new(), &mut c2)
    );
    report("knapsack n=20 cap=30", c1, c2);

    println!("9. amortised analysis");
    for _ in 0..ROUNDS {
        let mut q = TwoStackQueue {
            inbox: vec![],
            outbox: vec![],
            moves: 0,
        };
        let mut model = VecDeque::new();
        let mut pushes = 0u64;
        for i in 0..200 {
            if r.below(3) != 0 {
                q.push(i);
                model.push_back(i);
                pushes += 1;
            } else {
                assert_eq!(q.pop(), model.pop_front());
            }
        }
        assert!(
            q.moves <= pushes,
            "each element is moved inbox->outbox at most once"
        );
    }
    let mut vec_copies = 0u64;
    let (mut cap, mut len) = (1usize, 0usize);
    for _ in 0..100_000 {
        if len == cap {
            vec_copies += len as u64;
            cap *= 2;
        }
        len += 1;
    }
    assert!(
        vec_copies < 2 * len as u64,
        "doubling: total copies < 2n, i.e. O(1) amortised per push"
    );
    let n = 4000;
    let chain = |compress| {
        let mut d = Dsu::new(n, compress, false);
        for i in 0..n - 1 {
            d.p[i] = i + 1; // adversarial: one long path
        }
        for _ in 0..n {
            assert_eq!(d.find(0), n - 1);
        }
        d.hops
    };
    let (slow, fast) = (chain(false), chain(true));
    assert!(fast * 100 < slow);
    report("union-find: n finds on a path", slow, fast);
    for _ in 0..ROUNDS {
        let n = 2 + r.below(80) as usize;
        let mut d = Dsu::new(n, true, true);
        let mut label: Vec<usize> = (0..n).collect();
        let ops = 3 * n;
        for _ in 0..ops {
            let (a, b) = (r.below(n as u64) as usize, r.below(n as u64) as usize);
            d.union(a, b);
            let (la, lb) = (label[a], label[b]);
            label.iter_mut().for_each(|x| {
                if *x == lb {
                    *x = la
                }
            });
        }
        for a in 0..n {
            for b in 0..n {
                assert_eq!(d.find(a) == d.find(b), label[a] == label[b]);
            }
        }
    }

    println!("10. randomised algorithms");
    let n = 2000;
    let (mut tr, mut tf) = (0u64, 0u64);
    let trials = 100;
    for t in 0..trials {
        let mut a = vec_below(&mut r, n, 1_000_000);
        let k = r.below(n as u64) as usize;
        let mut sorted = a.clone();
        sorted.sort();
        let mut c = 0;
        assert_eq!(
            quickselect(&mut a, k, true, &mut r, &mut c),
            sorted[k],
            "trial {t}"
        );
        tr += c;
    }
    let mut sorted_input: Vec<i64> = (0..n as i64).collect();
    let mut c = 0;
    assert_eq!(
        quickselect(&mut sorted_input, n - 1, false, &mut r, &mut c),
        n as i64 - 1
    );
    tf += c;
    let mean = tr as f64 / trials as f64;
    assert!(
        mean < 3.6 * n as f64,
        "expected comparisons of random-pivot quickselect <= 2(1+ln2)n ~ 3.39n; got {mean}"
    );
    assert!(
        tf > (n as u64 * (n as u64 - 1)) / 2 - n as u64,
        "first-element pivot on sorted input is quadratic"
    );
    println!(
        "  quickselect n=2000: random pivot mean {mean:.0} comparisons ({:.2} n); first-pivot on sorted input {tf} ({:.0} n)",
        mean / n as f64,
        tf as f64 / n as f64
    );
    let (nn, kk, trials) = (10usize, 3usize, 30_000u64);
    let mut hits = vec![0u64; nn];
    for _ in 0..trials {
        for i in reservoir(nn, kk, &mut r) {
            hits[i] += 1;
        }
    }
    let expect = trials as f64 * kk as f64 / nn as f64;
    let sigma = (trials as f64 * 0.3 * 0.7).sqrt();
    for &h in &hits {
        assert!(
            (h as f64 - expect).abs() < 5.0 * sigma,
            "reservoir must be uniform: {hits:?}"
        );
    }
    println!(
        "  reservoir sampling k=3 of 10: counts {hits:?} (expect {expect:.0} +- {:.0})",
        5.0 * sigma
    );

    assert_eq!(reservoir(2, 5, &mut r), [0, 1]);
    assert!(reservoir(0, 3, &mut r).is_empty());

    println!("11. pruning: A* and early termination");
    let (mut sb, mut sa, mut solvable) = (0u64, 0u64, 0);
    for _ in 0..ROUNDS {
        let (h, w) = (4 + r.below(20) as usize, 4 + r.below(20) as usize);
        let mut g: Vec<Vec<bool>> = (0..h)
            .map(|_| (0..w).map(|_| r.below(100) < 22).collect())
            .collect();
        g[0][0] = false;
        g[h - 1][w - 1] = false;
        let (db, eb) = bfs_grid(&g, (0, 0), (h - 1, w - 1));
        let (da, ea) = astar_grid(&g, (0, 0), (h - 1, w - 1));
        assert_eq!(db, da, "A* with a consistent heuristic is optimal");
        if db.is_some() {
            assert!(
                ea <= eb,
                "A* never expands more than BFS here: {ea} vs {eb}"
            );
            solvable += 1;
        }
        sb += eb;
        sa += ea;
    }
    assert!(sa < sb, "over all grids the heuristic saves work");
    println!(
        "  grid search ({solvable} solvable of {ROUNDS}): BFS expansions {sb}, A* expansions {sa}"
    );
    let (mut a, mut b) = (0, 0);
    for n in 0..3000u64 {
        assert_eq!(prime_naive(n, &mut a), prime_6k(n, &mut b), "n={n}");
    }
    assert_eq!(prime_naive(1_000_003, &mut a), prime_6k(1_000_003, &mut b));
    assert!(a > 20 * b);
    report("primality test, n<3000 and 1000003", a, b);

    println!("12. cache-oblivious transpose (simulated LRU: 64 lines x 8 elements)");
    for n in [1usize, 2, 3, 7, 16, 33] {
        let src: Vec<i64> = (0..(n * n) as i64).collect();
        let (mut d1, mut d2) = (vec![0; n * n], vec![0; n * n]);
        let (mut c1, mut c2) = (LruCache::new(64, 8), LruCache::new(64, 8));
        transpose_naive(&src, &mut d1, n, &mut c1);
        transpose_rec(&src, &mut d2, n, (0, n, 0, n), &mut c2);
        assert_eq!(d1, d2);
        for i in 0..n {
            for j in 0..n {
                assert_eq!(d1[j * n + i], src[i * n + j]);
            }
        }
    }
    let n = 256;
    let src: Vec<i64> = (0..(n * n) as i64).collect();
    let (mut d1, mut d2) = (vec![0; n * n], vec![0; n * n]);
    let (mut c1, mut c2) = (LruCache::new(64, 8), LruCache::new(64, 8));
    transpose_naive(&src, &mut d1, n, &mut c1);
    transpose_rec(&src, &mut d2, n, (0, n, 0, n), &mut c2);
    assert_eq!(d1, d2);
    assert!(
        c2.misses * 2 < c1.misses,
        "recursive layout should miss far less"
    );
    report("transpose 256x256 cache misses", c1.misses, c2.misses);
    println!("all algorithmic_optimization_toolbox checks passed");
}
