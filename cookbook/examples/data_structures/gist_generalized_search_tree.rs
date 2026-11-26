//! RECIPE 714 (trees): GiST, the Generalized Search Tree: ONE balanced tree, two index types (B-tree-like ranges and an R-tree).
//! LEVEL: advanced
//! Use in: extensible databases (new index types without new tree code), spatial/range/text indexes, nearest-neighbour search.   Run: cargo run --release --example gist_generalized_search_tree
#![allow(dead_code)]
//!
//! WHAT: a height-balanced tree whose shape logic (descend, split, grow the root, condense on delete, best-first kNN) is written
//! ONCE, and whose *semantics* come from a small trait, the GiST "extension methods":
//!   consistent(key, query, is_leaf) : may this subtree contain a match?  (must never give a false negative; false positives are fine)
//!   union(keys)                     : the smallest key that covers all of them (the parent's "bounding predicate")
//!   penalty(old, new)               : how bad is it to insert `new` below the entry keyed `old`?  (drives the descent)
//!   pick_split(keys)                : partition an overflowing node's keys into two groups
//!   same(a, b) / contains(outer, inner) : equality and coverage (needed to find an entry again on delete)
//!   min_dist(key, target)           : a LOWER BOUND on the distance to anything under `key` (drives nearest-neighbour search)
//! WHY: B-trees, R-trees, and trees over text or ranges all share the same skeleton. PostgreSQL ships exactly this idea as the
//! GiST access method, so extension authors (PostGIS, pg_trgm, btree_gist) only supply the methods above.
//! HOW: the tree is an arena of nodes (`Vec`, usize ids). An internal entry is (key = union of the child's keys, child id); a leaf
//! entry is (key, record id). Insert descends along the minimum penalty, pushes into a leaf, then walks back up splitting
//! overflowing nodes and refreshing the parent keys. Delete finds the entry through `contains`, removes it, DISSOLVES any node that
//! falls below the minimum fill and REINSERTS its orphaned records (the R-tree "condense tree" idea), then shrinks the root.
//! kNN is best-first: a min-heap of (distance lower bound, node or record); a record popped from the heap is provably the next
//! nearest because every unexpanded node has a lower bound at least as large.
//! Instances: `Ranges` (keys are integer intervals, points are degenerate intervals: a B-tree-like ordered index with range
//! search) and `Rects` (2-D rectangles: an R-tree with overlap / within queries). The `Tree<G>` code is identical for both.
//! Verified by two 5k-operation seeded differential tests (insert / delete / search / kNN) against brute-force oracles, plus an
//! invariant checker run along the way: uniform leaf depth, fill bounds, each parent key equals the union of its child's keys.
//! COMMON USES: PostgreSQL's GiST (PostGIS geometry indexes, range types, trigram and full-text indexes) is this framework;
//! R-tree variants index spatial data in many databases and GIS tools.
//! Course: CMU 15-721 (Advanced Database Systems) index/access-method lectures; CMU 15-445 tree-index lectures for the B+tree baseline.
//! Read: Hellerstein, Naughton, Pfeffer, "Generalized Search Trees for Database Systems": https://dsf.berkeley.edu/papers/vldb95-gist.pdf
//! Read: PostgreSQL manual, GiST indexes: https://www.postgresql.org/docs/current/gist.html
use std::cmp::Reverse;
use std::collections::BinaryHeap;
use std::fmt::Debug;

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

/// The extension interface. Everything index-specific lives here; nothing else knows what a key means.
trait Gist {
    type Key: Clone + Debug;
    type Query;
    type Target;
    fn consistent(k: &Self::Key, q: &Self::Query, leaf: bool) -> bool;
    fn union(keys: &[Self::Key]) -> Self::Key;
    fn penalty(old: &Self::Key, new: &Self::Key) -> f64;
    /// Returns two index lists that together cover 0..keys.len() with both sides non-empty.
    fn pick_split(keys: &[Self::Key]) -> (Vec<usize>, Vec<usize>);
    fn same(a: &Self::Key, b: &Self::Key) -> bool;
    fn contains(outer: &Self::Key, inner: &Self::Key) -> bool;
    fn min_dist(k: &Self::Key, t: &Self::Target) -> f64;
}

/// `ptr` is a child node id in internal nodes and a record id in leaves (one shape keeps the code short).
struct Entry<K> {
    key: K,
    ptr: u64,
}

struct Node<K> {
    leaf: bool,
    entries: Vec<Entry<K>>,
}

struct Tree<G: Gist> {
    nodes: Vec<Node<G::Key>>,
    free: Vec<usize>,
    root: usize,
    max: usize,
    min: usize,
    len: usize,
}

impl<G: Gist> Tree<G> {
    fn new(max: usize) -> Self {
        // min = max/3 so that both halves of a split (each >= max/2) satisfy the fill bound.
        Tree {
            nodes: vec![Node {
                leaf: true,
                entries: vec![],
            }],
            free: vec![],
            root: 0,
            max,
            min: max / 3,
            len: 0,
        }
    }

    fn alloc(&mut self, node: Node<G::Key>) -> usize {
        if let Some(i) = self.free.pop() {
            self.nodes[i] = node;
            i
        } else {
            self.nodes.push(node);
            self.nodes.len() - 1
        }
    }

    fn node_key(&self, n: usize) -> G::Key {
        let keys: Vec<G::Key> = self.nodes[n]
            .entries
            .iter()
            .map(|e| e.key.clone())
            .collect();
        G::union(&keys)
    }

    /// Insertion descent: at every internal node follow the cheapest entry (first one wins ties, for determinism).
    fn choose(&self, n: usize, key: &G::Key) -> usize {
        let mut best = (f64::INFINITY, 0);
        for (i, e) in self.nodes[n].entries.iter().enumerate() {
            let p = G::penalty(&e.key, key);
            if p < best.0 {
                best = (p, i);
            }
        }
        best.1
    }

    fn insert(&mut self, key: G::Key, id: u64) {
        let mut path = vec![]; // (internal node, index of the entry we descended through)
        let mut n = self.root;
        while !self.nodes[n].leaf {
            let i = self.choose(n, &key);
            path.push((n, i));
            n = self.nodes[n].entries[i].ptr as usize;
        }
        self.nodes[n].entries.push(Entry { key, ptr: id });
        self.len += 1;
        self.fix_upwards(n, path);
    }

    /// Walk back to the root: split overflowing nodes, refresh the key of the entry we came through, add the new sibling.
    fn fix_upwards(&mut self, mut cur: usize, mut path: Vec<(usize, usize)>) {
        loop {
            let sibling = if self.nodes[cur].entries.len() > self.max {
                Some(self.split(cur))
            } else {
                None
            };
            match path.pop() {
                None => {
                    if let Some(s) = sibling {
                        // The root split: the tree grows one level, the only way height ever increases.
                        let entries = vec![
                            Entry {
                                key: self.node_key(cur),
                                ptr: cur as u64,
                            },
                            Entry {
                                key: self.node_key(s),
                                ptr: s as u64,
                            },
                        ];
                        self.root = self.alloc(Node {
                            leaf: false,
                            entries,
                        });
                    }
                    return;
                }
                Some((p, i)) => {
                    self.nodes[p].entries[i].key = self.node_key(cur);
                    if let Some(s) = sibling {
                        let key = self.node_key(s);
                        self.nodes[p].entries.push(Entry { key, ptr: s as u64 });
                    }
                    cur = p;
                }
            }
        }
    }

    fn split(&mut self, n: usize) -> usize {
        let leaf = self.nodes[n].leaf;
        let old = std::mem::take(&mut self.nodes[n].entries);
        let keys: Vec<G::Key> = old.iter().map(|e| e.key.clone()).collect();
        let (a, b) = G::pick_split(&keys);
        assert!(
            a.len() >= self.min && b.len() >= self.min,
            "pick_split must respect the minimum fill"
        );
        let mut slots: Vec<Option<Entry<G::Key>>> = old.into_iter().map(Some).collect();
        let mut take = |idx: &[usize]| -> Vec<Entry<G::Key>> {
            idx.iter()
                .map(|&i| slots[i].take().expect("index used twice"))
                .collect()
        };
        let (ea, eb) = (take(&a), take(&b));
        self.nodes[n].entries = ea;
        self.alloc(Node { leaf, entries: eb })
    }

    /// Returns (matching record ids, nodes visited).
    fn search(&self, q: &G::Query) -> (Vec<u64>, usize) {
        let (mut out, mut visited, mut stack) = (vec![], 0, vec![self.root]);
        while let Some(n) = stack.pop() {
            visited += 1;
            let node = &self.nodes[n];
            for e in &node.entries {
                if G::consistent(&e.key, q, node.leaf) {
                    if node.leaf {
                        out.push(e.ptr);
                    } else {
                        stack.push(e.ptr as usize);
                    }
                }
            }
        }
        (out, visited)
    }

    /// Best-first kNN. Heap items are (distance bits, sequence, is_record, ptr); for non-negative f64 the raw bits order like the value.
    fn nearest(&self, t: &G::Target, k: usize) -> Vec<(f64, u64)> {
        let mut heap = BinaryHeap::new();
        let mut seq = 0u64;
        heap.push(Reverse((0f64.to_bits(), seq, false, self.root as u64)));
        let mut out = vec![];
        while let Some(Reverse((bits, _, is_rec, ptr))) = heap.pop() {
            if is_rec {
                out.push((f64::from_bits(bits), ptr));
                if out.len() == k {
                    break;
                }
                continue;
            }
            let node = &self.nodes[ptr as usize];
            for e in &node.entries {
                seq += 1;
                heap.push(Reverse((
                    G::min_dist(&e.key, t).to_bits(),
                    seq,
                    node.leaf,
                    e.ptr,
                )));
            }
        }
        out
    }

    fn delete(&mut self, key: &G::Key, id: u64) -> bool {
        let mut orphans = vec![];
        if !self.delete_rec(self.root, key, id, &mut orphans) {
            return false;
        }
        self.len -= 1 + orphans.len(); // orphans are re-counted when reinserted
        // Root cleanup: an internal root with one child is redundant; with none, the tree is empty.
        while !self.nodes[self.root].leaf && self.nodes[self.root].entries.len() <= 1 {
            let old = self.root;
            match self.nodes[old].entries.pop() {
                Some(e) => self.root = e.ptr as usize,
                None => self.nodes[old].leaf = true,
            }
            if self.nodes[old].leaf {
                break;
            }
            self.free.push(old);
        }
        for (k, i) in orphans {
            self.insert(k, i);
        }
        true
    }

    fn delete_rec(
        &mut self,
        n: usize,
        key: &G::Key,
        id: u64,
        orphans: &mut Vec<(G::Key, u64)>,
    ) -> bool {
        if self.nodes[n].leaf {
            let pos = self.nodes[n]
                .entries
                .iter()
                .position(|e| e.ptr == id && G::same(&e.key, key));
            return match pos {
                Some(p) => {
                    self.nodes[n].entries.remove(p);
                    true
                }
                None => false,
            };
        }
        for i in 0..self.nodes[n].entries.len() {
            if !G::contains(&self.nodes[n].entries[i].key, key) {
                continue; // an entry's key covers everything below it, so a non-covering subtree cannot hold the record
            }
            let c = self.nodes[n].entries[i].ptr as usize;
            if self.delete_rec(c, key, id, orphans) {
                if self.nodes[c].entries.len() < self.min {
                    self.dissolve(c, orphans);
                    self.nodes[n].entries.remove(i);
                } else {
                    self.nodes[n].entries[i].key = self.node_key(c);
                }
                return true;
            }
        }
        false
    }

    /// Free a whole subtree, handing its records back for reinsertion.
    fn dissolve(&mut self, n: usize, orphans: &mut Vec<(G::Key, u64)>) {
        let entries = std::mem::take(&mut self.nodes[n].entries);
        for e in entries {
            if self.nodes[n].leaf {
                orphans.push((e.key, e.ptr));
            } else {
                self.dissolve(e.ptr as usize, orphans);
            }
        }
        self.free.push(n);
    }

    /// Returns (height, key of the subtree) and panics on any violated invariant.
    fn check(&self) {
        let (_, count) = self.check_node(self.root, true);
        assert_eq!(count, self.len, "record count mismatch");
    }

    fn check_node(&self, n: usize, is_root: bool) -> (usize, usize) {
        let node = &self.nodes[n];
        assert!(node.entries.len() <= self.max, "overflow");
        if !is_root {
            assert!(
                node.entries.len() >= self.min,
                "underflow ({} < {})",
                node.entries.len(),
                self.min
            );
        }
        if node.leaf {
            return (1, node.entries.len());
        }
        assert!(node.entries.len() >= 2, "internal node with < 2 children");
        let (mut height, mut count) = (0, 0);
        for e in &node.entries {
            let c = e.ptr as usize;
            let (h, k) = self.check_node(c, false);
            assert!(height == 0 || height == h, "leaves at different depths");
            height = h;
            count += k;
            assert!(
                G::same(&e.key, &self.node_key(c)),
                "parent key is not the union of the child's keys"
            );
        }
        (height + 1, count)
    }

    fn height(&self) -> usize {
        let (mut h, mut n) = (1, self.root);
        while !self.nodes[n].leaf {
            n = self.nodes[n].entries[0].ptr as usize;
            h += 1;
        }
        h
    }

    fn live_nodes(&self) -> usize {
        self.nodes.len() - self.free.len()
    }
}

// ---------------------------------------------------------------------------------------------------------------------
// Instance 1: B-tree-like index over integer ranges. Records are points (lo == hi); internal keys are covering intervals.
// ---------------------------------------------------------------------------------------------------------------------
#[derive(Clone, Debug, PartialEq)]
struct Span {
    lo: i64,
    hi: i64,
}

struct Ranges;
impl Gist for Ranges {
    type Key = Span;
    type Query = (i64, i64); // inclusive range
    type Target = i64;
    fn consistent(k: &Span, q: &(i64, i64), _leaf: bool) -> bool {
        !(k.hi < q.0 || k.lo > q.1) // interval overlap; for a point key this is "inside the range"
    }
    fn union(keys: &[Span]) -> Span {
        Span {
            lo: keys.iter().map(|k| k.lo).min().unwrap(),
            hi: keys.iter().map(|k| k.hi).max().unwrap(),
        }
    }
    fn penalty(old: &Span, new: &Span) -> f64 {
        // How much the interval must widen. Zero when `new` already lies inside, which is the B-tree's "go to the right child".
        let u = Self::union(&[old.clone(), new.clone()]);
        ((u.hi - u.lo) - (old.hi - old.lo)) as f64
    }
    fn pick_split(keys: &[Span]) -> (Vec<usize>, Vec<usize>) {
        // Sorting by lower bound and cutting in the middle is exactly the B-tree split.
        let mut idx: Vec<usize> = (0..keys.len()).collect();
        idx.sort_by_key(|&i| (keys[i].lo, keys[i].hi));
        let b = idx.split_off(idx.len() / 2);
        (idx, b)
    }
    fn same(a: &Span, b: &Span) -> bool {
        a == b
    }
    fn contains(o: &Span, i: &Span) -> bool {
        o.lo <= i.lo && i.hi <= o.hi
    }
    fn min_dist(k: &Span, t: &i64) -> f64 {
        if *t < k.lo {
            (k.lo - t) as f64
        } else if *t > k.hi {
            (t - k.hi) as f64
        } else {
            0.0
        }
    }
}

// ---------------------------------------------------------------------------------------------------------------------
// Instance 2: R-tree over 2-D rectangles (integer-valued coordinates stored as f64 so equality and sums are exact).
// ---------------------------------------------------------------------------------------------------------------------
#[derive(Clone, Debug, PartialEq)]
struct Rect {
    x0: f64,
    y0: f64,
    x1: f64,
    y1: f64,
}
impl Rect {
    fn area(&self) -> f64 {
        (self.x1 - self.x0) * (self.y1 - self.y0)
    }
    fn overlaps(&self, o: &Rect) -> bool {
        self.x0 <= o.x1 && o.x0 <= self.x1 && self.y0 <= o.y1 && o.y0 <= self.y1
    }
    fn inside(&self, o: &Rect) -> bool {
        o.x0 <= self.x0 && self.x1 <= o.x1 && o.y0 <= self.y0 && self.y1 <= o.y1
    }
}

struct RectQuery {
    area: Rect,
    within: bool, // false: overlap query; true: report only rectangles fully inside `area`
}

struct Rects;
impl Gist for Rects {
    type Key = Rect;
    type Query = RectQuery;
    type Target = (f64, f64);
    fn consistent(k: &Rect, q: &RectQuery, leaf: bool) -> bool {
        // Internal levels always use overlap: a child can hold a "within" match whenever its bounding box merely overlaps.
        if leaf && q.within {
            k.inside(&q.area)
        } else {
            k.overlaps(&q.area)
        }
    }
    fn union(keys: &[Rect]) -> Rect {
        let f = |g: fn(&Rect) -> f64, m: fn(f64, f64) -> f64| keys.iter().map(g).reduce(m).unwrap();
        Rect {
            x0: f(|r| r.x0, f64::min),
            y0: f(|r| r.y0, f64::min),
            x1: f(|r| r.x1, f64::max),
            y1: f(|r| r.y1, f64::max),
        }
    }
    fn penalty(old: &Rect, new: &Rect) -> f64 {
        // Guttman's least-enlargement rule, with a tiny tie-break towards the smaller box.
        let u = Self::union(&[old.clone(), new.clone()]);
        (u.area() - old.area()) + 1e-9 * old.area()
    }
    fn pick_split(keys: &[Rect]) -> (Vec<usize>, Vec<usize>) {
        // Simple and decent: sort by centre along the axis with the larger spread, cut in the middle.
        let cx = |r: &Rect| r.x0 + r.x1;
        let cy = |r: &Rect| r.y0 + r.y1;
        let spread = |c: &dyn Fn(&Rect) -> f64| {
            let v: Vec<f64> = keys.iter().map(c).collect();
            v.iter().cloned().fold(f64::MIN, f64::max) - v.iter().cloned().fold(f64::MAX, f64::min)
        };
        let use_x = spread(&cx) >= spread(&cy);
        let mut idx: Vec<usize> = (0..keys.len()).collect();
        idx.sort_by(|&a, &b| {
            let (ka, kb) = if use_x {
                (cx(&keys[a]), cx(&keys[b]))
            } else {
                (cy(&keys[a]), cy(&keys[b]))
            };
            ka.partial_cmp(&kb).unwrap().then(a.cmp(&b))
        });
        let b = idx.split_off(idx.len() / 2);
        (idx, b)
    }
    fn same(a: &Rect, b: &Rect) -> bool {
        a == b
    }
    fn contains(o: &Rect, i: &Rect) -> bool {
        i.inside(o)
    }
    fn min_dist(k: &Rect, t: &(f64, f64)) -> f64 {
        let dx = (k.x0 - t.0).max(0.0).max(t.0 - k.x1);
        let dy = (k.y0 - t.1).max(0.0).max(t.1 - k.y1);
        (dx * dx + dy * dy).sqrt()
    }
}

// ---------------------------------------------------------------------------------------------------------------------
// Generic differential test: the SAME driver runs both instances against brute-force oracles.
// ---------------------------------------------------------------------------------------------------------------------
struct Workload<G: Gist> {
    name: &'static str,
    gen_key: fn(&mut Rng) -> G::Key,
    gen_query: fn(&mut Rng) -> G::Query,
    gen_target: fn(&mut Rng) -> G::Target,
    oracle_match: fn(&G::Key, &G::Query) -> bool,
    oracle_dist: fn(&G::Key, &G::Target) -> f64,
}

fn run<G: Gist>(w: Workload<G>, max: usize, ops: usize, seed: u64) {
    let mut rng = Rng(seed);
    let mut tree: Tree<G> = Tree::new(max);
    let mut live: Vec<(G::Key, u64)> = vec![];
    let (mut next_id, mut searches, mut visited_sum, mut knn_checks, mut max_h) =
        (0u64, 0usize, 0usize, 0usize, 1);
    for step in 0..ops {
        let r = rng.below(100);
        if r < 55 || live.is_empty() {
            let k = (w.gen_key)(&mut rng);
            tree.insert(k.clone(), next_id);
            live.push((k, next_id));
            next_id += 1;
        } else if r < 80 {
            let (k, id) = live.swap_remove(rng.below(live.len() as u64) as usize);
            assert!(
                tree.delete(&k, id),
                "{}: live record not found on delete",
                w.name
            );
            assert!(!tree.delete(&k, id), "{}: double delete must fail", w.name);
        } else if r < 92 {
            let q = (w.gen_query)(&mut rng);
            let (mut got, visited) = tree.search(&q);
            let mut want: Vec<u64> = live
                .iter()
                .filter(|(k, _)| (w.oracle_match)(k, &q))
                .map(|(_, i)| *i)
                .collect();
            got.sort();
            want.sort();
            assert_eq!(got, want, "{}: search mismatch at step {step}", w.name);
            searches += 1;
            visited_sum += visited * 1000 / tree.live_nodes().max(1);
        } else {
            let t = (w.gen_target)(&mut rng);
            let k = 1 + rng.below(8) as usize;
            let got: Vec<f64> = tree.nearest(&t, k).iter().map(|x| x.0).collect();
            let mut want: Vec<f64> = live
                .iter()
                .map(|(key, _)| (w.oracle_dist)(key, &t))
                .collect();
            want.sort_by(|a, b| a.partial_cmp(b).unwrap());
            want.truncate(k);
            assert_eq!(got.len(), want.len(), "{}: kNN count", w.name);
            for (a, b) in got.iter().zip(&want) {
                assert!(
                    (a - b).abs() < 1e-9,
                    "{}: kNN distance mismatch {got:?} vs {want:?}",
                    w.name
                );
            }
            knn_checks += 1;
        }
        max_h = max_h.max(tree.height());
        if step % 50 == 0 {
            tree.check();
        }
    }
    tree.check();
    assert_eq!(tree.len, live.len());
    // Drain everything: exercises condense/reinsert and root shrinking all the way to an empty tree.
    for (k, id) in live.drain(..) {
        assert!(tree.delete(&k, id));
    }
    tree.check();
    assert_eq!(tree.len, 0);
    assert_eq!(tree.height(), 1);
    println!(
        "{:<8} ops={ops} max_height={max_h} searches={searches} kNN={knn_checks} avg nodes visited per search={:.1}% of tree",
        w.name,
        visited_sum as f64 / 10.0 / searches.max(1) as f64
    );
}

fn main() {
    println!("-- one Tree<G>, two GiST instances --");
    run::<Ranges>(
        Workload {
            name: "ranges",
            gen_key: |r| {
                let v = r.below(20_000) as i64;
                Span { lo: v, hi: v }
            },
            gen_query: |r| {
                let a = r.below(20_000) as i64;
                (a, a + r.below(1500) as i64)
            },
            gen_target: |r| r.below(20_000) as i64,
            oracle_match: |k, q| k.lo >= q.0 && k.hi <= q.1,
            oracle_dist: |k, t| (k.lo - t).abs() as f64,
        },
        8,
        5000,
        0x9157_0001,
    );
    run::<Rects>(
        Workload {
            name: "rects",
            gen_key: |r| {
                let (x, y) = (r.below(1000) as f64, r.below(1000) as f64);
                Rect {
                    x0: x,
                    y0: y,
                    x1: x + r.below(30) as f64,
                    y1: y + r.below(30) as f64,
                }
            },
            gen_query: |r| {
                let (x, y) = (r.below(1000) as f64, r.below(1000) as f64);
                let area = Rect {
                    x0: x,
                    y0: y,
                    x1: x + r.below(200) as f64,
                    y1: y + r.below(200) as f64,
                };
                RectQuery {
                    area,
                    within: r.below(2) == 0,
                }
            },
            gen_target: |r| (r.below(1000) as f64, r.below(1000) as f64),
            oracle_match: |k, q| {
                if q.within {
                    k.inside(&q.area)
                } else {
                    k.overlaps(&q.area)
                }
            },
            oracle_dist: |k, t| {
                let dx = if t.0 < k.x0 {
                    k.x0 - t.0
                } else if t.0 > k.x1 {
                    t.0 - k.x1
                } else {
                    0.0
                };
                let dy = if t.1 < k.y0 {
                    k.y0 - t.1
                } else if t.1 > k.y1 {
                    t.1 - k.y1
                } else {
                    0.0
                };
                (dx * dx + dy * dy).sqrt()
            },
        },
        10,
        5000,
        0x9157_0002,
    );

    // A direct check of the lower-bound property: kNN must touch far fewer nodes than the whole tree.
    let mut rng = Rng(77);
    let mut t: Tree<Rects> = Tree::new(10);
    for i in 0..4000u64 {
        let (x, y) = (rng.below(2000) as f64, rng.below(2000) as f64);
        t.insert(
            Rect {
                x0: x,
                y0: y,
                x1: x + 3.0,
                y1: y + 3.0,
            },
            i,
        );
    }
    let (_, visited) = t.search(&RectQuery {
        area: Rect {
            x0: 100.0,
            y0: 100.0,
            x1: 160.0,
            y1: 160.0,
        },
        within: false,
    });
    assert!(
        visited * 5 < t.live_nodes(),
        "a small window query should prune most of the tree"
    );
    println!(
        "pruning: small window visited {visited} of {} nodes (height {})",
        t.live_nodes(),
        t.height()
    );
    println!("all GiST checks passed");
}
