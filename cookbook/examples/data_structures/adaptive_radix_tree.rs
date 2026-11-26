//! RECIPE 711 (trees): the Adaptive Radix Tree (ART): Node4/16/48/256, path compression, lazy leaf expansion, ordered scans.
//! LEVEL: advanced
//! Use in: in-memory database indexes (primary keys, string dictionaries), routing tables, ordered maps with prefix queries.   Run: cargo run --release --example adaptive_radix_tree
#![allow(dead_code)]
//!
//! WHAT: a radix tree over BYTES whose inner nodes adapt their layout to how many children they really have, so a sparse
//! node costs 52 bytes instead of the 2 KiB a plain 256-way trie node always costs, while a dense node keeps O(1) child lookup.
//!   Node4   : up to 4 sorted key bytes + 4 child ids (linear search)
//!   Node16  : up to 16 sorted key bytes + 16 child ids (the paper uses SIMD compare; a short scan here)
//!   Node48  : 256-entry byte->slot index + 48 child ids (one indirection)
//!   Node256 : 256 child ids indexed directly
//! Nodes GROW (4->16->48->256) when full and SHRINK (256->48 at 37, 48->16 at 12, 16->4 at 3 children) on delete.
//! PATH COMPRESSION: a chain of single-child nodes is replaced by a `prefix` stored in the next node; deleting down to one child
//! merges the node into its child (prefix = parent prefix + branch byte + child prefix). LAZY EXPANSION: a leaf is attached as
//! high as possible and stores its FULL key, so a unique suffix costs no inner nodes; it is split only when a second key arrives.
//! KEYS must be binary-comparable and prefix-free: u64 -> 8 big-endian bytes; i64 -> flip the sign bit first; f64 -> flip all bits
//! if negative else the sign bit; strings -> UTF-8 bytes plus a 0 terminator (so "ab" sorts before "abc"). Byte order then equals
//! key order, hence ordered iteration, range scans and PREFIX scans fall out of an in-order walk with subtree pruning.
//! MEMORY ACCOUNTING uses the paper's layout sizes (16-byte header, 8-byte pointers) so numbers compare with the paper; the
//! arena here uses u32 ids and `Vec`s, which is a simplification, not the model.
//! Verified by a 20k-op differential test each for u64 and string keys against `BTreeMap`, a structural invariant checker (kind
//! capacity thresholds, >= 2 children, key/path consistency), a deterministic grow/shrink ladder, and order-preservation tests.
//! COMMON USES: HyPer and DuckDB use ART for indexes; the Linux kernel's XArray/IDR and many routing-table implementations use
//! radix trees with the same compressed-path idea; Redis rax is a compressed radix tree for stream and cluster keys.
//! Course: CMU 15-445 Lecture 08-09 (Tree Indexes): radix trees / tries as alternatives to the B+tree; ART for in-memory indexes.
//! Read: Leis, Kemper, Neumann, "The Adaptive Radix Tree: ARTful Indexing for Main-Memory Databases": https://db.in.tum.de/~leis/papers/ART.pdf
//! Read: Leis et al., "The ART of Practical Synchronization": https://db.in.tum.de/~leis/papers/artsync.pdf
use std::cmp::Ordering;
use std::collections::BTreeMap;
use std::time::Instant;

const NIL: u32 = u32::MAX;
const NONE_IDX: u8 = 0xFF;

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

// ---- order-preserving key encodings -------------------------------------------------------------------------------
fn enc_u64(x: u64) -> [u8; 8] {
    x.to_be_bytes()
}
fn enc_i64(x: i64) -> [u8; 8] {
    ((x as u64) ^ (1 << 63)).to_be_bytes()
}
fn enc_f64(x: f64) -> [u8; 8] {
    let b = x.to_bits();
    (if b >> 63 == 1 { !b } else { b ^ (1 << 63) }).to_be_bytes()
}
fn enc_str(s: &str) -> Vec<u8> {
    assert!(!s.as_bytes().contains(&0), "0 is the terminator");
    let mut v = s.as_bytes().to_vec();
    v.push(0);
    v
}

// ---- the four inner-node layouts -----------------------------------------------------------------------------------
enum Body {
    /// Node4 (cap 4) and Node16 (cap 16): parallel sorted vectors.
    Sorted {
        cap: usize,
        keys: Vec<u8>,
        kids: Vec<u32>,
    },
    N48 {
        idx: Vec<u8>,
        kids: Vec<u32>,
        n: usize,
    },
    N256 {
        kids: Vec<u32>,
        n: usize,
    },
}

impl Body {
    fn with_entries(cap: usize, entries: &[(u8, u32)]) -> Body {
        assert!(entries.len() <= cap);
        match cap {
            4 | 16 => Body::Sorted {
                cap,
                keys: entries.iter().map(|e| e.0).collect(),
                kids: entries.iter().map(|e| e.1).collect(),
            },
            48 => {
                let mut idx = vec![NONE_IDX; 256];
                let mut kids = vec![NIL; 48];
                for (slot, &(b, c)) in entries.iter().enumerate() {
                    idx[b as usize] = slot as u8;
                    kids[slot] = c;
                }
                Body::N48 {
                    idx,
                    kids,
                    n: entries.len(),
                }
            }
            256 => {
                let mut kids = vec![NIL; 256];
                for &(b, c) in entries {
                    kids[b as usize] = c;
                }
                Body::N256 {
                    kids,
                    n: entries.len(),
                }
            }
            _ => unreachable!(),
        }
    }
    fn cap(&self) -> usize {
        match self {
            Body::Sorted { cap, .. } => *cap,
            Body::N48 { .. } => 48,
            Body::N256 { .. } => 256,
        }
    }
    fn count(&self) -> usize {
        match self {
            Body::Sorted { keys, .. } => keys.len(),
            Body::N48 { n, .. } | Body::N256 { n, .. } => *n,
        }
    }
    fn find(&self, b: u8) -> Option<u32> {
        match self {
            Body::Sorted { keys, kids, .. } => keys.iter().position(|&k| k == b).map(|i| kids[i]),
            Body::N48 { idx, kids, .. } => {
                (idx[b as usize] != NONE_IDX).then(|| kids[idx[b as usize] as usize])
            }
            Body::N256 { kids, .. } => (kids[b as usize] != NIL).then(|| kids[b as usize]),
        }
    }
    /// Replace the child of an EXISTING byte.
    fn set(&mut self, b: u8, c: u32) {
        match self {
            Body::Sorted { keys, kids, .. } => kids[keys.iter().position(|&k| k == b).unwrap()] = c,
            Body::N48 { idx, kids, .. } => kids[idx[b as usize] as usize] = c,
            Body::N256 { kids, .. } => kids[b as usize] = c,
        }
    }
    /// Add a NEW byte; the caller grows the node first if it is full.
    fn add(&mut self, b: u8, c: u32) {
        assert!(self.count() < self.cap());
        match self {
            Body::Sorted { keys, kids, .. } => {
                let p = keys.partition_point(|&k| k < b); // keep sorted: ordered iteration needs no extra sort
                keys.insert(p, b);
                kids.insert(p, c);
            }
            Body::N48 { idx, kids, n } => {
                let slot = kids.iter().position(|&k| k == NIL).unwrap();
                kids[slot] = c;
                idx[b as usize] = slot as u8;
                *n += 1;
            }
            Body::N256 { kids, n } => {
                kids[b as usize] = c;
                *n += 1;
            }
        }
    }
    fn remove(&mut self, b: u8) {
        match self {
            Body::Sorted { keys, kids, .. } => {
                let p = keys.iter().position(|&k| k == b).unwrap();
                keys.remove(p);
                kids.remove(p);
            }
            Body::N48 { idx, kids, n } => {
                kids[idx[b as usize] as usize] = NIL;
                idx[b as usize] = NONE_IDX;
                *n -= 1;
            }
            Body::N256 { kids, n } => {
                kids[b as usize] = NIL;
                *n -= 1;
            }
        }
    }
    /// Children in ascending byte order.
    fn entries(&self) -> Vec<(u8, u32)> {
        match self {
            Body::Sorted { keys, kids, .. } => {
                keys.iter().copied().zip(kids.iter().copied()).collect()
            }
            Body::N48 { idx, kids, .. } => (0..=255u8)
                .filter(|&b| idx[b as usize] != NONE_IDX)
                .map(|b| (b, kids[idx[b as usize] as usize]))
                .collect(),
            Body::N256 { kids, .. } => (0..=255u8)
                .filter(|&b| kids[b as usize] != NIL)
                .map(|b| (b, kids[b as usize]))
                .collect(),
        }
    }
    /// Paper layout sizes: header 16 + keys + 8-byte child pointers.
    fn bytes(&self) -> usize {
        match self.cap() {
            4 => 16 + 4 + 4 * 8,
            16 => 16 + 16 + 16 * 8,
            48 => 16 + 256 + 48 * 8,
            _ => 16 + 256 * 8,
        }
    }
}

struct Inner {
    prefix: Vec<u8>,
    body: Body,
}

enum Node {
    Leaf { key: Vec<u8>, val: u64 },
    Inner(Inner),
    Free,
}

#[derive(Default, Debug, Clone)]
struct Events {
    grows: [usize; 3],   // 4->16, 16->48, 48->256
    shrinks: [usize; 3], // 256->48, 48->16, 16->4
    collapses: usize,    // single-child node merged into its child (path compression on delete)
    prefix_splits: usize,
}

#[derive(Default, Debug)]
struct Census {
    n4: usize,
    n16: usize,
    n48: usize,
    n256: usize,
    leaves: usize,
    bytes: usize,
    leaf_bytes: usize,
}

struct Art {
    nodes: Vec<Node>,
    free: Vec<u32>,
    root: u32,
    len: usize,
    ev: Events,
}

fn lcp(a: &[u8], b: &[u8]) -> usize {
    a.iter().zip(b).take_while(|(x, y)| x == y).count()
}

impl Art {
    fn new() -> Art {
        Art {
            nodes: vec![],
            free: vec![],
            root: NIL,
            len: 0,
            ev: Events::default(),
        }
    }
    fn alloc(&mut self, n: Node) -> u32 {
        if let Some(id) = self.free.pop() {
            self.nodes[id as usize] = n;
            id
        } else {
            self.nodes.push(n);
            (self.nodes.len() - 1) as u32
        }
    }
    fn release(&mut self, id: u32) {
        self.nodes[id as usize] = Node::Free;
        self.free.push(id);
    }
    fn inner_mut(&mut self, id: u32) -> &mut Inner {
        match &mut self.nodes[id as usize] {
            Node::Inner(i) => i,
            _ => unreachable!("expected inner node"),
        }
    }
    fn new_n4(&mut self, prefix: &[u8], kids: [(u8, u32); 2]) -> u32 {
        let mut kids = kids;
        kids.sort();
        let body = Body::with_entries(4, &kids);
        self.alloc(Node::Inner(Inner {
            prefix: prefix.to_vec(),
            body,
        }))
    }

    fn get(&self, key: &[u8]) -> Option<u64> {
        self.get_counting(key).0
    }
    /// Lookup that also reports how many inner nodes were visited.
    fn get_counting(&self, key: &[u8]) -> (Option<u64>, usize) {
        let (mut id, mut depth, mut visited) = (self.root, 0, 0);
        while id != NIL {
            match &self.nodes[id as usize] {
                // Lazy expansion: the leaf holds the whole key, so one comparison confirms the match.
                Node::Leaf { key: k, val } => {
                    return ((k.as_slice() == key).then_some(*val), visited);
                }
                Node::Inner(inn) => {
                    visited += 1;
                    if !key[depth..].starts_with(&inn.prefix) {
                        return (None, visited);
                    }
                    depth += inn.prefix.len();
                    let Some(&b) = key.get(depth) else {
                        return (None, visited);
                    };
                    id = inn.body.find(b).unwrap_or(NIL);
                    depth += 1;
                }
                Node::Free => unreachable!(),
            }
        }
        (None, visited)
    }

    fn insert(&mut self, key: &[u8], val: u64) -> Option<u64> {
        let (root, old) = self.insert_rec(self.root, key, 0, val);
        self.root = root;
        if old.is_none() {
            self.len += 1;
        }
        old
    }

    /// Returns the (possibly new) id of the subtree root and the replaced value.
    fn insert_rec(&mut self, id: u32, key: &[u8], depth: usize, val: u64) -> (u32, Option<u64>) {
        if id == NIL {
            return (
                self.alloc(Node::Leaf {
                    key: key.to_vec(),
                    val,
                }),
                None,
            );
        }
        match &mut self.nodes[id as usize] {
            Node::Leaf { key: ek, val: ev } => {
                if ek.as_slice() == key {
                    return (id, Some(std::mem::replace(ev, val)));
                }
                // Lazy expansion undone: two keys now share this slot, so create the N4 holding their common path.
                let l = lcp(&ek[depth..], &key[depth..]);
                let (eb, nb) = (ek[depth + l], key[depth + l]);
                let leaf = self.alloc(Node::Leaf {
                    key: key.to_vec(),
                    val,
                });
                (
                    self.new_n4(&key[depth..depth + l], [(eb, id), (nb, leaf)]),
                    None,
                )
            }
            Node::Inner(inn) => {
                let p = lcp(&inn.prefix, &key[depth..]);
                if p < inn.prefix.len() {
                    // New key diverges inside the compressed path: split it with a new parent node.
                    let branch = inn.prefix[p];
                    let head = inn.prefix[..p].to_vec();
                    inn.prefix.drain(..=p);
                    let leaf = self.alloc(Node::Leaf {
                        key: key.to_vec(),
                        val,
                    });
                    self.ev.prefix_splits += 1;
                    return (
                        self.new_n4(&head, [(branch, id), (key[depth + p], leaf)]),
                        None,
                    );
                }
                let d = depth + inn.prefix.len();
                let b = key[d];
                match inn.body.find(b) {
                    Some(child) => {
                        let (c, old) = self.insert_rec(child, key, d + 1, val);
                        if c != child {
                            self.inner_mut(id).body.set(b, c);
                        }
                        (id, old)
                    }
                    None => {
                        let leaf = self.alloc(Node::Leaf {
                            key: key.to_vec(),
                            val,
                        });
                        self.add_child(id, b, leaf);
                        (id, None)
                    }
                }
            }
            Node::Free => unreachable!(),
        }
    }

    fn add_child(&mut self, id: u32, b: u8, child: u32) {
        let inn = self.inner_mut(id);
        if inn.body.count() == inn.body.cap() {
            let next = match inn.body.cap() {
                4 => 16,
                16 => 48,
                _ => 256,
            };
            inn.body = Body::with_entries(next, &inn.body.entries());
            let step = match next {
                16 => 0,
                48 => 1,
                _ => 2,
            };
            self.ev.grows[step] += 1;
        }
        self.inner_mut(id).body.add(b, child);
    }

    fn remove(&mut self, key: &[u8]) -> Option<u64> {
        let (root, old) = self.remove_rec(self.root, key, 0);
        self.root = root;
        if old.is_some() {
            self.len -= 1;
        }
        old
    }

    fn remove_rec(&mut self, id: u32, key: &[u8], depth: usize) -> (u32, Option<u64>) {
        if id == NIL {
            return (NIL, None);
        }
        let (b, d, child) = match &self.nodes[id as usize] {
            Node::Leaf { key: k, val } => {
                if k.as_slice() != key {
                    return (id, None);
                }
                let v = *val;
                self.release(id);
                return (NIL, Some(v));
            }
            Node::Inner(inn) => {
                if !key[depth..].starts_with(&inn.prefix) {
                    return (id, None);
                }
                let d = depth + inn.prefix.len();
                let Some(&b) = key.get(d) else {
                    return (id, None);
                };
                let Some(child) = inn.body.find(b) else {
                    return (id, None);
                };
                (b, d, child)
            }
            Node::Free => unreachable!(),
        };
        let (c, old) = self.remove_rec(child, key, d + 1);
        if old.is_none() {
            return (id, None);
        }
        if c != NIL {
            if c != child {
                self.inner_mut(id).body.set(b, c);
            }
            return (id, old);
        }
        self.inner_mut(id).body.remove(b);
        (self.after_removal(id), old)
    }

    /// Shrink the layout if sparse, or merge into the only remaining child (path compression).
    fn after_removal(&mut self, id: u32) -> u32 {
        let inn = self.inner_mut(id);
        let n = inn.body.count();
        if n == 1 {
            let (b, only) = inn.body.entries()[0];
            let parent_prefix = std::mem::take(&mut inn.prefix);
            self.release(id);
            self.ev.collapses += 1;
            if let Node::Inner(ci) = &mut self.nodes[only as usize] {
                let mut p = parent_prefix;
                p.push(b);
                p.extend_from_slice(&ci.prefix);
                ci.prefix = p; // a leaf needs nothing: it stores its whole key
            }
            return only;
        }
        let target = match (inn.body.cap(), n) {
            (256, ..=37) => Some((48, 0)),
            (48, ..=12) => Some((16, 1)),
            (16, ..=3) => Some((4, 2)),
            _ => None,
        };
        if let Some((cap, step)) = target {
            inn.body = Body::with_entries(cap, &inn.body.entries());
            self.ev.shrinks[step] += 1;
        }
        id
    }

    // ---- ordered traversal ------------------------------------------------------------------------------------------
    /// All (key, value) with lo <= key < hi, in key order. Subtrees whose path cannot intersect the range are pruned.
    fn range(&self, lo: &[u8], hi: Option<&[u8]>) -> Vec<(Vec<u8>, u64)> {
        let mut out = vec![];
        if self.root != NIL {
            self.walk(self.root, &mut vec![], lo, hi, &mut out);
        }
        out
    }
    fn prune(path: &[u8], lo: &[u8], hi: Option<&[u8]>) -> bool {
        let n = path.len().min(lo.len());
        if path[..n] < lo[..n] {
            return true; // every key below this path is smaller than lo
        }
        hi.is_some_and(|h| {
            let n = path.len().min(h.len());
            match path[..n].cmp(&h[..n]) {
                Ordering::Greater => true,
                Ordering::Equal => path.len() >= h.len(),
                Ordering::Less => false,
            }
        })
    }
    fn walk(
        &self,
        id: u32,
        path: &mut Vec<u8>,
        lo: &[u8],
        hi: Option<&[u8]>,
        out: &mut Vec<(Vec<u8>, u64)>,
    ) {
        match &self.nodes[id as usize] {
            Node::Leaf { key, val } => {
                if key.as_slice() >= lo && hi.is_none_or(|h| key.as_slice() < h) {
                    out.push((key.clone(), *val));
                }
            }
            Node::Inner(inn) => {
                let base = path.len();
                path.extend_from_slice(&inn.prefix);
                if !Self::prune(path, lo, hi) {
                    for (b, c) in inn.body.entries() {
                        path.push(b);
                        if !Self::prune(path, lo, hi) {
                            self.walk(c, path, lo, hi, out);
                        }
                        path.pop();
                    }
                }
                path.truncate(base);
            }
            Node::Free => unreachable!(),
        }
    }
    fn iter_all(&self) -> Vec<(Vec<u8>, u64)> {
        self.range(&[], None)
    }
    /// Smallest byte string greater than every string with this prefix.
    fn prefix_end(p: &[u8]) -> Option<Vec<u8>> {
        let mut e = p.to_vec();
        while let Some(last) = e.pop() {
            if last != 0xFF {
                e.push(last + 1);
                return Some(e);
            }
        }
        None
    }
    fn prefix_scan(&self, p: &[u8]) -> Vec<(Vec<u8>, u64)> {
        let end = Self::prefix_end(p);
        self.range(p, end.as_deref())
    }

    // ---- accounting and invariants ------------------------------------------------------------------------------------
    fn census(&self) -> Census {
        let mut c = Census::default();
        for n in &self.nodes {
            match n {
                Node::Leaf { key, .. } => {
                    c.leaves += 1;
                    let b = 16 + key.len(); // header + value + key bytes
                    c.bytes += b;
                    c.leaf_bytes += b;
                }
                Node::Inner(i) => {
                    match i.body.cap() {
                        4 => c.n4 += 1,
                        16 => c.n16 += 1,
                        48 => c.n48 += 1,
                        _ => c.n256 += 1,
                    }
                    c.bytes += i.body.bytes() + i.prefix.len().saturating_sub(8); // 8 prefix bytes live in the header
                }
                Node::Free => {}
            }
        }
        c
    }

    fn check(&self) {
        let mut n = 0;
        if self.root != NIL {
            n = self.check_node(self.root, &mut vec![]);
        }
        assert_eq!(n, self.len);
        let all = self.iter_all();
        assert!(
            all.windows(2).all(|w| w[0].0 < w[1].0),
            "iteration is strictly ordered"
        );
    }
    fn check_node(&self, id: u32, path: &mut Vec<u8>) -> usize {
        match &self.nodes[id as usize] {
            Node::Leaf { key, .. } => {
                assert!(key.starts_with(path), "leaf key must extend its path");
                1
            }
            Node::Inner(inn) => {
                let n = inn.body.count();
                let ok = match inn.body.cap() {
                    4 => (2..=4).contains(&n),
                    16 => (4..=16).contains(&n),
                    48 => (13..=48).contains(&n),
                    _ => (38..=256).contains(&n),
                };
                assert!(
                    ok,
                    "node cap {} holds {n} children, outside its band",
                    inn.body.cap()
                );
                let base = path.len();
                path.extend_from_slice(&inn.prefix);
                let mut total = 0;
                let entries = inn.body.entries();
                assert_eq!(entries.len(), n);
                assert!(entries.windows(2).all(|w| w[0].0 < w[1].0));
                for (b, c) in entries {
                    path.push(b);
                    total += self.check_node(c, path);
                    path.pop();
                }
                path.truncate(base);
                total
            }
            Node::Free => panic!("reachable free node"),
        }
    }
}

// ---- comparison structures -----------------------------------------------------------------------------------------
/// A plain 256-way trie over fixed 8-byte keys: every node is 256 slots, whatever its real fan-out.
struct PlainTrie {
    nodes: Vec<Vec<u32>>,
    vals: Vec<u64>,
}
impl PlainTrie {
    fn new() -> Self {
        PlainTrie {
            nodes: vec![vec![NIL; 256]],
            vals: vec![],
        }
    }
    fn insert(&mut self, key: &[u8; 8], val: u64) {
        let mut id = 0;
        for (i, &b) in key.iter().enumerate() {
            if i == 7 {
                self.vals.push(val);
                self.nodes[id][b as usize] = (self.vals.len() - 1) as u32;
                return;
            }
            if self.nodes[id][b as usize] == NIL {
                self.nodes.push(vec![NIL; 256]);
                let n = (self.nodes.len() - 1) as u32;
                self.nodes[id][b as usize] = n;
            }
            id = self.nodes[id][b as usize] as usize;
        }
    }
    fn get(&self, key: &[u8; 8]) -> Option<u64> {
        let mut id = 0;
        for &b in &key[..7] {
            id = match self.nodes[id][b as usize] {
                NIL => return None,
                n => n as usize,
            };
        }
        let s = self.nodes[id][key[7] as usize];
        (s != NIL).then(|| self.vals[s as usize])
    }
    fn bytes(&self) -> usize {
        self.nodes.len() * 256 * 8 + self.vals.len() * 8
    }
}

/// Sorted array standing in for a B+tree: counts key comparisons like a node-wise binary search would.
fn sorted_array_get(keys: &[u64], k: u64, cmps: &mut usize) -> bool {
    let (mut lo, mut hi) = (0, keys.len());
    while lo < hi {
        let mid = (lo + hi) / 2;
        *cmps += 1;
        match keys[mid].cmp(&k) {
            Ordering::Less => lo = mid + 1,
            Ordering::Greater => hi = mid,
            Ordering::Equal => return true,
        }
    }
    false
}

// ---- tests ---------------------------------------------------------------------------------------------------------
fn gen_u64(rng: &mut Rng) -> u64 {
    match rng.below(4) {
        0 => rng.below(300),    // dense low bytes -> Node256 / Node48
        1 => rng.below(70_000), // two dense bytes
        2 => (rng.below(5) << 56) | (rng.below(60) << 8) | rng.below(256), // clustered under long prefixes
        _ => rng.next(), // sparse -> lazy leaves, path compression
    }
}

fn diff_u64() -> Events {
    let (mut art, mut oracle, mut rng) = (Art::new(), BTreeMap::new(), Rng(0xA17));
    for step in 0..20_000u64 {
        let k = gen_u64(&mut rng);
        let kb = enc_u64(k);
        match rng.below(100) {
            0..=49 => assert_eq!(art.insert(&kb, step), oracle.insert(k, step)),
            50..=79 => assert_eq!(art.remove(&kb), oracle.remove(&k)),
            80..=89 => assert_eq!(art.get(&kb), oracle.get(&k).copied()),
            _ => {
                let (lo, hi) = (k, k.saturating_add(rng.below(3000)));
                let want: Vec<(Vec<u8>, u64)> = oracle
                    .range(lo..hi)
                    .map(|(k, v)| (enc_u64(*k).to_vec(), *v))
                    .collect();
                assert_eq!(
                    art.range(&enc_u64(lo), Some(&enc_u64(hi))),
                    want,
                    "range [{lo},{hi})"
                );
            }
        }
        assert_eq!(art.len, oracle.len());
        if step % 250 == 0 {
            art.check();
        }
    }
    art.check();
    let want: Vec<(Vec<u8>, u64)> = oracle
        .iter()
        .map(|(k, v)| (enc_u64(*k).to_vec(), *v))
        .collect();
    assert_eq!(art.iter_all(), want);
    println!(
        "u64 differential: {} keys alive, events {:?}",
        art.len, art.ev
    );
    art.ev.clone()
}

fn rand_word(rng: &mut Rng) -> String {
    let stems = [
        "user/",
        "user/admin/",
        "order/2024/",
        "order/2025/",
        "",
        "x",
    ];
    let mut s = stems[rng.below(stems.len() as u64) as usize].to_string();
    for _ in 0..rng.below(6) {
        s.push((b'a' + rng.below(6) as u8) as char);
    }
    s
}

fn diff_strings() {
    let (mut art, mut oracle, mut rng) = (Art::new(), BTreeMap::new(), Rng(0x5712));
    for step in 0..20_000u64 {
        let k = rand_word(&mut rng);
        let kb = enc_str(&k);
        match rng.below(100) {
            0..=44 => assert_eq!(art.insert(&kb, step), oracle.insert(k, step)),
            45..=74 => assert_eq!(art.remove(&kb), oracle.remove(&k)),
            75..=89 => assert_eq!(art.get(&kb), oracle.get(&k).copied()),
            90..=94 => {
                let hi = rand_word(&mut rng);
                if k < hi {
                    let want: Vec<(Vec<u8>, u64)> = oracle
                        .range(k.clone()..hi.clone())
                        .map(|(k, v)| (enc_str(k), *v))
                        .collect();
                    assert_eq!(art.range(&kb, Some(&enc_str(&hi))), want, "string range");
                }
            }
            _ => {
                let p = &k[..k.len().min(rng.below(7) as usize)];
                let want: Vec<(Vec<u8>, u64)> = oracle
                    .iter()
                    .filter(|(k, _)| k.starts_with(p))
                    .map(|(k, v)| (enc_str(k), *v))
                    .collect();
                assert_eq!(art.prefix_scan(p.as_bytes()), want, "prefix scan {p:?}");
            }
        }
        if step % 250 == 0 {
            art.check();
        }
    }
    art.check();
    println!(
        "string differential: {} keys alive, {} prefix splits",
        art.len, art.ev.prefix_splits
    );
}

/// Deterministic ladder: the layout changes exactly at the documented thresholds.
fn grow_shrink_ladder() {
    let mut art = Art::new();
    let cap_of = |a: &Art| match &a.nodes[a.root as usize] {
        Node::Inner(i) => i.body.cap(),
        _ => 0,
    };
    let key = |b: u8| [9u8, 9, 9, 9, 9, 9, 9, b]; // 7 shared bytes (one compressed prefix) + 1 branching byte
    for b in 0..=255u8 {
        art.insert(&key(b), b as u64);
        let want = match b as usize + 1 {
            ..=4 => 4,
            5..=16 => 16,
            17..=48 => 48,
            _ => 256,
        };
        if b >= 1 {
            assert_eq!(cap_of(&art), want, "after {} children", b as usize + 1);
        }
    }
    art.check();
    assert_eq!(art.ev.grows, [1, 1, 1]);
    for b in (1..=255u8).rev() {
        art.remove(&key(b));
        let n = b as usize;
        if n >= 2 {
            let want = match n {
                ..=3 => 4,
                4..=12 => 16,
                13..=37 => 48,
                _ => 256,
            };
            // Thresholds are hysteretic (shrink at 37/12/3): only assert the unambiguous bands.
            assert!(cap_of(&art) >= want || n == 0, "n={n} cap={}", cap_of(&art));
            art.check();
        }
    }
    assert_eq!(art.ev.shrinks, [1, 1, 1]);
    assert_eq!(art.len, 1, "one key left");
    assert!(
        matches!(art.nodes[art.root as usize], Node::Leaf { .. }),
        "last inner node collapsed into the leaf"
    );
    assert_eq!(art.get(&key(0)), Some(0));
}

fn encoding_tests() {
    let ints = [i64::MIN, -5_000_000_000, -1, 0, 1, 7, i64::MAX];
    assert!(ints.windows(2).all(|w| enc_i64(w[0]) < enc_i64(w[1])));
    let fl = [
        f64::NEG_INFINITY,
        -1e300,
        -2.5,
        -0.0001,
        0.0,
        0.0001,
        3.5,
        1e300,
        f64::INFINITY,
    ];
    assert!(fl.windows(2).all(|w| enc_f64(w[0]) < enc_f64(w[1])));
    let strs = ["", "a", "ab", "abc", "b"];
    assert!(strs.windows(2).all(|w| enc_str(w[0]) < enc_str(w[1])));
    let mut art = Art::new();
    for (i, s) in strs.iter().enumerate() {
        art.insert(&enc_str(s), i as u64);
    }
    assert_eq!(art.prefix_scan(b"ab").len(), 2, "ab and abc");
    assert_eq!(art.get(&enc_str("abd")), None);
}

fn comparison() {
    let mut rng = Rng(77);
    let n = 4_000;
    let mut keys: Vec<u64> = (0..n).map(|_| rng.next()).collect();
    keys.sort();
    keys.dedup();
    let mut dense: Vec<u64> = (0..n as u64).collect();
    println!(
        "memory and lookup comparison ({} sparse random u64 keys / {} dense sequential keys):",
        keys.len(),
        dense.len()
    );
    for (name, set) in [("sparse random", &keys), ("dense sequential", &dense)] {
        let (mut art, mut trie) = (Art::new(), PlainTrie::new());
        for (i, &k) in set.iter().enumerate() {
            art.insert(&enc_u64(k), i as u64);
            trie.insert(&enc_u64(k), i as u64);
        }
        art.check();
        let c = art.census();
        let btree_bytes = (set.len() as f64 * 16.0 / 0.69) as usize; // 8-byte key + 8-byte value at ln2 fill
        let (mut cmps, mut visited) = (0, 0);
        let t = Instant::now();
        for &k in set.iter() {
            assert!(sorted_array_get(set, k, &mut cmps));
            let (v, vis) = art.get_counting(&enc_u64(k));
            assert!(v.is_some());
            visited += vis;
            assert!(trie.get(&enc_u64(k)).is_some());
        }
        let _ = t.elapsed();
        let m = set.len() as f64;
        println!(
            "  {name:<17} ART {:>8} B ({:>5.1} B/key; n4 {} n16 {} n48 {} n256 {}; {:.1} inner nodes/lookup) | plain trie {:>9} B ({:>6.1} B/key) | B+tree est. {:>7} B ({:.1} cmps/lookup)",
            c.bytes,
            c.bytes as f64 / m,
            c.n4,
            c.n16,
            c.n48,
            c.n256,
            visited as f64 / m,
            trie.bytes(),
            trie.bytes() as f64 / m,
            btree_bytes,
            cmps as f64 / m
        );
        assert!(
            c.bytes - c.leaf_bytes < trie.bytes() - set.len() * 8,
            "adaptive inner nodes beat fixed 256-way nodes"
        );
        for &k in set.iter().take(50) {
            assert_eq!(
                art.get(&enc_u64(k.wrapping_add(1) ^ 0x5555_0000_0000_0000))
                    .is_some(),
                set.binary_search(&(k.wrapping_add(1) ^ 0x5555_0000_0000_0000))
                    .is_ok()
            );
        }
    }
    // Timing is machine dependent: printed, never asserted.
    let mut art = Art::new();
    let mut bt = BTreeMap::new();
    for &k in &keys {
        art.insert(&enc_u64(k), k);
        bt.insert(k, k);
    }
    let reps = 50;
    let t = Instant::now();
    let mut s = 0u64;
    for _ in 0..reps {
        for &k in &keys {
            s = s.wrapping_add(art.get(&enc_u64(k)).unwrap());
        }
    }
    let ta = t.elapsed();
    let t = Instant::now();
    for _ in 0..reps {
        for &k in &keys {
            s = s.wrapping_add(bt[&k]);
        }
    }
    let tb = t.elapsed();
    println!(
        "  lookup time over {} lookups: ART {:?}, std BTreeMap {:?} (checksum {s})",
        reps * keys.len(),
        ta,
        tb
    );
    dense.clear();
}

fn main() {
    encoding_tests();
    grow_shrink_ladder();
    println!(
        "ladder: Node4 -> 16 -> 48 -> 256 at 5/17/49 children, back down at 37/12/3, last node collapsed into its leaf"
    );
    let ev = diff_u64();
    assert!(
        ev.grows.iter().all(|&g| g > 0),
        "random ops exercised every growth step"
    );
    assert!(ev.collapses > 0 && ev.prefix_splits > 0);
    diff_strings();
    comparison();
    println!("all assertions passed");
}
