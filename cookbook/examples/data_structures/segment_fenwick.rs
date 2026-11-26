//! RECIPE 131 (trees): Fenwick tree, monoid segment tree, lazy range-add segment tree, sparse table
//! LEVEL: intermediate
//! Use in: leaderboards and ranking, order statistics, time-series window queries, competitive-programming style range problems.   Run: cargo run --release --example segment_fenwick
#![allow(dead_code)]
//!
//! WHAT: four structures answering "aggregate over a range of an array" with different trade-offs:
//!   * `Fenwick` (BIT): point update + prefix/range SUM in O(log n), plus k-th element by binary lifting. Tiny and fast; needs an invertible op.
//!   * `SegTree<M>`: generic over a `Monoid` (identity + associative op), so one implementation gives sum, min and max; no inverse needed.
//!   * `LazySeg`: range ADD + range SUM; updates are deferred in `add` tags and pushed down only when a query must look inside a node.
//!   * `SparseTable`: STATIC range-min in O(1) after O(n log n) build, by overlapping two power-of-two windows (min is idempotent).
//! WHY: scanning a range is O(n) per query; these trade a little memory for O(log n) (or O(1)) queries under updates.
//! HOW: Fenwick node i covers the last `lowbit(i)` elements ending at i; the segment tree is the bottom-up array layout (leaves at n..2n).
//! Each structure is checked against a naive scan on seeded random arrays.
//! COMMON USES: game leaderboards (rank = count of higher scores); percentile/median over a stream (k-th via bucket counts); rolling
//! max/min over time-series windows; counting inversions; range-update dashboards; LCA via RMQ on an Euler tour.
//! Read: https://en.wikipedia.org/wiki/Fenwick_tree and https://en.wikipedia.org/wiki/Segment_tree
struct Rng(u64);
impl Rng {
    fn next(&mut self) -> u64 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        self.0
    }
    fn below(&mut self, n: usize) -> usize {
        (self.next() % n as u64) as usize
    }
    fn val(&mut self) -> i64 {
        self.below(2001) as i64 - 1000
    }
    /// Random half-open range with l <= r.
    fn range(&mut self, n: usize) -> (usize, usize) {
        let (a, b) = (self.below(n + 1), self.below(n + 1));
        (a.min(b), a.max(b))
    }
}

// ------------------------------------------------------------------ Fenwick
struct Fenwick {
    t: Vec<i64>, // 1-based; t[i] = sum of the lowbit(i) elements ending at i
}

impl Fenwick {
    fn new(n: usize) -> Self {
        Fenwick { t: vec![0; n + 1] }
    }
    fn len(&self) -> usize {
        self.t.len() - 1
    }
    fn add(&mut self, i: usize, delta: i64) {
        let mut i = i + 1;
        while i < self.t.len() {
            self.t[i] += delta;
            i += i & i.wrapping_neg(); // jump to the next node that covers i
        }
    }
    /// Sum of elements [0, i).
    fn prefix(&self, mut i: usize) -> i64 {
        let mut s = 0;
        while i > 0 {
            s += self.t[i];
            i &= i - 1; // clear the lowest set bit
        }
        s
    }
    fn range(&self, l: usize, r: usize) -> i64 {
        self.prefix(r) - self.prefix(l)
    }
    /// Smallest index whose prefix sum (inclusive) is >= k; requires non-negative elements (counts). None if the total is < k.
    fn kth(&self, mut k: i64) -> Option<usize> {
        let (n, mut pos) = (self.len(), 0);
        let mut step = n.checked_next_power_of_two()?;
        while step > 0 {
            // Binary lifting: take the big jump whenever the covered block is still too small.
            if pos + step <= n && self.t[pos + step] < k {
                pos += step;
                k -= self.t[pos];
            }
            step >>= 1;
        }
        (pos < n).then_some(pos)
    }
}

// --------------------------------------------------------------- segment tree
trait Monoid {
    type T: Clone;
    fn id() -> Self::T;
    fn op(a: &Self::T, b: &Self::T) -> Self::T;
}
struct Sum;
struct Min;
struct Max;
impl Monoid for Sum {
    type T = i64;
    fn id() -> i64 {
        0
    }
    fn op(a: &i64, b: &i64) -> i64 {
        a + b
    }
}
impl Monoid for Min {
    type T = i64;
    fn id() -> i64 {
        i64::MAX
    }
    fn op(a: &i64, b: &i64) -> i64 {
        *a.min(b)
    }
}
impl Monoid for Max {
    type T = i64;
    fn id() -> i64 {
        i64::MIN
    }
    fn op(a: &i64, b: &i64) -> i64 {
        *a.max(b)
    }
}

struct SegTree<M: Monoid> {
    n: usize,
    t: Vec<M::T>, // leaves at [n, 2n); node i = op(2i, 2i+1)
}

impl<M: Monoid> SegTree<M> {
    fn build(a: &[M::T]) -> Self {
        let n = a.len();
        let mut t = vec![M::id(); 2 * n];
        t[n..].clone_from_slice(a);
        for i in (1..n).rev() {
            t[i] = M::op(&t[2 * i], &t[2 * i + 1]);
        }
        SegTree { n, t }
    }
    fn update(&mut self, i: usize, v: M::T) {
        let mut i = i + self.n;
        self.t[i] = v;
        while i > 1 {
            i /= 2;
            self.t[i] = M::op(&self.t[2 * i], &self.t[2 * i + 1]);
        }
    }
    /// Aggregate of [l, r). Keeps left and right accumulators separately so the op need not be commutative.
    fn query(&self, l: usize, r: usize) -> M::T {
        let (mut l, mut r) = (l + self.n, r + self.n);
        let (mut left, mut right) = (M::id(), M::id());
        while l < r {
            if l & 1 == 1 {
                left = M::op(&left, &self.t[l]);
                l += 1;
            }
            if r & 1 == 1 {
                r -= 1;
                right = M::op(&self.t[r], &right);
            }
            l /= 2;
            r /= 2;
        }
        M::op(&left, &right)
    }
}

// ----------------------------------------------------------------- lazy range add
struct LazySeg {
    n: usize,
    sum: Vec<i64>,
    add: Vec<i64>, // pending "add v to every element of this node" not yet pushed to children
}

impl LazySeg {
    fn new(a: &[i64]) -> Self {
        let mut s = LazySeg {
            n: a.len(),
            sum: vec![0; 4 * a.len().max(1)],
            add: vec![0; 4 * a.len().max(1)],
        };
        s.build(1, 0, a.len(), a);
        s
    }
    fn build(&mut self, node: usize, lo: usize, hi: usize, a: &[i64]) {
        if hi - lo == 1 {
            self.sum[node] = a[lo];
            return;
        }
        let mid = (lo + hi) / 2;
        self.build(2 * node, lo, mid, a);
        self.build(2 * node + 1, mid, hi, a);
        self.sum[node] = self.sum[2 * node] + self.sum[2 * node + 1];
    }
    fn apply(&mut self, node: usize, len: usize, v: i64) {
        self.sum[node] += v * len as i64;
        self.add[node] += v;
    }
    fn push(&mut self, node: usize, lo: usize, hi: usize) {
        let (mid, v) = ((lo + hi) / 2, self.add[node]);
        if v != 0 {
            self.apply(2 * node, mid - lo, v);
            self.apply(2 * node + 1, hi - mid, v);
            self.add[node] = 0;
        }
    }
    fn range_add(&mut self, l: usize, r: usize, v: i64) {
        self.add_rec(1, 0, self.n, l, r, v);
    }
    fn add_rec(&mut self, node: usize, lo: usize, hi: usize, l: usize, r: usize, v: i64) {
        if r <= lo || hi <= l {
            return;
        }
        if l <= lo && hi <= r {
            return self.apply(node, hi - lo, v); // fully covered: stop here, leave children stale (lazy)
        }
        self.push(node, lo, hi);
        let mid = (lo + hi) / 2;
        self.add_rec(2 * node, lo, mid, l, r, v);
        self.add_rec(2 * node + 1, mid, hi, l, r, v);
        self.sum[node] = self.sum[2 * node] + self.sum[2 * node + 1];
    }
    fn range_sum(&mut self, l: usize, r: usize) -> i64 {
        self.sum_rec(1, 0, self.n, l, r)
    }
    fn sum_rec(&mut self, node: usize, lo: usize, hi: usize, l: usize, r: usize) -> i64 {
        if r <= lo || hi <= l {
            return 0;
        }
        if l <= lo && hi <= r {
            return self.sum[node];
        }
        self.push(node, lo, hi);
        let mid = (lo + hi) / 2;
        self.sum_rec(2 * node, lo, mid, l, r) + self.sum_rec(2 * node + 1, mid, hi, l, r)
    }
}

// ----------------------------------------------------------------- sparse table
struct SparseTable {
    levels: Vec<Vec<i64>>, // levels[j][i] = min of a[i .. i + 2^j]
}

impl SparseTable {
    fn build(a: &[i64]) -> Self {
        let mut levels = vec![a.to_vec()];
        let mut j = 1;
        while (1 << j) <= a.len() {
            let prev = &levels[j - 1];
            let half = 1 << (j - 1);
            let next = (0..=a.len() - (1 << j))
                .map(|i| prev[i].min(prev[i + half]))
                .collect();
            levels.push(next);
            j += 1;
        }
        SparseTable { levels }
    }
    /// Min of [l, r), r > l. Two overlapping windows cover the range; overlap is harmless because min is idempotent.
    fn min(&self, l: usize, r: usize) -> i64 {
        let j = (usize::BITS - 1 - (r - l).leading_zeros()) as usize;
        self.levels[j][l].min(self.levels[j][r - (1 << j)])
    }
}

// --------------------------------------------------------------------- tests
fn test_fenwick(r: &mut Rng) {
    let n = 300;
    let mut a = vec![0i64; n];
    let mut f = Fenwick::new(n);
    for _ in 0..5000 {
        let i = r.below(n);
        let d = r.val();
        a[i] += d;
        f.add(i, d);
        let (l, h) = r.range(n);
        assert_eq!(f.range(l, h), a[l..h].iter().sum::<i64>());
    }
    // k-th needs non-negative counts: rebuild as a histogram.
    let mut cnt = vec![0i64; 64];
    let mut f = Fenwick::new(64);
    for _ in 0..2000 {
        let b = r.below(64);
        cnt[b] += 1;
        f.add(b, 1);
        let k = 1 + r.below(2000.min(cnt.iter().sum::<i64>() as usize)) as i64;
        let naive = (0..64)
            .scan(0, |acc, i| {
                *acc += cnt[i];
                Some(*acc)
            })
            .position(|s| s >= k);
        assert_eq!(f.kth(k), naive);
    }
    assert_eq!(f.kth(1_000_000), None);
    println!("fenwick: 5000 sum ops + 2000 k-th ops match naive");
}

fn test_segtree(r: &mut Rng) {
    let n = 257; // deliberately not a power of two
    let mut a: Vec<i64> = (0..n).map(|_| r.val()).collect();
    let (mut s, mut mn, mut mx) = (
        SegTree::<Sum>::build(&a),
        SegTree::<Min>::build(&a),
        SegTree::<Max>::build(&a),
    );
    for _ in 0..5000 {
        let i = r.below(n);
        a[i] = r.val();
        (s.update(i, a[i]), mn.update(i, a[i]), mx.update(i, a[i]));
        let (l, h) = r.range(n);
        assert_eq!(s.query(l, h), a[l..h].iter().sum::<i64>());
        assert_eq!(
            mn.query(l, h),
            a[l..h].iter().copied().min().unwrap_or(i64::MAX)
        );
        assert_eq!(
            mx.query(l, h),
            a[l..h].iter().copied().max().unwrap_or(i64::MIN)
        );
    }
    println!("segment tree (sum/min/max): 5000 updates + queries match naive");
}

fn test_lazy_and_sparse(r: &mut Rng) {
    let n = 200;
    let mut a: Vec<i64> = (0..n).map(|_| r.val()).collect();
    let mut z = LazySeg::new(&a);
    for _ in 0..5000 {
        let (l, h) = r.range(n);
        if r.below(2) == 0 {
            let v = r.val();
            a[l..h].iter_mut().for_each(|x| *x += v);
            z.range_add(l, h, v);
        } else {
            assert_eq!(z.range_sum(l, h), a[l..h].iter().sum::<i64>());
        }
    }
    println!("lazy segment tree: 5000 range-add/range-sum ops match naive");
    let a: Vec<i64> = (0..1000).map(|_| r.val()).collect();
    let st = SparseTable::build(&a);
    for _ in 0..20000 {
        let (l, h) = r.range(a.len());
        if l < h {
            assert_eq!(st.min(l, h), *a[l..h].iter().min().unwrap());
        }
    }
    println!("sparse table: 20000 O(1) RMQ queries match naive");
}

fn demos() {
    // Leaderboard: scores 0..=100; rank of a score = number of players strictly above it.
    let mut board = Fenwick::new(101);
    let scores = [50, 80, 80, 20, 95, 60];
    scores.iter().for_each(|&s| board.add(s, 1));
    let above = |s: usize| board.range(s + 1, 101);
    println!(
        "leaderboard: players above 60 = {}, above 95 = {}",
        above(60),
        above(95)
    );
    assert_eq!((above(60), above(95)), (3, 0));
    // Order statistic: median of the stream so far = k-th with k = (count+1)/2.
    let median = board.kth((scores.len() as i64 + 1) / 2).unwrap();
    println!("median score = {median}");
    assert_eq!(median, 60);
    // Time-series window: max over the last 4 samples, via a segment tree.
    let cpu = [12, 40, 33, 90, 15, 20, 18, 25];
    let t = SegTree::<Max>::build(&cpu);
    println!("max of samples [3,7) = {}", t.query(3, 7));
    assert_eq!(t.query(3, 7), 90);
    // Static data: sparse table gives min of any window in O(1).
    let st = SparseTable::build(&cpu);
    assert_eq!(st.min(0, 8), 12);
}

fn main() {
    let mut r = Rng(0xDEADBEEFCAFEF00D);
    demos();
    test_fenwick(&mut r);
    test_segtree(&mut r);
    test_lazy_and_sparse(&mut r);
}
