//! RECIPE 132 (trees): a split/merge treap with pluggable augmentation: order statistics and an interval tree
//! LEVEL: advanced
//! Use in: leaderboards (rank/k-th), calendar and booking overlap checks, genome interval queries, any "augmented BST".   Run: cargo run --release --example treap_interval
#![allow(dead_code)]
//!
//! WHAT: a treap is a BST on keys that is simultaneously a max-heap on random priorities. Random priorities make its shape that of a
//! random BST, so depth is O(log n) EXPECTED with no rotations or balance bookkeeping: everything is built from two primitives,
//! `split(t, key)` (cut into < key and >= key) and `merge(a, b)` (join two treaps where all of a < all of b).
//! insert = split + merge; remove = split twice + merge; arena-based (`Vec` of nodes, `usize` ids, arena_ids).
//! AUGMENTATION is the general technique: each node caches an aggregate of its subtree, recomputed in `pull` after any structural
//! change. `size` gives order statistics (rank, k-th, range count). A `max hi` aggregate over (lo, hi, id) keys gives an INTERVAL TREE:
//! a whole subtree whose max endpoint is < a can be skipped, so "which intervals overlap [a, b]" is output-sensitive.
//! The augmentation is a trait (`Augment`), so the same treap code serves both uses.
//! Verified against a sorted `Vec` / naive interval scan with 10k seeded random operations each, plus a structural checker.
//! COMMON USES: order-statistic trees (rank/percentile queries); interval trees in calendar double-booking checks, range-based
//! scheduling, and genomics tools that intersect regions (BED-style intervals); Cartesian-tree/rope style sequences (implicit treaps).
//! Read: https://en.wikipedia.org/wiki/Treap and https://en.wikipedia.org/wiki/Interval_tree
const NIL: usize = usize::MAX;

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
}

/// A subtree summary: `leaf` summarises one key, `combine` merges summaries in key order.
trait Augment<K> {
    type A: Copy;
    fn leaf(k: &K) -> Self::A;
    fn combine(left: Self::A, right: Self::A) -> Self::A;
}

struct NoAug;
impl<K> Augment<K> for NoAug {
    type A = ();
    fn leaf(_: &K) {}
    fn combine(_: (), _: ()) {}
}

/// Interval key: (lo, hi, unique id), so equal intervals can coexist. Aggregate = max `hi` in the subtree.
type Iv = (i64, i64, u32);
struct MaxHi;
impl Augment<Iv> for MaxHi {
    type A = i64;
    fn leaf(k: &Iv) -> i64 {
        k.1
    }
    fn combine(a: i64, b: i64) -> i64 {
        a.max(b)
    }
}

struct Node<K, G: Augment<K>> {
    key: K,
    pri: u64,
    l: usize,
    r: usize,
    size: usize,
    agg: G::A,
}

struct Treap<K, G: Augment<K>> {
    nodes: Vec<Node<K, G>>,
    free: Vec<usize>,
    root: usize,
    rng: Rng,
}

impl<K: Ord + Copy, G: Augment<K>> Treap<K, G> {
    fn new(seed: u64) -> Self {
        Treap {
            nodes: vec![],
            free: vec![],
            root: NIL,
            rng: Rng(seed),
        }
    }
    fn len(&self) -> usize {
        self.size(self.root)
    }
    fn size(&self, t: usize) -> usize {
        if t == NIL { 0 } else { self.nodes[t].size }
    }
    /// Recompute a node's cached size and aggregate from its children. Call after EVERY child change.
    fn pull(&mut self, t: usize) {
        let (l, r) = (self.nodes[t].l, self.nodes[t].r);
        let (mut size, mut agg) = (1, G::leaf(&self.nodes[t].key));
        if l != NIL {
            size += self.nodes[l].size;
            agg = G::combine(self.nodes[l].agg, agg);
        }
        if r != NIL {
            size += self.nodes[r].size;
            agg = G::combine(agg, self.nodes[r].agg);
        }
        (self.nodes[t].size, self.nodes[t].agg) = (size, agg);
    }
    fn alloc(&mut self, key: K) -> usize {
        let pri = self.rng.next();
        let node = Node {
            key,
            pri,
            l: NIL,
            r: NIL,
            size: 1,
            agg: G::leaf(&key),
        };
        if let Some(i) = self.free.pop() {
            self.nodes[i] = node;
            i
        } else {
            self.nodes.push(node);
            self.nodes.len() - 1
        }
    }
    /// Split into (keys < key, keys >= key); with `inclusive`, keys == key go left instead.
    fn split(&mut self, t: usize, key: &K, inclusive: bool) -> (usize, usize) {
        if t == NIL {
            return (NIL, NIL);
        }
        let goes_left = self.nodes[t].key < *key || (inclusive && self.nodes[t].key == *key);
        if goes_left {
            let (a, b) = self.split(self.nodes[t].r, key, inclusive);
            self.nodes[t].r = a;
            self.pull(t);
            (t, b)
        } else {
            let (a, b) = self.split(self.nodes[t].l, key, inclusive);
            self.nodes[t].l = b;
            self.pull(t);
            (a, t)
        }
    }
    /// Join two treaps where every key in `a` is below every key in `b`; the higher priority becomes the root.
    fn merge(&mut self, a: usize, b: usize) -> usize {
        if a == NIL || b == NIL {
            return a.min(b); // NIL is usize::MAX, so min picks the real node
        }
        if self.nodes[a].pri > self.nodes[b].pri {
            let m = self.merge(self.nodes[a].r, b);
            self.nodes[a].r = m;
            self.pull(a);
            a
        } else {
            let m = self.merge(a, self.nodes[b].l);
            self.nodes[b].l = m;
            self.pull(b);
            b
        }
    }
    fn contains(&self, key: &K) -> bool {
        let mut t = self.root;
        while t != NIL {
            match key.cmp(&self.nodes[t].key) {
                std::cmp::Ordering::Equal => return true,
                std::cmp::Ordering::Less => t = self.nodes[t].l,
                std::cmp::Ordering::Greater => t = self.nodes[t].r,
            }
        }
        false
    }
    fn insert(&mut self, key: K) -> bool {
        if self.contains(&key) {
            return false;
        }
        let (a, b) = self.split(self.root, &key, false);
        let n = self.alloc(key);
        let left = self.merge(a, n);
        self.root = self.merge(left, b);
        true
    }
    fn remove(&mut self, key: &K) -> bool {
        if !self.contains(key) {
            return false;
        }
        let (a, rest) = self.split(self.root, key, false);
        let (mid, b) = self.split(rest, key, true); // `mid` is exactly the node holding `key`
        self.free.push(mid);
        self.root = self.merge(a, b);
        true
    }
    /// Number of keys strictly below `key`.
    fn rank(&self, key: &K) -> usize {
        let (mut t, mut cnt) = (self.root, 0);
        while t != NIL {
            if *key <= self.nodes[t].key {
                t = self.nodes[t].l;
            } else {
                cnt += 1 + self.size(self.nodes[t].l);
                t = self.nodes[t].r;
            }
        }
        cnt
    }
    /// The i-th smallest key (0-based).
    fn kth(&self, mut i: usize) -> Option<K> {
        let mut t = self.root;
        while t != NIL {
            let ls = self.size(self.nodes[t].l);
            if i < ls {
                t = self.nodes[t].l;
            } else if i == ls {
                return Some(self.nodes[t].key);
            } else {
                i -= ls + 1;
                t = self.nodes[t].r;
            }
        }
        None
    }
    fn range_count(&self, lo: &K, hi: &K) -> usize {
        self.rank(hi).saturating_sub(self.rank(lo))
    }
    fn height(&self, t: usize) -> usize {
        if t == NIL {
            0
        } else {
            1 + self
                .height(self.nodes[t].l)
                .max(self.height(self.nodes[t].r))
        }
    }
    /// BST order, heap order on priorities, and correct cached sizes/aggregates; returns the in-order keys.
    fn check(&self) -> Vec<K> {
        let mut out = vec![];
        self.check_rec(self.root, &mut out);
        assert!(
            out.windows(2).all(|w| w[0] < w[1]),
            "in-order keys strictly increasing"
        );
        assert_eq!(out.len(), self.len());
        assert_eq!(
            out.len() + self.free.len(),
            self.nodes.len(),
            "no leaked nodes"
        );
        out
    }
    fn check_rec(&self, t: usize, out: &mut Vec<K>) {
        if t == NIL {
            return;
        }
        let n = &self.nodes[t];
        for c in [n.l, n.r].into_iter().filter(|&c| c != NIL) {
            assert!(self.nodes[c].pri <= n.pri, "heap property");
        }
        self.check_rec(n.l, out);
        out.push(n.key);
        self.check_rec(n.r, out);
        assert_eq!(n.size, 1 + self.size(n.l) + self.size(n.r), "cached size");
    }
}

// ------------------------------------------------------------ interval queries
impl Treap<Iv, MaxHi> {
    fn add(&mut self, lo: i64, hi: i64, id: u32) {
        assert!(lo <= hi);
        self.insert((lo, hi, id));
    }
    /// All stored intervals overlapping the closed interval [a, b], sorted by (lo, hi, id).
    fn overlapping(&self, a: i64, b: i64) -> Vec<Iv> {
        let mut out = vec![];
        self.overlap_rec(self.root, a, b, &mut out);
        out
    }
    fn overlap_rec(&self, t: usize, a: i64, b: i64, out: &mut Vec<Iv>) {
        if t == NIL || self.nodes[t].agg < a {
            return; // nothing in this subtree reaches as far right as `a`: prune it whole
        }
        let n = &self.nodes[t];
        self.overlap_rec(n.l, a, b, out);
        if n.key.0 > b {
            return; // this node and everything to its right starts after `b`
        }
        if n.key.1 >= a {
            out.push(n.key);
        }
        self.overlap_rec(n.r, a, b, out);
    }
    /// Stabbing query: which intervals contain the point p?
    fn stabbing(&self, p: i64) -> Vec<Iv> {
        self.overlapping(p, p)
    }
    /// Verify every cached max-hi equals a recomputation.
    fn check_agg(&self, t: usize) -> i64 {
        if t == NIL {
            return i64::MIN;
        }
        let n = &self.nodes[t];
        let m = self.check_agg(n.l).max(self.check_agg(n.r)).max(n.key.1);
        assert_eq!(n.agg, m, "cached max hi");
        m
    }
}

fn test_order_statistics() {
    let mut r = Rng(7);
    let mut t: Treap<i64, NoAug> = Treap::new(99);
    let mut model: Vec<i64> = vec![]; // sorted
    for step in 0..10_000 {
        let k = r.below(600) as i64;
        let pos = model.binary_search(&k);
        if r.below(100) < 60 {
            assert_eq!(t.insert(k), pos.is_err());
            if let Err(i) = pos {
                model.insert(i, k)
            }
        } else {
            assert_eq!(t.remove(&k), pos.is_ok());
            if let Ok(i) = pos {
                model.remove(i);
            }
        }
        let rk = r.below(600) as i64;
        assert_eq!(t.rank(&rk), model.partition_point(|&x| x < rk));
        let i = r.below(model.len() + 2);
        assert_eq!(t.kth(i), model.get(i).copied());
        let (lo, hi) = (r.below(600) as i64, r.below(600) as i64);
        let want = model
            .partition_point(|&x| x < hi)
            .saturating_sub(model.partition_point(|&x| x < lo));
        assert_eq!(t.range_count(&lo, &hi), want);
        if step % 500 == 0 {
            assert_eq!(t.check(), model);
        }
    }
    assert_eq!(t.check(), model);
    println!(
        "order statistics: 10000 insert/remove/rank/kth/range_count ops match sorted Vec ({} keys left)",
        model.len()
    );
}

fn test_intervals() {
    let mut r = Rng(1234);
    let mut t: Treap<Iv, MaxHi> = Treap::new(5);
    let mut model: Vec<Iv> = vec![];
    for id in 0..10_000u32 {
        if model.is_empty() || r.below(100) < 60 {
            let lo = r.below(2000) as i64;
            let iv = (lo, lo + r.below(120) as i64, id);
            t.add(iv.0, iv.1, iv.2);
            model.push(iv);
        } else {
            let iv = model.swap_remove(r.below(model.len()));
            assert!(t.remove(&iv));
        }
        let (a, b) = (r.below(2100) as i64, r.below(2100) as i64);
        let (a, b) = (a.min(b), a.max(b));
        let mut want: Vec<Iv> = model
            .iter()
            .copied()
            .filter(|&(lo, hi, _)| lo <= b && hi >= a)
            .collect();
        want.sort();
        assert_eq!(t.overlapping(a, b), want, "overlap [{a},{b}]");
        let mut stab: Vec<Iv> = model
            .iter()
            .copied()
            .filter(|&(lo, hi, _)| lo <= a && a <= hi)
            .collect();
        stab.sort();
        assert_eq!(t.stabbing(a), stab);
    }
    t.check_agg(t.root);
    t.check();
    println!(
        "interval tree: 10000 add/remove ops, overlap + stabbing queries match naive scan ({} intervals)",
        model.len()
    );
}

fn demo() {
    // Sorted insertion is the worst case for a plain BST (a linked list); random priorities keep the treap shallow.
    let mut t: Treap<i64, NoAug> = Treap::new(1);
    for k in 0..100_000 {
        t.insert(k);
    }
    let h = t.height(t.root);
    println!("100000 sequential inserts: height {h} (log2 n = 17, a plain BST would be 100000)");
    assert!(h < 60);
    assert_eq!((t.kth(99_999), t.rank(&5000)), (Some(99_999), 5000));
    // Calendar booking: does a proposed meeting clash with an existing one?
    let mut cal: Treap<Iv, MaxHi> = Treap::new(2);
    for (id, (s, e)) in [(9, 10), (11, 12), (14, 16)].into_iter().enumerate() {
        cal.add(s, e, id as u32);
    }
    println!("clashes with 10..11? {:?}", cal.overlapping(10, 11));
    println!("who is booked at 15? {:?}", cal.stabbing(15));
    assert_eq!(cal.overlapping(12, 13).len(), 1);
    assert!(cal.overlapping(12 + 1, 13).is_empty());
    assert_eq!(cal.stabbing(15), [(14, 16, 2)]);
}

fn main() {
    demo();
    test_order_statistics();
    test_intervals();
}
