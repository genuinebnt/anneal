//! RECIPE 710 (trees): production B+tree features: bulk loading, prefix compression, suffix truncation, duplicates, split policy.
//! LEVEL: advanced
//! Use in: database index builders (CREATE INDEX), ordered KV stores with long string keys, non-unique secondary indexes.   Run: cargo run --release --example btree_bulk_load_prefix_compression
#![allow(dead_code)]
//!
//! WHAT: an arena-based B+tree (nodes in a `Vec`, addressed by `usize`) over variable-length string keys, built to show
//! the features that separate a textbook B+tree from the one inside a database:
//!  1. BULK LOADING: given sorted input, pack leaves left to right to a target FILL FACTOR, then build each internal
//!     level from the one below. No descent, no splits, every page written once, and the tree is denser than one built by
//!     repeated insertion (random inserts settle near ln 2 = 69 percent full, ascending inserts with 50/50 splits at 50).
//!  2. PREFIX COMPRESSION in leaves: neighbouring keys share a long prefix (`warehouse/03/sku-...`). Each leaf stores the
//!     common prefix ONCE plus per-entry suffixes. Because keys inside a leaf are sorted, the LCP of all keys equals the
//!     LCP of the first and last key, so recomputing it is O(prefix).
//!  3. SUFFIX TRUNCATION of separators: an internal key only has to route, so store the SHORTEST string s with
//!     max(left) < s <= min(right) (a prefix of min(right)) instead of the whole key. Shorter separators mean higher fan-out.
//!     Routing stays exact because the invariant `keys(kid i) < sep i <= keys(kid i+1)` is still true; `check()` verifies it
//!     for every key and every node, and the differential test would fail on any mis-route.
//!  4. NON-UNIQUE KEYS: the tree orders `(key, rid)`, i.e. the record id is appended to the key (PostgreSQL 13+ and InnoDB do
//!     the equivalent). Every entry is unique, duplicates of a key are adjacent, and lookup of all rids is a range scan from
//!     `(key, 0)`. Alternative: posting lists (one entry per key holding a rid list), cheaper for hot keys, costlier to update.
//!  5. RANGE SCANS follow `next` sibling pointers across leaves (no re-descent).
//!  6. SPLIT POLICY: `Half` splits 50/50; `RightHeavy` notices an append at the rightmost position and leaves the old leaf full,
//!     so ascending inserts (timestamps, auto-increment ids) give ~100 percent full leaves instead of 50.
//! DELETE is LAZY (entries removed, pages never merged), as in most production B-trees until VACUUM or REINDEX.
//! Keys are ASCII so byte-level prefix cuts are always char boundaries.
//! Verified by `check()` (ordering, routing bounds, uniform depth, prefix validity, leaf chain) and a 20k-op differential test
//! against `BTreeSet<(String, u32)>` for both split policies, started empty and from a bulk-loaded tree.
//! COMMON USES: PostgreSQL btree (deduplication and suffix truncation since v12/13), InnoDB and SQLite index builds from a
//! sorted sort run, RocksDB/LevelDB data-block prefix (restart-point) compression, `COPY`-then-index bulk loads.
//! Course: CMU 15-445 Lecture 08 (Tree Indexes I) and 09 (Tree Indexes II): B+tree design choices, duplicate keys, clustered indexes, bulk loading.
//! Read: Wikipedia, B+ tree: https://en.wikipedia.org/wiki/B%2B_tree
//! Read: PostgreSQL docs, B-Tree implementation (deduplication, suffix truncation): https://www.postgresql.org/docs/current/btree.html
use std::cmp::Ordering;
use std::collections::BTreeSet;

type Entry = (String, u32);
const LCAP: usize = 32; // max entries per leaf
const ICAP: usize = 32; // max separators per internal node (ICAP + 1 children)

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

fn lcp(a: &str, b: &str) -> usize {
    a.bytes().zip(b.bytes()).take_while(|(x, y)| x == y).count()
}

/// Shortest separator `s` with `left < s <= right` in `(key, rid)` order. If the keys differ, a prefix of `right.key`
/// suffices (rid 0 is the smallest rid, so `(prefix, 0) <= right`). Equal keys cannot be truncated: keep the full right entry.
fn shortest_separator(left: &Entry, right: &Entry) -> Entry {
    debug_assert!(left < right);
    if left.0 == right.0 {
        return right.clone();
    }
    let r = &right.0;
    for j in 1..=r.len() {
        if r[..j] > *left.0 {
            return (r[..j].to_string(), 0);
        }
    }
    unreachable!("left.key < right.key guarantees the full key qualifies")
}

/// Sorted entries stored as `prefix + suffix[i]` with a parallel rid vector.
#[derive(Default)]
struct Leaf {
    prefix: String,
    sfx: Vec<String>,
    rids: Vec<u32>,
    next: Option<usize>,
}

impl Leaf {
    fn from_sorted(entries: &[Entry]) -> Leaf {
        let mut l = Leaf::default();
        if let (Some(first), Some(last)) = (entries.first(), entries.last()) {
            let p = lcp(&first.0, &last.0); // sorted => LCP(all) == LCP(first, last)
            l.prefix = first.0[..p].to_string();
            l.sfx = entries.iter().map(|e| e.0[p..].to_string()).collect();
            l.rids = entries.iter().map(|e| e.1).collect();
        }
        l
    }
    fn len(&self) -> usize {
        self.rids.len()
    }
    fn entry(&self, i: usize) -> Entry {
        (format!("{}{}", self.prefix, self.sfx[i]), self.rids[i])
    }
    fn entries_from(&self, at: usize) -> Vec<Entry> {
        (at..self.len()).map(|i| self.entry(i)).collect()
    }
    fn search(&self, t: &Entry) -> Result<usize, usize> {
        let (mut lo, mut hi) = (0, self.len());
        while lo < hi {
            let mid = (lo + hi) / 2;
            match self.entry(mid).cmp(t) {
                Ordering::Less => lo = mid + 1,
                Ordering::Greater => hi = mid,
                Ordering::Equal => return Ok(mid),
            }
        }
        Err(lo)
    }
    fn insert_at(&mut self, pos: usize, e: &Entry) {
        if self.sfx.is_empty() {
            self.prefix = e.0.clone();
        }
        let p = lcp(&self.prefix, &e.0);
        if p < self.prefix.len() {
            // The new key breaks the shared prefix: push the dropped part back onto every suffix (a rare, O(n) event).
            let dropped = self.prefix[p..].to_string();
            for s in &mut self.sfx {
                s.insert_str(0, &dropped);
            }
            self.prefix.truncate(p);
        }
        self.sfx.insert(pos, e.0[self.prefix.len()..].to_string());
        self.rids.insert(pos, e.1);
    }
    fn remove_at(&mut self, pos: usize) {
        self.sfx.remove(pos);
        self.rids.remove(pos); // prefix stays valid (maybe no longer maximal); split/bulk-load recompute it
    }
    /// Cut off entries `at..` and return them; keep the prefix maximal for what remains.
    fn split_off(&mut self, at: usize) -> Vec<Entry> {
        let tail = self.entries_from(at);
        let head = self.entries_from(0)[..at].to_vec();
        let next = self.next;
        *self = Leaf::from_sorted(&head);
        self.next = next;
        tail
    }
    /// Bytes with compression: prefix once, suffixes, 4-byte rids, 2-byte slot per entry.
    fn bytes_compressed(&self) -> usize {
        self.prefix.len() + self.sfx.iter().map(|s| s.len()).sum::<usize>() + 6 * self.len()
    }
    fn bytes_raw(&self) -> usize {
        (0..self.len())
            .map(|i| self.prefix.len() + self.sfx[i].len())
            .sum::<usize>()
            + 6 * self.len()
    }
}

#[derive(Default)]
struct Inner {
    seps: Vec<Entry>,
    kids: Vec<usize>,
}

impl Inner {
    /// Child that may hold `t`: kids[i] holds keys in [seps[i-1], seps[i]).
    fn route(&self, t: &Entry) -> usize {
        self.seps.partition_point(|s| s <= t)
    }
}

enum Node {
    Leaf(Leaf),
    Inner(Inner),
}

#[derive(Clone, Copy, PartialEq, Debug)]
enum Policy {
    Half,
    RightHeavy,
}

#[derive(Debug, Default)]
struct Stats {
    leaves: usize,
    inners: usize,
    height: usize,
    entries: usize,
    leaf_fill: f64,
    leaf_bytes_comp: usize,
    leaf_bytes_raw: usize,
    sep_bytes_trunc: usize,
    sep_bytes_full: usize,
}

struct Tree {
    nodes: Vec<Node>,
    root: usize,
    policy: Policy,
    len: usize,
}

struct Scan<'a> {
    tree: &'a Tree,
    leaf: Option<usize>,
    pos: usize,
}

impl Iterator for Scan<'_> {
    type Item = Entry;
    fn next(&mut self) -> Option<Entry> {
        loop {
            let l = self.tree.leaf(self.leaf?);
            if self.pos < l.len() {
                self.pos += 1;
                return Some(l.entry(self.pos - 1));
            }
            self.leaf = l.next; // sibling pointer: no re-descent
            self.pos = 0;
        }
    }
}

fn even_sizes(total: usize, groups: usize) -> Vec<usize> {
    (0..groups)
        .map(|i| total / groups + usize::from(i < total % groups))
        .collect()
}

struct Item {
    id: usize,
    min: Entry,
    max: Entry,
}

impl Tree {
    fn new(policy: Policy) -> Tree {
        Tree {
            nodes: vec![Node::Leaf(Leaf::default())],
            root: 0,
            policy,
            len: 0,
        }
    }
    fn leaf(&self, id: usize) -> &Leaf {
        match &self.nodes[id] {
            Node::Leaf(l) => l,
            Node::Inner(_) => unreachable!("expected leaf"),
        }
    }
    fn leaf_mut(&mut self, id: usize) -> &mut Leaf {
        match &mut self.nodes[id] {
            Node::Leaf(l) => l,
            Node::Inner(_) => unreachable!("expected leaf"),
        }
    }
    fn inner_mut(&mut self, id: usize) -> &mut Inner {
        match &mut self.nodes[id] {
            Node::Inner(n) => n,
            Node::Leaf(_) => unreachable!("expected inner"),
        }
    }
    fn alloc(&mut self, n: Node) -> usize {
        self.nodes.push(n);
        self.nodes.len() - 1
    }

    /// BULK LOAD: `sorted` must be strictly increasing. Leaves hold `fill * LCAP` entries, spread EVENLY so the last leaf
    /// is never a runt, then each level above is built from the (min, max) of the nodes below: separators are truncated.
    fn bulk_load(sorted: &[Entry], fill: f64, policy: Policy) -> Tree {
        assert!(
            sorted.windows(2).all(|w| w[0] < w[1]),
            "bulk load input must be strictly sorted"
        );
        let mut t = Tree {
            nodes: vec![],
            root: 0,
            policy,
            len: sorted.len(),
        };
        if sorted.is_empty() {
            return Tree::new(policy);
        }
        let per = ((LCAP as f64 * fill).round() as usize).clamp(1, LCAP);
        let mut items: Vec<Item> = vec![];
        let mut off = 0;
        for size in even_sizes(sorted.len(), sorted.len().div_ceil(per)) {
            let chunk = &sorted[off..off + size];
            off += size;
            let id = t.alloc(Node::Leaf(Leaf::from_sorted(chunk)));
            if let Some(prev) = items.last() {
                t.leaf_mut(prev.id).next = Some(id);
            }
            items.push(Item {
                id,
                min: chunk[0].clone(),
                max: chunk[size - 1].clone(),
            });
        }
        let kids_per = (((ICAP + 1) as f64 * fill).round() as usize).clamp(2, ICAP + 1);
        while items.len() > 1 {
            let k = items.len();
            let groups = k.div_ceil(kids_per).min(k / 2).max(1);
            let (mut next_level, mut off) = (vec![], 0);
            for size in even_sizes(k, groups) {
                let g = &items[off..off + size];
                off += size;
                let seps = (1..size)
                    .map(|i| shortest_separator(&g[i - 1].max, &g[i].min))
                    .collect();
                let kids = g.iter().map(|it| it.id).collect();
                let id = t.alloc(Node::Inner(Inner { seps, kids }));
                next_level.push(Item {
                    id,
                    min: g[0].min.clone(),
                    max: g[size - 1].max.clone(),
                });
            }
            items = next_level;
        }
        t.root = items[0].id;
        t
    }

    fn find_leaf(&self, t: &Entry) -> usize {
        let mut id = self.root;
        while let Node::Inner(n) = &self.nodes[id] {
            id = n.kids[n.route(t)];
        }
        id
    }

    fn insert(&mut self, key: &str, rid: u32) -> bool {
        debug_assert!(key.is_ascii());
        let t = (key.to_string(), rid);
        let (mut path, mut id) = (vec![], self.root);
        while let Node::Inner(n) = &self.nodes[id] {
            let i = n.route(&t);
            path.push((id, i));
            id = n.kids[i];
        }
        let leaf = self.leaf_mut(id);
        let pos = match leaf.search(&t) {
            Ok(_) => return false,
            Err(p) => p,
        };
        leaf.insert_at(pos, &t);
        let n = leaf.len();
        self.len += 1;
        if n <= LCAP {
            return true;
        }
        // RightHeavy: the new entry landed at the far right of a full leaf => keep the old leaf full, new leaf gets 1 entry.
        let at = if self.policy == Policy::RightHeavy && pos == n - 1 {
            n - 1
        } else {
            n / 2
        };
        let (sep, new_id) = self.split_leaf(id, at);
        self.propagate(path, sep, new_id);
        true
    }

    fn split_leaf(&mut self, id: usize, at: usize) -> (Entry, usize) {
        let leaf = self.leaf_mut(id);
        let tail = leaf.split_off(at);
        let last_left = leaf.entry(leaf.len() - 1);
        let old_next = leaf.next;
        let mut right = Leaf::from_sorted(&tail);
        right.next = old_next;
        let sep = shortest_separator(&last_left, &tail[0]);
        let new_id = self.alloc(Node::Leaf(right));
        self.leaf_mut(id).next = Some(new_id);
        (sep, new_id)
    }

    fn propagate(&mut self, mut path: Vec<(usize, usize)>, mut sep: Entry, mut new_id: usize) {
        let policy = self.policy;
        while let Some((pid, idx)) = path.pop() {
            let inner = self.inner_mut(pid);
            inner.seps.insert(idx, sep);
            inner.kids.insert(idx + 1, new_id);
            let n = inner.seps.len();
            if n <= ICAP {
                return;
            }
            let mid = if policy == Policy::RightHeavy && idx == n - 1 {
                n - 2
            } else {
                n / 2
            };
            let rseps = inner.seps.split_off(mid + 1);
            let rkids = inner.kids.split_off(mid + 1);
            sep = inner.seps.pop().expect("mid separator moves up");
            new_id = self.alloc(Node::Inner(Inner {
                seps: rseps,
                kids: rkids,
            }));
        }
        let old = self.root;
        self.root = self.alloc(Node::Inner(Inner {
            seps: vec![sep],
            kids: vec![old, new_id],
        }));
    }

    fn remove(&mut self, key: &str, rid: u32) -> bool {
        let t = (key.to_string(), rid);
        let id = self.find_leaf(&t);
        let leaf = self.leaf_mut(id);
        match leaf.search(&t) {
            Ok(p) => {
                leaf.remove_at(p);
                self.len -= 1;
                true
            }
            Err(_) => false,
        }
    }

    /// Iterator over entries `>= t`, following sibling pointers.
    fn scan_from(&self, t: &Entry) -> Scan<'_> {
        let id = self.find_leaf(t);
        let pos = self.leaf(id).search(t).unwrap_or_else(|p| p);
        Scan {
            tree: self,
            leaf: Some(id),
            pos,
        }
    }
    fn get_all(&self, key: &str) -> Vec<u32> {
        self.scan_from(&(key.to_string(), 0))
            .take_while(|e| e.0 == key)
            .map(|e| e.1)
            .collect()
    }
    fn range(&self, lo: &Entry, hi: &Entry) -> Vec<Entry> {
        self.scan_from(lo).take_while(|e| e < hi).collect()
    }
    fn all(&self) -> Vec<Entry> {
        self.scan_from(&(String::new(), 0)).collect()
    }

    /// Invariants: strict order, routing bounds (`lo <= x < hi` for every key under every separator), valid prefixes,
    /// capacity, equal leaf depth, and the sibling chain visiting exactly the DFS leaf order.
    fn check(&self) {
        let mut dfs_leaves = vec![];
        let mut leaf_depth = None;
        let count = self.check_node(self.root, None, None, 0, &mut leaf_depth, &mut dfs_leaves);
        assert_eq!(count, self.len, "entry count");
        let mut chain = vec![];
        let mut cur = Some(dfs_leaves[0]);
        while let Some(id) = cur {
            chain.push(id);
            cur = self.leaf(id).next;
        }
        assert_eq!(chain, dfs_leaves, "sibling chain must match tree order");
    }

    fn check_node(
        &self,
        id: usize,
        lo: Option<&Entry>,
        hi: Option<&Entry>,
        depth: usize,
        leaf_depth: &mut Option<usize>,
        leaves: &mut Vec<usize>,
    ) -> usize {
        match &self.nodes[id] {
            Node::Leaf(l) => {
                assert!(l.len() <= LCAP && l.sfx.len() == l.rids.len());
                assert_eq!(
                    *leaf_depth.get_or_insert(depth),
                    depth,
                    "all leaves at one depth"
                );
                leaves.push(id);
                let mut prev: Option<Entry> = None;
                for i in 0..l.len() {
                    let e = l.entry(i);
                    assert!(e.0.starts_with(&l.prefix), "prefix shared by all keys");
                    assert!(prev.as_ref().is_none_or(|p| *p < e), "strictly increasing");
                    assert!(
                        lo.is_none_or(|b| *b <= e),
                        "routing lower bound violated: {e:?} < {lo:?}"
                    );
                    assert!(
                        hi.is_none_or(|b| e < *b),
                        "routing upper bound violated: {e:?} >= {hi:?}"
                    );
                    prev = Some(e);
                }
                l.len()
            }
            Node::Inner(n) => {
                assert!(
                    !n.seps.is_empty() && n.seps.len() <= ICAP && n.kids.len() == n.seps.len() + 1
                );
                assert!(n.seps.windows(2).all(|w| w[0] < w[1]));
                let mut total = 0;
                for (i, &kid) in n.kids.iter().enumerate() {
                    let klo = if i == 0 { lo } else { Some(&n.seps[i - 1]) };
                    let khi = if i == n.seps.len() {
                        hi
                    } else {
                        Some(&n.seps[i])
                    };
                    total += self.check_node(kid, klo, khi, depth + 1, leaf_depth, leaves);
                }
                total
            }
        }
    }

    fn min_entry(&self, mut id: usize) -> Option<Entry> {
        while let Node::Inner(n) = &self.nodes[id] {
            id = n.kids[0];
        }
        self.leaf(id).entries_from(0).into_iter().next()
    }

    fn stats(&self) -> Stats {
        let mut s = Stats::default();
        let mut stack = vec![(self.root, 1)];
        while let Some((id, d)) = stack.pop() {
            s.height = s.height.max(d);
            match &self.nodes[id] {
                Node::Leaf(l) => {
                    s.leaves += 1;
                    s.entries += l.len();
                    s.leaf_bytes_comp += l.bytes_compressed();
                    s.leaf_bytes_raw += l.bytes_raw();
                }
                Node::Inner(n) => {
                    s.inners += 1;
                    s.sep_bytes_trunc += n.seps.iter().map(|e| e.0.len() + 4).sum::<usize>();
                    for i in 1..n.kids.len() {
                        // What an un-truncated tree would store: the first full key of the right child.
                        s.sep_bytes_full += self.min_entry(n.kids[i]).map_or(0, |e| e.0.len() + 4);
                    }
                    stack.extend(n.kids.iter().map(|&k| (k, d + 1)));
                }
            }
        }
        s.leaf_fill = s.entries as f64 / (s.leaves * LCAP) as f64;
        s
    }
}

fn word(rng: &mut Rng) -> String {
    (0..3 + rng.below(7))
        .map(|_| (b'a' + rng.below(5) as u8) as char)
        .collect()
}

/// Pool of distinct keys with a long shared prefix, as in `warehouse/03/sku-abca`.
fn key_pool(rng: &mut Rng, n: usize) -> Vec<String> {
    let mut set = BTreeSet::new();
    while set.len() < n {
        set.insert(format!("warehouse/{:02}/sku-{}", rng.below(6), word(rng)));
    }
    set.into_iter().collect()
}

fn sorted_dataset(pool: &[String], dup: u32) -> Vec<Entry> {
    pool.iter()
        .flat_map(|k| (0..dup).map(move |r| (k.clone(), r)))
        .collect()
}

fn row(name: &str, s: &Stats) {
    println!(
        "  {name:<28} leaves {:>5} inner {:>3} height {} leaf fill {:>5.1}%  leaf bytes {:>8}  sep bytes {:>6}",
        s.leaves,
        s.inners,
        s.height,
        s.leaf_fill * 100.0,
        s.leaf_bytes_comp,
        s.sep_bytes_trunc
    );
}

/// 20k random ops against the oracle; invariants every 100 ops.
fn differential(mut tree: Tree, mut oracle: BTreeSet<Entry>, pool: &[String], seed: u64) -> Tree {
    let mut rng = Rng(seed);
    for step in 0..20_000 {
        let key = &pool[rng.below(pool.len() as u64) as usize];
        let rid = rng.below(12) as u32;
        match rng.below(100) {
            0..=49 => assert_eq!(tree.insert(key, rid), oracle.insert((key.clone(), rid))),
            50..=74 => assert_eq!(tree.remove(key, rid), oracle.remove(&(key.clone(), rid))),
            75..=89 => {
                let want: Vec<u32> = oracle
                    .range((key.clone(), 0)..=(key.clone(), u32::MAX))
                    .map(|e| e.1)
                    .collect();
                assert_eq!(tree.get_all(key), want, "duplicate lookup of {key}");
            }
            _ => {
                let other = &pool[rng.below(pool.len() as u64) as usize];
                let (lo, hi) = ((key.clone(), 0), (other.clone(), 0));
                if lo < hi {
                    let want: Vec<Entry> = oracle.range(lo.clone()..hi.clone()).cloned().collect();
                    assert_eq!(tree.range(&lo, &hi), want, "range scan");
                }
            }
        }
        assert_eq!(tree.len, oracle.len());
        if step % 100 == 0 {
            tree.check();
        }
    }
    tree.check();
    assert_eq!(tree.all(), oracle.iter().cloned().collect::<Vec<_>>());
    tree
}

fn main() {
    let mut rng = Rng(0x9E37_79B9_7F4A_7C15);
    let pool = key_pool(&mut rng, 6000);
    let data = sorted_dataset(&pool, 5);
    println!(
        "dataset: {} distinct keys x 5 rids = {} entries, LCAP={LCAP}, ICAP={ICAP}",
        pool.len(),
        data.len()
    );

    // 1) Repeated insertion vs bulk load.
    let mut seq_half = Tree::new(Policy::Half);
    let mut seq_right = Tree::new(Policy::RightHeavy);
    for e in &data {
        seq_half.insert(&e.0, e.1);
        seq_right.insert(&e.0, e.1);
    }
    let mut shuffled = data.clone();
    for i in (1..shuffled.len()).rev() {
        shuffled.swap(i, rng.below(i as u64 + 1) as usize);
    }
    let mut random_half = Tree::new(Policy::Half);
    for e in &shuffled {
        random_half.insert(&e.0, e.1);
    }
    let bulk90 = Tree::bulk_load(&data, 0.90, Policy::Half);
    let bulk70 = Tree::bulk_load(&data, 0.70, Policy::Half);
    for t in [&seq_half, &seq_right, &random_half, &bulk90, &bulk70] {
        t.check();
        assert_eq!(t.all(), data);
    }
    let (sh, sr, rh, b9, b7) = (
        seq_half.stats(),
        seq_right.stats(),
        random_half.stats(),
        bulk90.stats(),
        bulk70.stats(),
    );
    println!("build strategies:");
    row("ascending inserts, Half", &sh);
    row("ascending inserts, RightHeavy", &sr);
    row("random inserts, Half", &rh);
    row("bulk load fill 0.90", &b9);
    row("bulk load fill 0.70", &b7);
    assert!(
        sh.leaf_fill < 0.60,
        "50/50 splits on ascending input leave leaves about half full"
    );
    assert!(
        sr.leaf_fill > 0.93,
        "right-heavy split packs ascending input"
    );
    assert!(
        (0.62..0.78).contains(&rh.leaf_fill),
        "random inserts settle near ln 2"
    );
    assert!((0.86..=0.91).contains(&b9.leaf_fill) && (0.66..=0.71).contains(&b7.leaf_fill));
    assert!(
        b9.leaves + b9.inners < sh.leaves + sh.inners,
        "bulk load makes fewer nodes than 50/50 inserts"
    );
    assert!(
        b9.leaves < rh.leaves,
        "bulk load is denser than random insertion"
    );
    println!(
        "  -> bulk(0.90) uses {:.0}% of the nodes of random inserts and {:.0}% of ascending 50/50 inserts",
        100.0 * (b9.leaves + b9.inners) as f64 / (rh.leaves + rh.inners) as f64,
        100.0 * (b9.leaves + b9.inners) as f64 / (sh.leaves + sh.inners) as f64
    );

    // 2) Prefix compression and suffix truncation.
    let saved = 1.0 - b9.leaf_bytes_comp as f64 / b9.leaf_bytes_raw as f64;
    let sep_saved = 1.0 - b9.sep_bytes_trunc as f64 / b9.sep_bytes_full as f64;
    println!(
        "prefix compression: leaf bytes {} -> {} ({:.1}% saved); suffix truncation: separator bytes {} -> {} ({:.1}% saved)",
        b9.leaf_bytes_raw,
        b9.leaf_bytes_comp,
        saved * 100.0,
        b9.sep_bytes_full,
        b9.sep_bytes_trunc,
        sep_saved * 100.0
    );
    assert!(saved > 0.30, "shared prefixes must save space");
    assert!(
        sep_saved > 0.0,
        "truncated separators are never longer, and here slightly shorter"
    );
    // Dense keys differ only in their last bytes, so truncation helps little; long keys that differ early gain a lot.
    let mut long_keys = BTreeSet::new();
    while long_keys.len() < 5000 {
        let tail: String = (0..40)
            .map(|_| (b'a' + rng.below(26) as u8) as char)
            .collect();
        long_keys.insert(format!("user-{tail}"));
    }
    let long: Vec<Entry> = long_keys.into_iter().map(|k| (k, 0)).collect();
    let lt = Tree::bulk_load(&long, 0.9, Policy::Half);
    lt.check();
    let ls = lt.stats();
    let lsaved = 1.0 - ls.sep_bytes_trunc as f64 / ls.sep_bytes_full as f64;
    println!(
        "  sparse 45-byte keys: separator bytes {} -> {} ({:.1}% saved)",
        ls.sep_bytes_full,
        ls.sep_bytes_trunc,
        lsaved * 100.0
    );
    assert!(lsaved > 0.5, "sparse long keys truncate to a few bytes");
    for (k, r) in &long {
        assert_eq!(
            lt.get_all(k),
            vec![*r],
            "truncated separators still route exactly"
        );
    }

    // 3) Non-unique keys and range scans via sibling pointers.
    let k = &pool[1234];
    assert_eq!(bulk90.get_all(k), vec![0, 1, 2, 3, 4]);
    assert!(bulk90.get_all("warehouse/zz/none").is_empty());
    let (lo, hi) = ((pool[100].clone(), 0), (pool[900].clone(), 0));
    let r = bulk90.range(&lo, &hi);
    assert_eq!(r.len(), 800 * 5);
    println!(
        "range [{}, {}) -> {} entries across leaves via sibling pointers",
        pool[100],
        pool[900],
        r.len()
    );
    for k in pool.iter().step_by(37) {
        assert_eq!(
            seq_right.get_all(k).len(),
            5,
            "truncated separators route every key correctly"
        );
    }

    // 4) Differential tests: both policies, empty start and bulk-loaded start.
    for (i, policy) in [Policy::Half, Policy::RightHeavy].into_iter().enumerate() {
        let t = differential(Tree::new(policy), BTreeSet::new(), &pool, 100 + i as u64);
        let s = t.stats();
        println!(
            "differential from empty ({policy:?}): {} entries, {} leaves, fill {:.1}%",
            t.len,
            s.leaves,
            s.leaf_fill * 100.0
        );
        let init: Vec<Entry> = data.iter().step_by(2).cloned().collect();
        let oracle: BTreeSet<Entry> = init.iter().cloned().collect();
        let t = differential(
            Tree::bulk_load(&init, 0.9, policy),
            oracle,
            &pool,
            200 + i as u64,
        );
        println!(
            "differential from bulk load ({policy:?}): {} entries, height {}",
            t.len,
            t.stats().height
        );
    }

    // 5) Edge cases: tiny bulk loads.
    for n in [0usize, 1, 2, 31, 32, 33, 100, 1089, 1090] {
        let d: Vec<Entry> = data.iter().take(n).cloned().collect();
        let t = Tree::bulk_load(&d, 1.0, Policy::Half);
        t.check();
        assert_eq!(t.all(), d);
    }
    println!("all assertions passed");
}
