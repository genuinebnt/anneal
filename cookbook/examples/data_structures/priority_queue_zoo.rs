//! RECIPE 232 (data-structures): a priority queue zoo: pairing heap, leftist heap, indexed 4-ary heap, Dial's bucket queue, radix heap
//! LEVEL: advanced
//! Use in: Dijkstra/Prim/A*, event simulation, schedulers, mergeable queues, anything where `BinaryHeap` lacks decrease-key, meld, or a monotone fast path.   Run: cargo run --release --example priority_queue_zoo
//! COMMON USES: shortest paths on road/network graphs (Dial's and radix heaps for integer weights), discrete-event simulators and timer wheels (bucket
//! queues), mergeable priority queues in functional/persistent code (leftist and skew heaps), Prim's MST and Dijkstra with decrease-key (d-ary or pairing
//! heaps), k-way merge, task schedulers whose keys only move forward in time (monotone queues).
#![allow(dead_code)]
//!
//! What: five structures beyond `std::collections::BinaryHeap`, all ARENA based (parallel Vecs + `usize` indices + free list, no Box, no Rc, no unsafe):
//!  * `PNodes` pairing heap: a heap-ordered tree where the min's children form a sibling list. insert and meld are one comparison (O(1)); pop-min merges
//!    the children with the two-pass pairing rule (amortised O(log n)); decrease-key cuts the node out and re-links it with the root. Simple, and very fast
//!    in practice, though its exact decrease-key bound is subtle (between Omega(log log n) and O(2^(2*sqrt(log log n))) amortised, Fredman 1999 / Pettie 2005).
//!  * `LNodes` leftist heap: every node keeps its rank (shortest path to a missing child) and keeps the right spine short by swapping children, so meld
//!    is O(log n) WORST case and everything else is built on meld: insert = meld with a singleton, pop = meld(left, right). Because both live in one
//!    arena, any two heaps in it can be melded without copying: that is the point of a mergeable queue (tested with 8 heaps melded at random).
//!  * `DHeap<D>` indexed d-ary heap with decrease-key (`pos[id]` tracks where each id sits). D=4 makes the tree half as deep as a binary heap and the 4 children of a
//!    node are adjacent in memory (one or two cache lines of u32 ids), so sift-down does fewer, more predictable cache misses at the price of 4 comparisons
//!    per level. Sift-up (the cheap decrease-key path) gets strictly cheaper. Caveat measured below: this swap-based indexed version is slower than std's hole-based BinaryHeap for plain push/pop; the win needs decrease-key-heavy or very large workloads and a tuned sift.
//!  * `Dial` bucket queue: keys are small integers <= C apart, so a circular array of C+1 buckets replaces comparisons; push is O(1), pop scans forward.
//!  * `Radix` heap: for MONOTONE keys (never pushed below the last popped key). Bucket i holds keys whose highest differing bit versus `last` is i;
//!    popping redistributes the first non-empty bucket downward, so each element moves at most 64 times: O(log C) amortised, no weight bound needed.
//! Verified against `BinaryHeap`/`BTreeSet` oracles on seeded random operation sequences, and as Dijkstra queues: every variant must produce exactly the
//! distances of the `BinaryHeap` Dijkstra on random graphs. A 1M-operation benchmark is printed only (timings are never asserted).
//!
//!   structure        insert        pop-min        decrease-key        meld        wins when
//!   binary heap      O(log n)*     O(log n)       O(log n) indexed    O(n)        general purpose; std is hard to beat for plain push/pop
//!   4-ary indexed    O(log4 n)     O(4 log4 n)    O(log4 n)           O(n)        Dijkstra/Prim with decrease-key on big graphs (cache friendly)
//!   pairing          O(1)          O(log n) am.   o(log n) am. (open) O(1)        decrease-key heavy loads and cheap meld, small constants
//!   leftist          O(log n)      O(log n)       (cut + meld)        O(log n)    you need worst-case O(log n) meld / persistence / merging queues
//!   Dial (bucket)    O(1)          O(1)+scan      O(1) lazy           -           small integer weights <= C, total O(m + n*C) for Dijkstra
//!   radix            O(1)          O(log C) am.   O(1) lazy           -           monotone integer keys of any size (Dijkstra with u32/u64 weights)
//!   (* amortised O(1) average for random keys)
//! Read: https://en.wikipedia.org/wiki/Pairing_heap , https://en.wikipedia.org/wiki/Leftist_tree , https://en.wikipedia.org/wiki/D-ary_heap ,
//! https://en.wikipedia.org/wiki/Dial%27s_algorithm , https://en.wikipedia.org/wiki/Radix_heap , https://doc.rust-lang.org/std/collections/struct.BinaryHeap.html
use std::cmp::Reverse;
use std::collections::{BTreeSet, BinaryHeap};
use std::time::Instant;

const NIL: usize = usize::MAX;
struct Rng(u64);
impl Rng {
    fn next(&mut self) -> u64 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        self.0
    }
    fn below(&mut self, n: u64) -> u64 {
        self.next() % n
    }
}

// ------------------------------------------------------------------ pairing heap in an arena (many roots may share the arena)
#[derive(Default)]
struct PNodes {
    key: Vec<u64>,
    val: Vec<u32>,
    child: Vec<usize>,
    sib: Vec<usize>,
    prev: Vec<usize>,
    free: Vec<usize>,
    scratch: Vec<usize>,
}
impl PNodes {
    fn new_node(&mut self, key: u64, val: u32) -> usize {
        if let Some(i) = self.free.pop() {
            self.key[i] = key;
            self.val[i] = val;
            self.child[i] = NIL;
            self.sib[i] = NIL;
            self.prev[i] = NIL;
            i
        } else {
            self.key.push(key);
            self.val.push(val);
            self.child.push(NIL);
            self.sib.push(NIL);
            self.prev.push(NIL);
            self.key.len() - 1
        }
    }
    /// meld two ROOTS (O(1)): the larger root becomes the first child of the smaller.
    fn meld(&mut self, a: usize, b: usize) -> usize {
        if a == NIL {
            return b;
        }
        if b == NIL {
            return a;
        }
        let (a, b) = if self.key[b] < self.key[a] {
            (b, a)
        } else {
            (a, b)
        };
        let c = self.child[a];
        self.sib[b] = c;
        if c != NIL {
            self.prev[c] = b;
        }
        self.prev[b] = a;
        self.child[a] = b;
        a
    }
    /// remove the root; returns (key, val, new root)
    fn pop_root(&mut self, root: usize) -> (u64, u32, usize) {
        let (k, v) = (self.key[root], self.val[root]);
        let mut list = std::mem::take(&mut self.scratch);
        list.clear();
        let mut c = self.child[root];
        while c != NIL {
            let n = self.sib[c];
            self.sib[c] = NIL;
            self.prev[c] = NIL;
            list.push(c);
            c = n;
        }
        let mut paired = 0; // pass 1, left to right: meld in pairs (results written back into the front of `list`)
        let mut i = 0;
        while i < list.len() {
            let m = if i + 1 < list.len() {
                self.meld(list[i], list[i + 1])
            } else {
                list[i]
            };
            list[paired] = m;
            paired += 1;
            i += 2;
        }
        let mut acc = NIL; // pass 2, right to left: fold into one tree
        for j in (0..paired).rev() {
            acc = self.meld(acc, list[j]);
        }
        self.scratch = list;
        self.free.push(root);
        (k, v, acc)
    }
    /// lower `h`'s key; returns the (possibly new) root
    fn decrease_key(&mut self, root: usize, h: usize, new_key: u64) -> usize {
        assert!(new_key <= self.key[h]);
        self.key[h] = new_key;
        if h == root {
            return root;
        }
        let (p, s) = (self.prev[h], self.sib[h]);
        if self.child[p] == h {
            self.child[p] = s
        } else {
            self.sib[p] = s
        } // `prev` is the parent or the left sibling
        if s != NIL {
            self.prev[s] = p;
        }
        self.prev[h] = NIL;
        self.sib[h] = NIL;
        self.meld(root, h)
    }
}
struct PairingHeap {
    n: PNodes,
    root: usize,
    len: usize,
}
impl PairingHeap {
    fn new() -> Self {
        PairingHeap {
            n: PNodes::default(),
            root: NIL,
            len: 0,
        }
    }
    fn insert(&mut self, key: u64, val: u32) -> usize {
        let h = self.n.new_node(key, val);
        self.root = self.n.meld(self.root, h);
        self.len += 1;
        h
    }
    fn pop_min(&mut self) -> Option<(u64, u32)> {
        if self.root == NIL {
            return None;
        }
        let (k, v, r) = self.n.pop_root(self.root);
        self.root = r;
        self.len -= 1;
        Some((k, v))
    }
    fn peek(&self) -> Option<u64> {
        if self.root == NIL {
            None
        } else {
            Some(self.n.key[self.root])
        }
    }
    fn decrease_key(&mut self, h: usize, k: u64) {
        self.root = self.n.decrease_key(self.root, h, k);
    }
}

// ------------------------------------------------------------------ leftist heap in an arena
#[derive(Default)]
struct LNodes {
    key: Vec<u64>,
    val: Vec<u32>,
    left: Vec<usize>,
    right: Vec<usize>,
    rank: Vec<u32>,
    free: Vec<usize>,
}
impl LNodes {
    fn rank_of(&self, i: usize) -> u32 {
        if i == NIL { 0 } else { self.rank[i] }
    }
    fn singleton(&mut self, key: u64, val: u32) -> usize {
        if let Some(i) = self.free.pop() {
            self.key[i] = key;
            self.val[i] = val;
            self.left[i] = NIL;
            self.right[i] = NIL;
            self.rank[i] = 1;
            i
        } else {
            self.key.push(key);
            self.val.push(val);
            self.left.push(NIL);
            self.right.push(NIL);
            self.rank.push(1);
            self.key.len() - 1
        }
    }
    /// O(log n) worst case: recursion only walks the right spines, whose length is at most log2(n+1).
    fn meld(&mut self, a: usize, b: usize) -> usize {
        if a == NIL {
            return b;
        }
        if b == NIL {
            return a;
        }
        let (a, b) = if self.key[b] < self.key[a] {
            (b, a)
        } else {
            (a, b)
        };
        let r = self.meld(self.right[a], b);
        self.right[a] = r;
        if self.rank_of(self.left[a]) < self.rank_of(self.right[a]) {
            let l = self.left[a];
            self.left[a] = self.right[a];
            self.right[a] = l;
        }
        self.rank[a] = self.rank_of(self.right[a]) + 1;
        a
    }
    fn pop_root(&mut self, root: usize) -> (u64, u32, usize) {
        let (k, v, nr) = (
            self.key[root],
            self.val[root],
            self.meld(self.left[root], self.right[root]),
        );
        self.free.push(root);
        (k, v, nr)
    }
}

// ------------------------------------------------------------------ indexed d-ary heap with decrease-key
struct DHeap<const D: usize> {
    heap: Vec<u32>,
    pos: Vec<u32>,
    key: Vec<u64>,
}
impl<const D: usize> DHeap<D> {
    const ABSENT: u32 = u32::MAX;
    fn new(ids: usize) -> Self {
        DHeap {
            heap: Vec::new(),
            pos: vec![Self::ABSENT; ids],
            key: vec![0; ids],
        }
    }
    fn contains(&self, id: u32) -> bool {
        self.pos[id as usize] != Self::ABSENT
    }
    fn swap(&mut self, i: usize, j: usize) {
        self.heap.swap(i, j);
        self.pos[self.heap[i] as usize] = i as u32;
        self.pos[self.heap[j] as usize] = j as u32;
    }
    fn sift_up(&mut self, mut i: usize) {
        while i > 0 {
            let p = (i - 1) / D;
            if self.key[self.heap[i] as usize] < self.key[self.heap[p] as usize] {
                self.swap(i, p);
                i = p;
            } else {
                break;
            }
        }
    }
    fn sift_down(&mut self, mut i: usize) {
        loop {
            let first = i * D + 1;
            if first >= self.heap.len() {
                break;
            }
            let last = (first + D).min(self.heap.len());
            let best = (first..last)
                .min_by_key(|&c| self.key[self.heap[c] as usize])
                .unwrap();
            if self.key[self.heap[best] as usize] < self.key[self.heap[i] as usize] {
                self.swap(i, best);
                i = best;
            } else {
                break;
            }
        }
    }
    /// insert, or lower the key if already present (a larger key is ignored). Returns true if the heap changed.
    fn push_or_decrease(&mut self, id: u32, k: u64) -> bool {
        if !self.contains(id) {
            self.key[id as usize] = k;
            self.pos[id as usize] = self.heap.len() as u32;
            self.heap.push(id);
            self.sift_up(self.heap.len() - 1);
            true
        } else if k < self.key[id as usize] {
            self.key[id as usize] = k;
            self.sift_up(self.pos[id as usize] as usize);
            true
        } else {
            false
        }
    }
    fn pop_min(&mut self) -> Option<(u32, u64)> {
        if self.heap.is_empty() {
            return None;
        }
        let last = self.heap.len() - 1;
        self.swap(0, last);
        let id = self.heap.pop().unwrap();
        self.pos[id as usize] = Self::ABSENT;
        if !self.heap.is_empty() {
            self.sift_down(0);
        }
        Some((id, self.key[id as usize]))
    }
    fn len(&self) -> usize {
        self.heap.len()
    }
}

// ------------------------------------------------------------------ monotone queues: Dial, radix, and a BinaryHeap adapter
trait MonoPq {
    fn push(&mut self, key: u64, val: u32);
    fn pop(&mut self) -> Option<(u64, u32)>;
}
struct Std(BinaryHeap<Reverse<(u64, u32)>>);
impl MonoPq for Std {
    fn push(&mut self, k: u64, v: u32) {
        self.0.push(Reverse((k, v)));
    }
    fn pop(&mut self) -> Option<(u64, u32)> {
        self.0.pop().map(|Reverse(x)| x)
    }
}
/// Dial: requires every pushed key to lie in [last popped, last popped + C].
struct Dial {
    buckets: Vec<Vec<(u64, u32)>>,
    cur: u64,
    count: usize,
}
impl Dial {
    fn new(c: u64) -> Self {
        Dial {
            buckets: vec![Vec::new(); c as usize + 1],
            cur: 0,
            count: 0,
        }
    }
}
impl MonoPq for Dial {
    fn push(&mut self, k: u64, v: u32) {
        assert!(
            k >= self.cur && k - self.cur < self.buckets.len() as u64,
            "key outside the Dial window"
        );
        let m = self.buckets.len() as u64;
        self.buckets[(k % m) as usize].push((k, v));
        self.count += 1;
    }
    fn pop(&mut self) -> Option<(u64, u32)> {
        if self.count == 0 {
            return None;
        }
        let m = self.buckets.len() as u64;
        while self.buckets[(self.cur % m) as usize].is_empty() {
            self.cur += 1;
        }
        self.count -= 1;
        self.buckets[(self.cur % m) as usize].pop()
    }
}
struct Radix {
    b: Vec<Vec<(u64, u32)>>,
    last: u64,
    len: usize,
}
impl Radix {
    fn new() -> Self {
        Radix {
            b: vec![Vec::new(); 65],
            last: 0,
            len: 0,
        }
    }
    fn bucket(&self, k: u64) -> usize {
        64 - (k ^ self.last).leading_zeros() as usize
    } // 0 when k == last
}
impl MonoPq for Radix {
    fn push(&mut self, k: u64, v: u32) {
        assert!(k >= self.last, "radix heap is monotone");
        let i = self.bucket(k);
        self.b[i].push((k, v));
        self.len += 1;
    }
    fn pop(&mut self) -> Option<(u64, u32)> {
        if self.len == 0 {
            return None;
        }
        if self.b[0].is_empty() {
            let i = (1..65).find(|&i| !self.b[i].is_empty()).unwrap();
            self.last = self.b[i].iter().map(|x| x.0).min().unwrap();
            for (k, v) in std::mem::take(&mut self.b[i]) {
                let j = self.bucket(k);
                self.b[j].push((k, v));
            } // all land in buckets < i
        }
        self.len -= 1;
        self.b[0].pop()
    }
}

// ------------------------------------------------------------------ graphs + Dijkstra variants
struct Graph {
    adj: Vec<Vec<(u32, u32)>>,
    maxw: u32,
}
fn random_graph(n: usize, m: usize, maxw: u32, seed: u64) -> Graph {
    let mut g = Rng(seed);
    let mut adj = vec![Vec::new(); n];
    for i in 1..n {
        if g.below(10) < 8 {
            let j = g.below(i as u64) as usize;
            adj[j].push((i as u32, 1 + g.below(maxw as u64) as u32));
        }
    }
    for _ in 0..m {
        let (a, b) = (g.below(n as u64) as usize, g.below(n as u64) as u32);
        adj[a].push((b, 1 + g.below(maxw as u64) as u32));
    }
    Graph { adj, maxw }
}
fn dijkstra_lazy<Q: MonoPq>(g: &Graph, src: usize, mut q: Q) -> Vec<u64> {
    let mut dist = vec![u64::MAX; g.adj.len()];
    dist[src] = 0;
    q.push(0, src as u32);
    while let Some((d, u)) = q.pop() {
        if d > dist[u as usize] {
            continue;
        } // stale entry (lazy deletion instead of decrease-key)
        for &(v, w) in &g.adj[u as usize] {
            let nd = d + w as u64;
            if nd < dist[v as usize] {
                dist[v as usize] = nd;
                q.push(nd, v);
            }
        }
    }
    dist
}
fn dijkstra_dary<const D: usize>(g: &Graph, src: usize) -> Vec<u64> {
    let mut dist = vec![u64::MAX; g.adj.len()];
    dist[src] = 0;
    let mut h = DHeap::<D>::new(g.adj.len());
    h.push_or_decrease(src as u32, 0);
    while let Some((u, d)) = h.pop_min() {
        for &(v, w) in &g.adj[u as usize] {
            let nd = d + w as u64;
            if nd < dist[v as usize] {
                dist[v as usize] = nd;
                h.push_or_decrease(v, nd);
            }
        }
    }
    dist
}
fn dijkstra_pairing(g: &Graph, src: usize) -> Vec<u64> {
    let n = g.adj.len();
    let mut dist = vec![u64::MAX; n];
    dist[src] = 0;
    let mut handle = vec![NIL; n];
    let mut h = PairingHeap::new();
    handle[src] = h.insert(0, src as u32);
    while let Some((d, u)) = h.pop_min() {
        handle[u as usize] = NIL; // popped nodes are recycled, so forget the handle
        for &(v, w) in &g.adj[u as usize] {
            let nd = d + w as u64;
            if nd < dist[v as usize] {
                dist[v as usize] = nd;
                if handle[v as usize] == NIL {
                    handle[v as usize] = h.insert(nd, v)
                } else {
                    h.decrease_key(handle[v as usize], nd)
                }
            }
        }
    }
    dist
}

fn main() {
    let mut g = Rng(0xDEAD_BEEF_CAFE_F00D);

    // ---- pairing heap vs oracle: insert / pop-min / decrease-key (ids are payloads; ties make payload order arbitrary, so only keys are compared)
    let mut ph = PairingHeap::new();
    let mut oracle: BTreeSet<(u64, u32)> = BTreeSet::new();
    let (mut handles, mut keys) = (Vec::<usize>::new(), Vec::<u64>::new());
    let mut live: Vec<u32> = vec![];
    for _ in 0..50_000 {
        match g.below(10) {
            0..=3 => {
                let k = g.below(1_000_000);
                let id = handles.len() as u32;
                handles.push(ph.insert(k, id));
                keys.push(k);
                oracle.insert((k, id));
                live.push(id);
            }
            4..=6 if !live.is_empty() => {
                let idx = g.below(live.len() as u64) as usize;
                let id = live[idx];
                let nk = keys[id as usize].saturating_sub(g.below(5000));
                oracle.remove(&(keys[id as usize], id));
                oracle.insert((nk, id));
                keys[id as usize] = nk;
                ph.decrease_key(handles[id as usize], nk);
            }
            _ => {
                let got = ph.pop_min();
                let want = oracle.iter().next().copied();
                assert_eq!(got.map(|x| x.0), want.map(|x| x.0));
                if let Some((k, id)) = got {
                    assert_eq!(keys[id as usize], k);
                    assert!(oracle.remove(&(k, id)));
                    live.retain(|&x| x != id);
                }
            }
        }
        assert_eq!(ph.len, oracle.len());
        assert_eq!(ph.peek(), oracle.iter().next().map(|x| x.0));
    }
    println!(
        "pairing heap: 50k random insert/pop/decrease-key ops match the BTreeSet oracle ({} left)",
        ph.len
    );

    // ---- leftist heaps: 8 mergeable queues in ONE arena, random insert/pop/meld vs BinaryHeap::append
    let mut ln = LNodes::default();
    let mut roots = [NIL; 8];
    let mut orc: Vec<BinaryHeap<Reverse<u64>>> = (0..8).map(|_| BinaryHeap::new()).collect();
    let mut melds = 0;
    for _ in 0..50_000 {
        let i = g.below(8) as usize;
        match g.below(10) {
            0..=4 => {
                let k = g.below(1 << 30);
                let s = ln.singleton(k, 0);
                roots[i] = ln.meld(roots[i], s);
                orc[i].push(Reverse(k));
            }
            5..=7 => {
                let want = orc[i].pop().map(|Reverse(k)| k);
                let got = if roots[i] == NIL {
                    None
                } else {
                    let (k, _, r) = ln.pop_root(roots[i]);
                    roots[i] = r;
                    Some(k)
                };
                assert_eq!(got, want);
            }
            _ => {
                let j = g.below(8) as usize;
                if i != j {
                    roots[i] = ln.meld(roots[i], roots[j]);
                    roots[j] = NIL;
                    let mut o = std::mem::take(&mut orc[j]);
                    orc[i].append(&mut o);
                    melds += 1;
                }
            }
        }
    }
    for i in 0..8 {
        while let Some(Reverse(k)) = orc[i].pop() {
            let (got, _, r) = ln.pop_root(roots[i]);
            roots[i] = r;
            assert_eq!(got, k);
        }
        assert_eq!(roots[i], NIL);
    }
    println!("leftist heap: 50k ops incl. {melds} melds across 8 queues match BinaryHeap");
    // pairing heap meld (O(1)) in a shared arena, then drain sorted
    let mut pn = PNodes::default();
    let mut all = vec![];
    let mut rs = vec![];
    for _ in 0..40 {
        let mut r = NIL;
        for _ in 0..250 {
            let k = g.below(1 << 20);
            all.push(k);
            let h = pn.new_node(k, 0);
            r = pn.meld(r, h);
        }
        rs.push(r);
    }
    let mut root = NIL;
    for r in rs {
        root = pn.meld(root, r);
    }
    all.sort_unstable();
    for want in all {
        let (k, _, r) = pn.pop_root(root);
        root = r;
        assert_eq!(k, want);
    }
    assert_eq!(root, NIL);

    // ---- indexed d-ary heap vs oracle (push_or_decrease / pop)
    fn dtest<const D: usize>(g: &mut Rng) {
        let n = 2000;
        let mut h = DHeap::<D>::new(n);
        let mut orc: BTreeSet<(u64, u32)> = BTreeSet::new();
        let mut cur = vec![u64::MAX; n];
        for _ in 0..50_000 {
            if g.below(3) != 0 {
                let id = g.below(n as u64) as u32;
                let k = g.below(100_000);
                if k < cur[id as usize] {
                    if cur[id as usize] != u64::MAX {
                        orc.remove(&(cur[id as usize], id));
                    }
                    cur[id as usize] = k;
                    orc.insert((k, id));
                    assert!(h.push_or_decrease(id, k));
                } else {
                    assert!(!h.push_or_decrease(id, k));
                }
            } else {
                let got = h.pop_min();
                let want = orc.iter().next().copied();
                assert_eq!(got.map(|x| x.1), want.map(|x| x.0));
                if let Some((id, k)) = got {
                    assert!(orc.remove(&(k, id)));
                    cur[id as usize] = u64::MAX;
                }
            }
            assert_eq!(h.len(), orc.len());
        }
    }
    dtest::<2>(&mut g);
    dtest::<4>(&mut g);
    dtest::<8>(&mut g);
    println!("indexed d-ary heap (D = 2, 4, 8): 3 x 50k ops match the oracle");

    // ---- monotone queues vs BinaryHeap with a random monotone workload
    for c in [1u64, 10, 100] {
        let (mut dial, mut radix, mut std_q) = (Dial::new(c), Radix::new(), Std(BinaryHeap::new()));
        let mut cur = 0u64;
        for _ in 0..50_000 {
            if g.below(5) < 3 {
                let k = cur + g.below(c + 1);
                dial.push(k, 0);
                radix.push(k, 0);
                std_q.push(k, 0);
            } else {
                let (a, b, s) = (dial.pop(), radix.pop(), std_q.pop());
                assert_eq!(a.map(|x| x.0), s.map(|x| x.0));
                assert_eq!(b.map(|x| x.0), s.map(|x| x.0));
                if let Some((k, _)) = s {
                    cur = k;
                }
            }
        }
    }
    println!("Dial and radix heap: monotone workloads (C = 1, 10, 100) match BinaryHeap");

    // ---- Dijkstra with every queue must give identical distances
    for t in 0..30u64 {
        let n = 200 + (t as usize * 97) % 1500;
        let maxw = [1u32, 5, 20, 100, 1000][t as usize % 5];
        let gr = random_graph(n, n * 4, maxw, 1000 + t);
        let want = dijkstra_lazy(&gr, 0, Std(BinaryHeap::new()));
        assert_eq!(dijkstra_lazy(&gr, 0, Dial::new(maxw as u64)), want, "Dial");
        assert_eq!(dijkstra_lazy(&gr, 0, Radix::new()), want, "radix");
        assert_eq!(dijkstra_dary::<2>(&gr, 0), want, "2-ary");
        assert_eq!(dijkstra_dary::<4>(&gr, 0), want, "4-ary");
        assert_eq!(dijkstra_dary::<8>(&gr, 0), want, "8-ary");
        assert_eq!(dijkstra_pairing(&gr, 0), want, "pairing");
    }
    let big = random_graph(200_000, 1_000_000, 100, 7);
    let timed = |name: &str, f: &dyn Fn() -> Vec<u64>| {
        let t = Instant::now();
        let d = f();
        println!(
            "  dijkstra {name:<14} {:>7.1} ms",
            t.elapsed().as_secs_f64() * 1e3
        );
        d
    };
    println!("Dijkstra on 200k nodes / 1.2M edges, weights 1..=100 (printed timings only):");
    let want = timed("BinaryHeap", &|| {
        dijkstra_lazy(&big, 0, Std(BinaryHeap::new()))
    });
    assert_eq!(
        timed("Dial (C=100)", &|| dijkstra_lazy(
            &big,
            0,
            Dial::new(big.maxw as u64)
        )),
        want
    );
    assert_eq!(
        timed("radix heap", &|| dijkstra_lazy(&big, 0, Radix::new())),
        want
    );
    assert_eq!(
        timed("4-ary indexed", &|| dijkstra_dary::<4>(&big, 0)),
        want
    );
    assert_eq!(
        timed("2-ary indexed", &|| dijkstra_dary::<2>(&big, 0)),
        want
    );
    assert_eq!(timed("pairing heap", &|| dijkstra_pairing(&big, 0)), want);

    // ---- benchmark: 1M inserts then 1M pops of random keys (printed only)
    println!("1M inserts + 1M pops, random keys (printed timings only):");
    let keys: Vec<u64> = (0..1_000_000).map(|_| g.below(1 << 40)).collect();
    let bench = |name: &str, f: &dyn Fn() -> u64| {
        let t = Instant::now();
        let s = f();
        println!(
            "  {name:<14} {:>7.1} ms (checksum {s})",
            t.elapsed().as_secs_f64() * 1e3
        );
        s
    };
    let a = bench("BinaryHeap", &|| {
        let mut h = BinaryHeap::new();
        for &k in &keys {
            h.push(Reverse(k));
        }
        let mut s = 0u64;
        while let Some(Reverse(k)) = h.pop() {
            s = s.wrapping_mul(31).wrapping_add(k);
        }
        s
    });
    let b = bench("4-ary indexed", &|| {
        let mut h = DHeap::<4>::new(keys.len());
        for (i, &k) in keys.iter().enumerate() {
            h.push_or_decrease(i as u32, k);
        }
        let mut s = 0u64;
        while let Some((_, k)) = h.pop_min() {
            s = s.wrapping_mul(31).wrapping_add(k);
        }
        s
    });
    let c = bench("pairing heap", &|| {
        let mut h = PairingHeap::new();
        for (i, &k) in keys.iter().enumerate() {
            h.insert(k, i as u32);
        }
        let mut s = 0u64;
        while let Some((k, _)) = h.pop_min() {
            s = s.wrapping_mul(31).wrapping_add(k);
        }
        s
    });
    let d = bench("leftist heap", &|| {
        let mut h = LNodes::default();
        let mut r = NIL;
        for (i, &k) in keys.iter().enumerate() {
            let n = h.singleton(k, i as u32);
            r = h.meld(r, n);
        }
        let mut s = 0u64;
        while r != NIL {
            let (k, _, nr) = h.pop_root(r);
            r = nr;
            s = s.wrapping_mul(31).wrapping_add(k);
        }
        s
    });
    let e = bench("radix (mono.)", &|| {
        let mut sorted = keys.clone();
        sorted.sort_unstable();
        let mut q = Radix::new();
        let mut s = 0u64;
        for &k in &sorted {
            q.push(k, 0);
        }
        while let Some((k, _)) = q.pop() {
            s = s.wrapping_mul(31).wrapping_add(k);
        }
        s
    });
    assert!(
        a == b && b == c && c == d && d == e,
        "all queues must drain the same sorted sequence"
    );
    println!("all assertions passed");
}
