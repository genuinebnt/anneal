//! RECIPE 129 (trees): a trie and a compressed radix tree (Patricia) over byte strings, arena-based
//! LEVEL: intermediate
//! Use in: autocomplete, routers (HTTP paths, IP prefixes), symbol tables, dictionaries.   Run: cargo run --release --example trie_radix
#![allow(dead_code)]
//!
//! WHAT: two ordered maps keyed by byte strings. `Trie` has one node per byte; `Radix` stores whole labels on edges and only
//! keeps a node where the key set branches or a key ends, so a chain like "apple" -> "applesauce" costs 2 nodes instead of 10.
//! WHY: both give O(key length) lookups independent of how many keys are stored, ordered prefix scans (sorted DFS since children
//! are kept sorted by first byte), and longest-prefix match, which a hash map cannot do at all.
//! HOW: nodes live in a `Vec` and children are `usize` ids (arena_ids); freed slots go on a free list so ids stay stable. Radix insert
//! SPLITS an edge when a new key diverges mid-label; remove MERGES a node left with one child and no value back into its edge.
//! Verified by a differential test against `BTreeMap` (prefix query == range scan) on 5k seeded random strings.
//! COMMON USES: autocomplete and typeahead; HTTP route tables (`/api/v1/*`) and IP longest-prefix routing (the classic
//! radix/Patricia use); spell-check dictionaries; Linux kernel and many routers use radix-style trees for routing/page lookups.
//! Read: https://en.wikipedia.org/wiki/Radix_tree and https://en.wikipedia.org/wiki/Trie
use std::collections::BTreeMap;

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

// ---------------------------------------------------------------- plain trie
struct TNode<V> {
    kids: Vec<(u8, usize)>, // sorted by byte, so DFS yields sorted keys
    val: Option<V>,
}

struct Trie<V> {
    nodes: Vec<TNode<V>>,
    free: Vec<usize>,
}

impl<V> Trie<V> {
    fn new() -> Self {
        Trie {
            nodes: vec![TNode {
                kids: vec![],
                val: None,
            }],
            free: vec![],
        }
    }
    fn alloc(&mut self) -> usize {
        let n = TNode {
            kids: vec![],
            val: None,
        };
        if let Some(i) = self.free.pop() {
            self.nodes[i] = n;
            i
        } else {
            self.nodes.push(n);
            self.nodes.len() - 1
        }
    }
    fn live_nodes(&self) -> usize {
        self.nodes.len() - self.free.len()
    }
    fn child(&self, n: usize, b: u8) -> Option<usize> {
        let k = &self.nodes[n].kids;
        k.binary_search_by_key(&b, |&(c, _)| c).ok().map(|i| k[i].1)
    }
    fn insert(&mut self, key: &[u8], v: V) -> Option<V> {
        let mut cur = 0;
        for &b in key {
            cur = match self.child(cur, b) {
                Some(c) => c,
                None => {
                    let c = self.alloc();
                    let kids = &mut self.nodes[cur].kids;
                    let pos = kids.partition_point(|&(x, _)| x < b);
                    kids.insert(pos, (b, c));
                    c
                }
            };
        }
        self.nodes[cur].val.replace(v)
    }
    fn find(&self, key: &[u8]) -> Option<usize> {
        key.iter().try_fold(0, |n, &b| self.child(n, b))
    }
    fn get(&self, key: &[u8]) -> Option<&V> {
        self.nodes[self.find(key)?].val.as_ref()
    }
    fn remove(&mut self, key: &[u8]) -> Option<V> {
        let mut path = vec![0usize];
        for &b in key {
            path.push(self.child(*path.last().unwrap(), b)?);
        }
        let old = self.nodes[*path.last().unwrap()].val.take()?;
        // Prune upward while a node is useless (no value, no children).
        for i in (1..path.len()).rev() {
            let n = path[i];
            if self.nodes[n].val.is_some() || !self.nodes[n].kids.is_empty() {
                break;
            }
            self.nodes[path[i - 1]].kids.retain(|&(_, c)| c != n);
            self.free.push(n);
        }
        Some(old)
    }
    fn collect<'a>(&'a self, n: usize, acc: &mut Vec<u8>, out: &mut Vec<(Vec<u8>, &'a V)>) {
        if let Some(v) = &self.nodes[n].val {
            out.push((acc.clone(), v));
        }
        for &(b, c) in &self.nodes[n].kids {
            acc.push(b);
            self.collect(c, acc, out);
            acc.pop();
        }
    }
    /// All entries whose key starts with `prefix`, in sorted key order (autocomplete).
    fn with_prefix(&self, prefix: &[u8]) -> Vec<(Vec<u8>, &V)> {
        let mut out = vec![];
        if let Some(n) = self.find(prefix) {
            self.collect(n, &mut prefix.to_vec(), &mut out);
        }
        out
    }
    /// Longest stored key that is a prefix of `key`: (length, value).
    fn longest_prefix(&self, key: &[u8]) -> Option<(usize, &V)> {
        let mut best = self.nodes[0].val.as_ref().map(|v| (0, v));
        let mut n = 0;
        for (i, &b) in key.iter().enumerate() {
            let Some(c) = self.child(n, b) else { break };
            n = c;
            if let Some(v) = &self.nodes[n].val {
                best = Some((i + 1, v));
            }
        }
        best
    }
}

// ------------------------------------------------------------ radix (Patricia)
struct RNode<V> {
    edges: Vec<(Vec<u8>, usize)>, // sorted by first byte of label; first bytes are unique
    val: Option<V>,
}

struct Radix<V> {
    nodes: Vec<RNode<V>>,
    free: Vec<usize>,
}

fn lcp(a: &[u8], b: &[u8]) -> usize {
    a.iter().zip(b).take_while(|(x, y)| x == y).count()
}

impl<V> Radix<V> {
    fn new() -> Self {
        Radix {
            nodes: vec![RNode {
                edges: vec![],
                val: None,
            }],
            free: vec![],
        }
    }
    fn alloc(&mut self, val: Option<V>) -> usize {
        let n = RNode { edges: vec![], val };
        if let Some(i) = self.free.pop() {
            self.nodes[i] = n;
            i
        } else {
            self.nodes.push(n);
            self.nodes.len() - 1
        }
    }
    fn live_nodes(&self) -> usize {
        self.nodes.len() - self.free.len()
    }
    /// Ok(index) of the edge starting with byte `b`, or Err(insert position).
    fn edge_for(&self, n: usize, b: u8) -> Result<usize, usize> {
        self.nodes[n].edges.binary_search_by_key(&b, |(l, _)| l[0])
    }
    fn insert(&mut self, key: &[u8], v: V) -> Option<V> {
        let (mut cur, mut rest) = (0, key);
        loop {
            if rest.is_empty() {
                return self.nodes[cur].val.replace(v);
            }
            match self.edge_for(cur, rest[0]) {
                Err(pos) => {
                    let leaf = self.alloc(Some(v));
                    self.nodes[cur].edges.insert(pos, (rest.to_vec(), leaf));
                    return None;
                }
                Ok(i) => {
                    let (label, child) = self.nodes[cur].edges[i].clone();
                    let common = lcp(&label, rest);
                    if common < label.len() {
                        // Diverges mid-edge: split it with a new middle node.
                        let mid = self.alloc(None);
                        self.nodes[mid]
                            .edges
                            .push((label[common..].to_vec(), child));
                        self.nodes[cur].edges[i] = (label[..common].to_vec(), mid);
                        cur = mid;
                    } else {
                        cur = child;
                    }
                    rest = &rest[common..];
                }
            }
        }
    }
    /// Walk to the node where `key` ends exactly (whole labels only).
    fn find(&self, key: &[u8]) -> Option<usize> {
        let (mut cur, mut rest) = (0, key);
        while !rest.is_empty() {
            let i = self.edge_for(cur, rest[0]).ok()?;
            let (label, child) = &self.nodes[cur].edges[i];
            rest = rest.strip_prefix(label.as_slice())?;
            cur = *child;
        }
        Some(cur)
    }
    fn get(&self, key: &[u8]) -> Option<&V> {
        self.nodes[self.find(key)?].val.as_ref()
    }
    fn remove(&mut self, key: &[u8]) -> Option<V> {
        let (mut cur, mut rest) = (0, key);
        let mut path: Vec<(usize, usize)> = vec![]; // (parent, edge index)
        while !rest.is_empty() {
            let i = self.edge_for(cur, rest[0]).ok()?;
            let (label, child) = &self.nodes[cur].edges[i];
            rest = rest.strip_prefix(label.as_slice())?;
            path.push((cur, i));
            cur = *child;
        }
        let old = self.nodes[cur].val.take()?;
        while let Some(&(parent, i)) = path.last() {
            let n = &self.nodes[cur];
            if n.val.is_some() || n.edges.len() > 1 {
                break;
            }
            if n.edges.is_empty() {
                // Dead leaf: unlink it; the parent may now need merging too.
                self.nodes[parent].edges.remove(i);
                self.free.push(cur);
                path.pop();
                cur = parent;
            } else {
                // One child, no value: fuse the two labels into one edge.
                let (tail, grandchild) = self.nodes[cur].edges[0].clone();
                self.nodes[parent].edges[i].0.extend_from_slice(&tail);
                self.nodes[parent].edges[i].1 = grandchild;
                self.free.push(cur);
                break;
            }
        }
        Some(old)
    }
    fn collect<'a>(&'a self, n: usize, acc: &mut Vec<u8>, out: &mut Vec<(Vec<u8>, &'a V)>) {
        if let Some(v) = &self.nodes[n].val {
            out.push((acc.clone(), v));
        }
        for (label, c) in &self.nodes[n].edges {
            acc.extend_from_slice(label);
            self.collect(*c, acc, out);
            acc.truncate(acc.len() - label.len());
        }
    }
    fn with_prefix(&self, prefix: &[u8]) -> Vec<(Vec<u8>, &V)> {
        let (mut cur, mut rest) = (0, prefix);
        let mut acc = vec![];
        while !rest.is_empty() {
            let Ok(i) = self.edge_for(cur, rest[0]) else {
                return vec![];
            };
            let (label, child) = &self.nodes[cur].edges[i];
            // The prefix may end in the middle of a label: the whole subtree still matches.
            if !(rest.starts_with(label) || label.starts_with(rest)) {
                return vec![];
            }
            acc.extend_from_slice(label);
            rest = rest.get(label.len()..).unwrap_or(&[]);
            cur = *child;
        }
        let mut out = vec![];
        self.collect(cur, &mut acc, &mut out);
        out
    }
    fn longest_prefix(&self, key: &[u8]) -> Option<(usize, &V)> {
        let mut best = self.nodes[0].val.as_ref().map(|v| (0, v));
        let (mut cur, mut rest) = (0, key);
        while !rest.is_empty() {
            let Ok(i) = self.edge_for(cur, rest[0]) else {
                break;
            };
            let (label, child) = &self.nodes[cur].edges[i];
            let Some(r) = rest.strip_prefix(label.as_slice()) else {
                break;
            };
            rest = r;
            cur = *child;
            if let Some(v) = &self.nodes[cur].val {
                best = Some((key.len() - rest.len(), v));
            }
        }
        best
    }
}

// ------------------------------------------------------------------ the demos
fn demo_autocomplete() {
    let mut t = Radix::new();
    for (i, w) in ["car", "card", "care", "careful", "cat", "dog", "do"]
        .iter()
        .enumerate()
    {
        t.insert(w.as_bytes(), i);
    }
    let words: Vec<String> = t
        .with_prefix(b"car")
        .into_iter()
        .map(|(k, _)| String::from_utf8(k).unwrap())
        .collect();
    println!("autocomplete 'car' -> {words:?}");
    assert_eq!(words, ["car", "card", "care", "careful"]);
}

fn demo_routing() {
    let mut routes = Radix::new();
    for (p, h) in [
        ("/", "root"),
        ("/api/", "api"),
        ("/api/v1/", "v1"),
        ("/api/v1/users/", "users"),
        ("/static/", "static"),
    ] {
        routes.insert(p.as_bytes(), h);
    }
    for path in [
        "/api/v1/users/42",
        "/api/v1/orders",
        "/api/v2/x",
        "/static/a.css",
        "/zzz",
    ] {
        let (n, h) = routes.longest_prefix(path.as_bytes()).unwrap();
        println!("route {path:<18} -> {h} (matched {:?})", &path[..n]);
    }
    assert_eq!(
        routes.longest_prefix(b"/api/v1/orders").map(|(_, h)| *h),
        Some("v1")
    );
    assert_eq!(
        routes.longest_prefix(b"/api/v2/x").map(|(_, h)| *h),
        Some("api")
    );
    // IPv4 routing: the 4 octets ARE the key bytes, so /8, /16 and /24 prefixes are byte-aligned.
    let mut ip = Trie::new();
    ip.insert(&[10], "10/8 corp");
    ip.insert(&[10, 1], "10.1/16 dc1");
    ip.insert(&[10, 1, 2], "10.1.2/24 rack");
    assert_eq!(
        ip.longest_prefix(&[10, 1, 2, 3]).unwrap().1,
        &"10.1.2/24 rack"
    );
    assert_eq!(ip.longest_prefix(&[10, 9, 9, 9]).unwrap().1, &"10/8 corp");
    assert!(ip.longest_prefix(&[11, 0, 0, 1]).is_none());
}

fn random_key(r: &mut Rng) -> Vec<u8> {
    let len = r.below(9);
    (0..len).map(|_| b'a' + r.below(4) as u8).collect()
}

fn check_all(t: &Trie<usize>, rx: &Radix<usize>, m: &BTreeMap<Vec<u8>, usize>, r: &mut Rng) {
    for _ in 0..300 {
        let p = random_key(r);
        let want: Vec<(Vec<u8>, usize)> = m
            .range(p.clone()..)
            .take_while(|(k, _)| k.starts_with(&p))
            .map(|(k, v)| (k.clone(), *v))
            .collect();
        let a: Vec<_> = t
            .with_prefix(&p)
            .into_iter()
            .map(|(k, v)| (k, *v))
            .collect();
        let b: Vec<_> = rx
            .with_prefix(&p)
            .into_iter()
            .map(|(k, v)| (k, *v))
            .collect();
        assert_eq!(a, want, "trie prefix {p:?}");
        assert_eq!(b, want, "radix prefix {p:?}");
        assert_eq!(t.get(&p), m.get(&p));
        assert_eq!(rx.get(&p), m.get(&p));
        // Longest stored prefix of p, via the oracle.
        let lp = (0..=p.len())
            .rev()
            .find(|&l| m.contains_key(&p[..l]))
            .map(|l| (l, m[&p[..l]]));
        assert_eq!(t.longest_prefix(&p).map(|(l, v)| (l, *v)), lp);
        assert_eq!(rx.longest_prefix(&p).map(|(l, v)| (l, *v)), lp);
    }
}

fn differential() {
    let mut r = Rng(0x9E3779B97F4A7C15);
    let (mut t, mut rx, mut m) = (Trie::new(), Radix::new(), BTreeMap::new());
    for i in 0..5000 {
        let k = random_key(&mut r);
        assert_eq!(t.insert(&k, i), m.get(&k).copied());
        assert_eq!(rx.insert(&k, i), m.insert(k, i));
    }
    check_all(&t, &rx, &m, &mut r);
    println!(
        "{} keys: trie nodes = {}, radix nodes = {}",
        m.len(),
        t.live_nodes(),
        rx.live_nodes()
    );
    assert!(rx.live_nodes() < t.live_nodes());
    // Remove about 2/3 of the keys (plus some absent ones), re-verify, then drain everything.
    for _ in 0..8000 {
        let k = random_key(&mut r);
        assert_eq!(t.remove(&k), m.get(&k).copied());
        assert_eq!(rx.remove(&k), m.remove(&k));
    }
    check_all(&t, &rx, &m, &mut r);
    println!(
        "after removals: {} keys, trie nodes = {}, radix nodes = {}",
        m.len(),
        t.live_nodes(),
        rx.live_nodes()
    );
    for k in m.keys().cloned().collect::<Vec<_>>() {
        t.remove(&k);
        rx.remove(&k);
    }
    assert_eq!(
        (t.live_nodes(), rx.live_nodes()),
        (1, 1),
        "everything pruned back to the root"
    );
    println!("differential test vs BTreeMap: OK");
}

fn main() {
    demo_autocomplete();
    demo_routing();
    differential();
}
