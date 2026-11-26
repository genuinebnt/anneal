//! RECIPE 130 (trees): an in-memory B+ tree map with leaf chain, split, borrow and merge, arena-based
//! LEVEL: advanced
//! Use in: database indexes, ordered key-value stores, anything needing fast point lookups AND range scans.   Run: cargo run --release --example bplus_tree
#![allow(dead_code)]
//!
//! WHAT: `BPlusMap<K, V>` where internal nodes hold only separator keys and child ids, while leaves hold the key/value pairs and are
//! linked left-to-right. `order` = max keys per node (M >= 3); every non-root node keeps >= M/2 keys.
//! WHY databases use it: a node is sized to ONE disk page (4-16 KiB), so fan-out is in the hundreds; a billion rows need only 3-4
//! page reads per lookup, and the top levels stay cached. Internal nodes carry no values, so they pack even more keys per page.
//! Range scans find the first key once, then walk the leaf chain sequentially (cheap, prefetch-friendly) instead of re-descending.
//! HOW: nodes live in a `Vec` addressed by `usize` (arena_ids), freed slots recycled. Separator rule: keys in kids[i] < sep[i] <= keys in
//! kids[i+1]. Insert splits an overflowing node (leaf: copy up the right half's first key; internal: push the middle key up) and the
//! root grows when it splits. Delete fixes an underflowing child by BORROWING from a sibling with spare keys, else MERGING with one
//! (full borrow/merge, not lazy underflow); the root shrinks when it is left with one child.
//! Verified by `check_invariants()` after EVERY operation of a 20k-op seeded differential test against `BTreeMap`.
//! COMMON USES: relational database indexes (InnoDB clustered indexes, PostgreSQL btree indexes and SQLite tables are B-tree
//! family structures; the B+ variant keeps data in leaves); file-system directories and metadata; ordered storage engines.
//! Read: https://en.wikipedia.org/wiki/B%2B_tree
use std::collections::BTreeMap;
use std::ops::{Bound, RangeBounds};

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

/// One shape for both kinds keeps borrow handling simple: leaves use `vals`/`next`, internals use `kids`.
struct Node<K, V> {
    leaf: bool,
    keys: Vec<K>,
    vals: Vec<V>,
    kids: Vec<usize>,
    next: Option<usize>,
}

impl<K, V> Default for Node<K, V> {
    fn default() -> Self {
        Node {
            leaf: true,
            keys: vec![],
            vals: vec![],
            kids: vec![],
            next: None,
        }
    }
}

struct BPlusMap<K, V> {
    nodes: Vec<Node<K, V>>,
    free: Vec<usize>,
    root: usize,
    order: usize,
    len: usize,
    height: usize,
}

#[derive(Debug)]
struct Stats {
    height: usize,
    leaves: usize,
    internals: usize,
    entries: usize,
    fill: f64,
}

impl<K: Ord + Clone, V> BPlusMap<K, V> {
    fn new(order: usize) -> Self {
        assert!(order >= 3);
        BPlusMap {
            nodes: vec![Node::default()],
            free: vec![],
            root: 0,
            order,
            len: 0,
            height: 1,
        }
    }
    fn min_keys(&self) -> usize {
        self.order / 2
    }
    fn alloc(&mut self, n: Node<K, V>) -> usize {
        if let Some(i) = self.free.pop() {
            self.nodes[i] = n;
            i
        } else {
            self.nodes.push(n);
            self.nodes.len() - 1
        }
    }
    fn release(&mut self, id: usize) {
        self.nodes[id] = Node::default();
        self.free.push(id);
    }
    fn take(&mut self, id: usize) -> Node<K, V> {
        std::mem::take(&mut self.nodes[id])
    }
    /// Which child covers `k`: equal keys go right, matching "sep <= keys in the right child".
    fn child_index(&self, id: usize, k: &K) -> usize {
        self.nodes[id].keys.partition_point(|s| s <= k)
    }
    fn find_leaf(&self, k: &K) -> usize {
        let mut id = self.root;
        while !self.nodes[id].leaf {
            id = self.nodes[id].kids[self.child_index(id, k)];
        }
        id
    }
    fn leftmost_leaf(&self) -> usize {
        let mut id = self.root;
        while !self.nodes[id].leaf {
            id = self.nodes[id].kids[0];
        }
        id
    }

    fn get(&self, k: &K) -> Option<&V> {
        let n = &self.nodes[self.find_leaf(k)];
        n.keys.binary_search(k).ok().map(|i| &n.vals[i])
    }

    // ------------------------------------------------------------- insert
    fn insert(&mut self, k: K, v: V) -> Option<V> {
        let (old, split) = self.insert_rec(self.root, k, v);
        if let Some((sep, right)) = split {
            let new_root = Node {
                leaf: false,
                keys: vec![sep],
                vals: vec![],
                kids: vec![self.root, right],
                next: None,
            };
            self.root = self.alloc(new_root);
            self.height += 1; // the ONLY way a B+ tree gets taller: the root splits
        }
        if old.is_none() {
            self.len += 1;
        }
        old
    }

    /// Returns (replaced value, Some((separator, new right sibling)) if this node split).
    fn insert_rec(&mut self, id: usize, k: K, v: V) -> (Option<V>, Option<(K, usize)>) {
        let m = self.order;
        if self.nodes[id].leaf {
            let n = &mut self.nodes[id];
            match n.keys.binary_search(&k) {
                Ok(i) => return (Some(std::mem::replace(&mut n.vals[i], v)), None),
                Err(i) => {
                    n.keys.insert(i, k);
                    n.vals.insert(i, v);
                }
            }
            if n.keys.len() <= m {
                return (None, None);
            }
            let mid = (m + 1) / 2;
            let right = Node {
                leaf: true,
                keys: n.keys.split_off(mid),
                vals: n.vals.split_off(mid),
                kids: vec![],
                next: n.next,
            };
            let sep = right.keys[0].clone(); // leaf split COPIES the key up: it stays in the leaf too
            let rid = self.alloc(right);
            self.nodes[id].next = Some(rid);
            return (None, Some((sep, rid)));
        }
        let i = self.child_index(id, &k);
        let child = self.nodes[id].kids[i];
        let (old, split) = self.insert_rec(child, k, v);
        let Some((sep, rid)) = split else {
            return (old, None);
        };
        let n = &mut self.nodes[id];
        n.keys.insert(i, sep);
        n.kids.insert(i + 1, rid);
        if n.keys.len() <= m {
            return (old, None);
        }
        let mid = (m + 1) / 2;
        let right_keys = n.keys.split_off(mid + 1);
        let up = n.keys.pop().unwrap(); // internal split MOVES the middle key up
        let right = Node {
            leaf: false,
            keys: right_keys,
            vals: vec![],
            kids: n.kids.split_off(mid + 1),
            next: None,
        };
        let rid = self.alloc(right);
        (old, Some((up, rid)))
    }

    // ------------------------------------------------------------- delete
    fn remove(&mut self, k: &K) -> Option<V> {
        let old = self.remove_rec(self.root, k)?;
        self.len -= 1;
        let r = &self.nodes[self.root];
        if !r.leaf && r.keys.is_empty() {
            let only = r.kids[0];
            let old_root = self.root;
            self.root = only;
            self.release(old_root);
            self.height -= 1;
        }
        Some(old)
    }

    fn remove_rec(&mut self, id: usize, k: &K) -> Option<V> {
        if self.nodes[id].leaf {
            let n = &mut self.nodes[id];
            let i = n.keys.binary_search(k).ok()?;
            n.keys.remove(i);
            return Some(n.vals.remove(i));
        }
        let i = self.child_index(id, k);
        let child = self.nodes[id].kids[i];
        let old = self.remove_rec(child, k)?;
        if self.nodes[child].keys.len() < self.min_keys() {
            self.fix_child(id, i);
        }
        Some(old)
    }

    fn fix_child(&mut self, p: usize, i: usize) {
        let min = self.min_keys();
        let kids = self.nodes[p].kids.clone();
        let spare = |s: &Self, j: usize| s.nodes[kids[j]].keys.len() > min;
        if i > 0 && spare(self, i - 1) {
            self.borrow_from_left(p, i);
        } else if i + 1 < kids.len() && spare(self, i + 1) {
            self.borrow_from_right(p, i);
        } else if i > 0 {
            self.merge(p, i - 1);
        } else {
            self.merge(p, i);
        }
    }

    fn borrow_from_left(&mut self, p: usize, i: usize) {
        let (lid, cid) = (self.nodes[p].kids[i - 1], self.nodes[p].kids[i]);
        let (mut par, mut l, mut c) = (self.take(p), self.take(lid), self.take(cid));
        if c.leaf {
            c.keys.insert(0, l.keys.pop().unwrap());
            c.vals.insert(0, l.vals.pop().unwrap());
            par.keys[i - 1] = c.keys[0].clone();
        } else {
            // Rotate through the parent: separator comes down, left's last key goes up.
            c.keys.insert(
                0,
                std::mem::replace(&mut par.keys[i - 1], l.keys.pop().unwrap()),
            );
            c.kids.insert(0, l.kids.pop().unwrap());
        }
        (self.nodes[p], self.nodes[lid], self.nodes[cid]) = (par, l, c);
    }

    fn borrow_from_right(&mut self, p: usize, i: usize) {
        let (cid, rid) = (self.nodes[p].kids[i], self.nodes[p].kids[i + 1]);
        let (mut par, mut c, mut r) = (self.take(p), self.take(cid), self.take(rid));
        if c.leaf {
            c.keys.push(r.keys.remove(0));
            c.vals.push(r.vals.remove(0));
            par.keys[i] = r.keys[0].clone();
        } else {
            c.keys
                .push(std::mem::replace(&mut par.keys[i], r.keys.remove(0)));
            c.kids.push(r.kids.remove(0));
        }
        (self.nodes[p], self.nodes[cid], self.nodes[rid]) = (par, c, r);
    }

    /// Merge kids[j+1] into kids[j]; the separator between them leaves the parent.
    fn merge(&mut self, p: usize, j: usize) {
        let (lid, rid) = (self.nodes[p].kids[j], self.nodes[p].kids[j + 1]);
        let (mut par, mut l, mut r) = (self.take(p), self.take(lid), self.take(rid));
        let sep = par.keys.remove(j);
        par.kids.remove(j + 1);
        if l.leaf {
            l.keys.append(&mut r.keys);
            l.vals.append(&mut r.vals);
            l.next = r.next; // keep the leaf chain intact
        } else {
            l.keys.push(sep); // internal merge pulls the separator down
            l.keys.append(&mut r.keys);
            l.kids.append(&mut r.kids);
        }
        (self.nodes[p], self.nodes[lid]) = (par, l);
        self.release(rid);
    }

    // -------------------------------------------------------------- range
    fn range<R: RangeBounds<K>>(&self, r: R) -> RangeIter<'_, K, V> {
        let (leaf, pos) = match r.start_bound() {
            Bound::Unbounded => (self.leftmost_leaf(), 0),
            Bound::Included(k) => {
                let l = self.find_leaf(k);
                (l, self.nodes[l].keys.partition_point(|x| x < k))
            }
            Bound::Excluded(k) => {
                let l = self.find_leaf(k);
                (l, self.nodes[l].keys.partition_point(|x| x <= k))
            }
        };
        let end = match r.end_bound() {
            Bound::Included(k) => Bound::Included(k.clone()),
            Bound::Excluded(k) => Bound::Excluded(k.clone()),
            Bound::Unbounded => Bound::Unbounded,
        };
        RangeIter {
            tree: self,
            leaf: Some(leaf),
            pos,
            end,
        }
    }

    // --------------------------------------------------------- invariants
    fn check_invariants(&self) -> Stats {
        let mut st = Stats {
            height: 0,
            leaves: 0,
            internals: 0,
            entries: 0,
            fill: 0.0,
        };
        let mut leaves = vec![];
        let mut leaf_depth = None;
        self.check_node(
            self.root,
            None,
            None,
            1,
            &mut leaves,
            &mut leaf_depth,
            &mut st,
        );
        assert_eq!(
            leaf_depth,
            Some(self.height),
            "uniform leaf depth == tracked height"
        );
        assert_eq!(st.entries, self.len);
        // Leaf chain must visit exactly the DFS-ordered leaves.
        let mut chain = vec![];
        let mut cur = Some(self.leftmost_leaf());
        while let Some(l) = cur {
            chain.push(l);
            cur = self.nodes[l].next;
        }
        assert_eq!(chain, leaves, "leaf chain == left-to-right leaf order");
        st.height = self.height;
        st.fill = st.entries as f64 / (st.leaves * self.order) as f64;
        assert_eq!(
            st.leaves + st.internals,
            self.nodes.len() - self.free.len(),
            "no leaked nodes"
        );
        st
    }

    #[allow(clippy::too_many_arguments)]
    fn check_node(
        &self,
        id: usize,
        lo: Option<&K>,
        hi: Option<&K>,
        depth: usize,
        leaves: &mut Vec<usize>,
        ld: &mut Option<usize>,
        st: &mut Stats,
    ) {
        let n = &self.nodes[id];
        assert!(
            n.keys.windows(2).all(|w| w[0] < w[1]),
            "keys strictly sorted"
        );
        assert!(n.keys.len() <= self.order, "node overfull");
        assert!(
            n.keys
                .iter()
                .all(|k| lo.is_none_or(|l| l <= k) && hi.is_none_or(|h| k < h)),
            "keys within parent bounds"
        );
        let is_root = id == self.root;
        if n.leaf {
            assert_eq!(n.keys.len(), n.vals.len());
            assert!(is_root || n.keys.len() >= self.min_keys(), "leaf underfull");
            assert!(
                ld.replace(depth).is_none_or(|d| d == depth),
                "leaves at different depths"
            );
            leaves.push(id);
            st.leaves += 1;
            st.entries += n.keys.len();
            return;
        }
        assert_eq!(n.kids.len(), n.keys.len() + 1);
        assert!(
            if is_root {
                !n.keys.is_empty()
            } else {
                n.keys.len() >= self.min_keys().max(1)
            },
            "internal underfull"
        );
        st.internals += 1;
        for (i, &c) in n.kids.iter().enumerate() {
            let clo = if i == 0 { lo } else { Some(&n.keys[i - 1]) };
            let chi = if i == n.keys.len() {
                hi
            } else {
                Some(&n.keys[i])
            };
            self.check_node(c, clo, chi, depth + 1, leaves, ld, st);
        }
    }
}

struct RangeIter<'a, K, V> {
    tree: &'a BPlusMap<K, V>,
    leaf: Option<usize>,
    pos: usize,
    end: Bound<K>,
}

impl<'a, K: Ord + Clone, V> Iterator for RangeIter<'a, K, V> {
    type Item = (&'a K, &'a V);
    fn next(&mut self) -> Option<Self::Item> {
        loop {
            let n = &self.tree.nodes[self.leaf?];
            if self.pos >= n.keys.len() {
                self.leaf = n.next; // hop along the leaf chain: no re-descent from the root
                self.pos = 0;
                continue;
            }
            let k = &n.keys[self.pos];
            let in_range = match &self.end {
                Bound::Included(e) => k <= e,
                Bound::Excluded(e) => k < e,
                Bound::Unbounded => true,
            };
            if !in_range {
                self.leaf = None;
                return None;
            }
            self.pos += 1;
            return Some((k, &n.vals[self.pos - 1]));
        }
    }
}

fn differential(order: usize, seed: u64) {
    let mut r = Rng(seed);
    let (mut t, mut m) = (BPlusMap::new(order), BTreeMap::new());
    let mut max_height = 1;
    for step in 0..20_000u32 {
        let k = r.below(1500) as u32;
        match r.below(100) {
            0..=44 => assert_eq!(t.insert(k, step), m.insert(k, step)),
            45..=79 => assert_eq!(t.remove(&k), m.remove(&k)),
            80..=89 => assert_eq!(t.get(&k), m.get(&k)),
            _ => {
                let hi = k + r.below(200) as u32;
                let a: Vec<_> = t.range(k..hi).map(|(k, v)| (*k, *v)).collect();
                let b: Vec<_> = m.range(k..hi).map(|(k, v)| (*k, *v)).collect();
                assert_eq!(a, b, "range {k}..{hi}");
                let c: Vec<_> = t.range(k..=hi).map(|(k, _)| *k).collect();
                assert_eq!(c, m.range(k..=hi).map(|(k, _)| *k).collect::<Vec<_>>());
            }
        }
        let st = t.check_invariants();
        max_height = max_height.max(st.height);
        assert_eq!(t.len, m.len());
    }
    let all: Vec<_> = t.range(..).map(|(k, v)| (*k, *v)).collect();
    assert_eq!(all, m.iter().map(|(k, v)| (*k, *v)).collect::<Vec<_>>());
    for k in m.keys().cloned().collect::<Vec<_>>() {
        t.remove(&k);
        t.check_invariants();
    }
    assert!(
        t.len == 0 && t.height == 1,
        "drained tree collapses to one empty leaf"
    );
    println!(
        "order {order:>2}: 20000 random ops + invariants after each: OK (max height seen {max_height})"
    );
}

fn report(label: &str, t: &BPlusMap<u32, u32>) {
    let s = t.check_invariants();
    println!(
        "{label}: {} entries, height {}, {} leaves, {} internals, leaf fill {:.0}%",
        s.entries,
        s.height,
        s.leaves,
        s.internals,
        s.fill * 100.0
    );
}

fn main() {
    // Demo: a big tree with a page-like order. 128 keys/node ~ a few KiB page of u32 keys.
    let n = 200_000u32;
    let mut seq = BPlusMap::new(128);
    for i in 0..n {
        seq.insert(i, i);
    }
    report("sequential insert", &seq);
    let mut rnd = BPlusMap::new(128);
    let mut r = Rng(42);
    while rnd.len < n as usize {
        let k = r.below(u64::MAX / 2) as u32;
        rnd.insert(k, k);
    }
    report("random insert    ", &rnd);
    println!(
        "(sequential inserts always split the rightmost leaf and leave halves ~50% full; random settles near ln 2 ~ 69%)"
    );
    let slice: Vec<u32> = seq.range(1000..1005).map(|(k, _)| *k).collect();
    println!("range(1000..1005) via leaf chain = {slice:?}");
    assert_eq!(slice, [1000, 1001, 1002, 1003, 1004]);
    // With ~128-way fan-out, 200k keys take only 3 levels; a binary tree needs ~18.
    assert!(seq.height <= 3);

    for (order, seed) in [(3, 1), (4, 2), (5, 3), (8, 4), (32, 5)] {
        differential(order, seed);
    }
}
