//! RECIPE 715 (trees): the B^epsilon-tree: a B-tree whose internal nodes carry a MESSAGE BUFFER, trading a little read cost for far fewer writes.
//! LEVEL: advanced
//! Use in: write-heavy ordered stores on disks/SSDs, file systems and databases with random-insert workloads, anywhere B+tree write amplification hurts.   Run: cargo run --release --example b_epsilon_tree
#![allow(dead_code)]
//!
//! WHAT: a node has room for B records. A fraction of it, F = B^epsilon, holds PIVOTS (fanout F); the rest, B - F, is a BUFFER of
//! pending messages (Put / Delete / Upsert-add-delta) addressed to keys below this node. Updates never descend to a leaf: they are
//! appended to the ROOT's buffer. When a buffer overflows, the child receiving the most messages gets a whole BATCH pushed into it
//! (and may overflow in turn); a leaf finally APPLIES its messages. A message therefore pays I/O once per level, but shares each
//! node write with ~B/F other messages. That is the whole trick: amortised writes per insert are O(F * log_F(N) / B) node writes,
//! versus 1 page write per insert for a B+tree that cannot keep its leaves cached.
//! QUERIES: walk root to leaf collecting the matching messages from each buffer; a message in a higher node is NEWER than anything
//! below it, so the answer is the leaf value with the messages applied deepest-first, top level last ("newest wins"). The read cost
//! is still one node per level (a bit more levels than a B+tree because F < B, i.e. log_F N instead of log_B N).
//! epsilon is the knob: epsilon = 1 is a B+tree (no buffer), epsilon -> 0 gives huge buffers, fast inserts, slow-ish reads
//! (epsilon = 0.5 is the usual sweet spot: reads cost 2x a B-tree, inserts cost ~sqrt(B) times less I/O).
//! HOW: nodes live in a `Vec` arena. Buffers keep messages in arrival order, which IS recency, so a stable partition when
//! flushing preserves it. A flush may split a leaf into several pieces (a batch can add more than one node's worth) and the
//! parent absorbs the new pivots and may split too; a root split grows the tree. Deletes are tombstones and leaves are not merged
//! (lazy, as in most production trees; the verifier only demands the upper fill bounds).
//! Verified by a 20k-op differential test per parameter set (several B and epsilon) against `BTreeMap` with Put/Delete/Upsert and
//! point reads checked at every get, an invariant checker (uniform depth, key ranges respected by buffers and pivots, fanout and
//! buffer caps), and an I/O cost model asserting node writes per random insert fall far below the B+tree's.
//! COMMON USES: TokuDB / PerconaFT and BetrFS (a file system built on it) use B^epsilon-trees; the same buffering idea appears in
//! fractal-tree indexes and, conceptually, in LSM engines which buffer in memtables and compact in batches.
//! Course: CMU 15-746 (Storage Systems) write-optimised data structures; CMU 15-445 lectures on B+tree and LSM storage for the baselines.
//! Read: Jannen et al., "BetrFS: A Right-Optimized Write-Optimized File System" (FAST 2015): https://www.usenix.org/conference/fast15/technical-sessions/presentation/jannen
//! Read: Bender et al., "An Introduction to B^epsilon-trees and Write-Optimization" (USENIX ;login:, Oct 2015): https://www.usenix.org/system/files/login/articles/login_oct15_05_bender.pdf
use std::collections::BTreeMap;

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

#[derive(Clone, Copy, Debug, PartialEq)]
enum Msg {
    Put(i64),
    Del,
    Upsert(i64), // blind "add delta" with no read of the old value; absent counts as 0
}

/// The single definition of what a message means; leaf application, query merging and the oracle all go through it.
fn apply(cur: Option<i64>, m: Msg) -> Option<i64> {
    match m {
        Msg::Put(v) => Some(v),
        Msg::Del => None,
        Msg::Upsert(d) => Some(cur.unwrap_or(0) + d),
    }
}

/// One node shape for both kinds. Leaf: keys/vals. Internal: pivots/kids/buf.
struct Node {
    leaf: bool,
    keys: Vec<u64>,
    vals: Vec<i64>,
    pivots: Vec<u64>, // kids[i] covers keys in [pivots[i-1], pivots[i])
    kids: Vec<usize>,
    buf: Vec<(u64, Msg)>, // arrival order == recency order (oldest first)
}

impl Node {
    fn leaf() -> Node {
        Node {
            leaf: true,
            keys: vec![],
            vals: vec![],
            pivots: vec![],
            kids: vec![],
            buf: vec![],
        }
    }
    fn internal() -> Node {
        Node {
            leaf: false,
            ..Node::leaf()
        }
    }
    fn child_for(&self, key: u64) -> usize {
        self.pivots.partition_point(|p| *p <= key)
    }
}

#[derive(Default, Clone, Copy, Debug)]
struct Stats {
    node_writes: u64,
    flushes: u64,
}

/// Split `len` items into `parts` nearly equal ranges.
fn even_chunks(len: usize, parts: usize) -> Vec<(usize, usize)> {
    (0..parts)
        .map(|i| (len * i / parts, len * (i + 1) / parts))
        .collect()
}

struct BeTree {
    nodes: Vec<Node>,
    root: usize,
    b: usize,       // node capacity in records
    fanout: usize,  // F = B^epsilon
    buf_cap: usize, // B - F
    stats: Stats,
}

type Splits = Vec<(u64, usize)>; // (separator key, new right sibling) in key order

impl BeTree {
    fn new(b: usize, eps: f64) -> BeTree {
        let fanout = ((b as f64).powf(eps).round() as usize).clamp(4, b / 2);
        BeTree {
            nodes: vec![Node::leaf()],
            root: 0,
            b,
            fanout,
            buf_cap: b - fanout,
            stats: Stats::default(),
        }
    }

    fn submit(&mut self, key: u64, m: Msg) {
        let splits = self.push_to(self.root, vec![(key, m)]);
        if !splits.is_empty() {
            // The root split: grow one level. The root is assumed cached, so this costs no counted write.
            let mut root = Node::internal();
            root.kids.push(self.root);
            for (sep, id) in splits {
                root.pivots.push(sep);
                root.kids.push(id);
            }
            self.nodes.push(root);
            self.root = self.nodes.len() - 1;
        }
    }

    /// Deliver a batch (oldest first) to node `n`. Returns the splits of `n` the caller must absorb.
    fn push_to(&mut self, n: usize, msgs: Vec<(u64, Msg)>) -> Splits {
        if self.nodes[n].leaf {
            for (k, m) in msgs {
                self.apply_to_leaf(n, k, m);
            }
            return if self.nodes[n].keys.len() > self.b {
                self.split_leaf(n)
            } else {
                vec![]
            };
        }
        self.nodes[n].buf.extend(msgs);
        if self.nodes[n].buf.len() > self.buf_cap {
            self.flush(n)
        } else {
            vec![]
        }
    }

    fn apply_to_leaf(&mut self, n: usize, k: u64, m: Msg) {
        let node = &mut self.nodes[n];
        match node.keys.binary_search(&k) {
            Ok(i) => match apply(Some(node.vals[i]), m) {
                Some(v) => node.vals[i] = v,
                None => {
                    node.keys.remove(i);
                    node.vals.remove(i);
                }
            },
            Err(i) => {
                if let Some(v) = apply(None, m) {
                    node.keys.insert(i, k);
                    node.vals.insert(i, v);
                }
            }
        }
    }

    /// Push batches down until the buffer fits. Greedy rule: always pick the child with the MOST pending messages,
    /// because the node write is the cost and the batch size is the benefit.
    fn flush(&mut self, n: usize) -> Splits {
        while self.nodes[n].buf.len() > self.buf_cap {
            let node = &self.nodes[n];
            let mut counts = vec![0usize; node.kids.len()];
            for (k, _) in &node.buf {
                counts[node.child_for(*k)] += 1;
            }
            let idx = (0..counts.len())
                .max_by_key(|&i| (counts[i], std::cmp::Reverse(i)))
                .unwrap();
            let buf = std::mem::take(&mut self.nodes[n].buf);
            let (mine, rest): (Vec<_>, Vec<_>) = buf
                .into_iter()
                .partition(|(k, _)| self.nodes[n].child_for(*k) == idx);
            self.nodes[n].buf = rest;
            let child = self.nodes[n].kids[idx];
            self.stats.flushes += 1;
            self.stats.node_writes += 2; // parent buffer shrank, child changed
            let splits = self.push_to(child, mine);
            for (j, (sep, id)) in splits.into_iter().enumerate() {
                self.nodes[n].pivots.insert(idx + j, sep);
                self.nodes[n].kids.insert(idx + 1 + j, id);
                self.stats.node_writes += 1; // each new sibling is a fresh node
            }
        }
        if self.nodes[n].kids.len() > self.fanout {
            self.split_internal(n)
        } else {
            vec![]
        }
    }

    fn split_leaf(&mut self, n: usize) -> Splits {
        let len = self.nodes[n].keys.len();
        let parts = len.div_ceil(self.b * 3 / 4); // aim for ~75% full pieces; always >= 2 and each >= B/2
        let (keys, vals) = (
            std::mem::take(&mut self.nodes[n].keys),
            std::mem::take(&mut self.nodes[n].vals),
        );
        let mut out = vec![];
        for (i, (s, e)) in even_chunks(len, parts).into_iter().enumerate() {
            let mut piece = Node::leaf();
            piece.keys = keys[s..e].to_vec();
            piece.vals = vals[s..e].to_vec();
            if i == 0 {
                self.nodes[n] = piece;
            } else {
                out.push((piece.keys[0], self.nodes.len()));
                self.nodes.push(piece);
            }
        }
        out
    }

    fn split_internal(&mut self, n: usize) -> Splits {
        let c = self.nodes[n].kids.len();
        let parts = c.div_ceil(self.fanout * 3 / 4);
        let old = std::mem::replace(&mut self.nodes[n], Node::internal());
        let chunks = even_chunks(c, parts);
        // Pivot at the end of piece j (index e-1) moves UP as the separator; pieces keep the pivots strictly inside them.
        let seps: Vec<u64> = chunks[..parts - 1]
            .iter()
            .map(|&(_, e)| old.pivots[e - 1])
            .collect();
        let mut bufs: Vec<Vec<(u64, Msg)>> = vec![vec![]; parts];
        for (k, m) in old.buf {
            bufs[seps.partition_point(|s| *s <= k)].push((k, m)); // stable: recency order survives
        }
        let mut out = vec![];
        for (i, (s, e)) in chunks.into_iter().enumerate() {
            let mut piece = Node::internal();
            piece.kids = old.kids[s..e].to_vec();
            piece.pivots = old.pivots[s..e - 1].to_vec();
            piece.buf = std::mem::take(&mut bufs[i]);
            if i == 0 {
                self.nodes[n] = piece;
            } else {
                out.push((seps[i - 1], self.nodes.len()));
                self.nodes.push(piece);
            }
        }
        out
    }

    /// Point read. Returns (value, nodes read). Messages from higher levels are newer, so apply deepest level first.
    fn get(&self, key: u64) -> (Option<i64>, usize) {
        let mut levels: Vec<Vec<Msg>> = vec![];
        let (mut n, mut reads) = (self.root, 0);
        loop {
            reads += 1;
            let node = &self.nodes[n];
            if node.leaf {
                let base = node.keys.binary_search(&key).ok().map(|i| node.vals[i]);
                let v = levels
                    .iter()
                    .rev()
                    .flatten()
                    .fold(base, |cur, &m| apply(cur, m));
                return (v, reads);
            }
            levels.push(
                node.buf
                    .iter()
                    .filter(|(k, _)| *k == key)
                    .map(|(_, m)| *m)
                    .collect(),
            );
            n = node.kids[node.child_for(key)];
        }
    }

    /// Logical contents: children first, then this node's (newer) buffer on top.
    fn dump(&self, n: usize) -> BTreeMap<u64, i64> {
        let node = &self.nodes[n];
        if node.leaf {
            return node
                .keys
                .iter()
                .cloned()
                .zip(node.vals.iter().cloned())
                .collect();
        }
        let mut map = BTreeMap::new();
        for &c in &node.kids {
            map.extend(self.dump(c));
        }
        for &(k, m) in &node.buf {
            match apply(map.get(&k).copied(), m) {
                Some(v) => {
                    map.insert(k, v);
                }
                None => {
                    map.remove(&k);
                }
            }
        }
        map
    }

    fn height(&self) -> usize {
        let (mut h, mut n) = (1, self.root);
        while !self.nodes[n].leaf {
            n = self.nodes[n].kids[0];
            h += 1;
        }
        h
    }

    fn check(&self) {
        self.check_node(self.root, 0, u64::MAX, true);
    }

    fn check_node(&self, n: usize, lo: u64, hi: u64, _is_root: bool) -> usize {
        let node = &self.nodes[n];
        if node.leaf {
            assert!(node.keys.len() <= self.b, "leaf over capacity");
            assert!(
                node.keys.windows(2).all(|w| w[0] < w[1]),
                "leaf keys unsorted"
            );
            assert!(
                node.keys.iter().all(|&k| k >= lo && k < hi),
                "leaf key outside its range"
            );
            return 1;
        }
        assert!(node.buf.len() <= self.buf_cap, "buffer over capacity");
        assert!(
            node.kids.len() <= self.fanout && node.kids.len() >= 2,
            "bad fanout {}",
            node.kids.len()
        );
        assert_eq!(node.pivots.len() + 1, node.kids.len());
        assert!(
            node.pivots.windows(2).all(|w| w[0] < w[1]),
            "pivots unsorted"
        );
        assert!(
            node.buf.iter().all(|(k, _)| *k >= lo && *k < hi),
            "buffered message outside the node's key range"
        );
        let mut depth = 0;
        for (i, &c) in node.kids.iter().enumerate() {
            let clo = if i == 0 { lo } else { node.pivots[i - 1] };
            let chi = if i == node.pivots.len() {
                hi
            } else {
                node.pivots[i]
            };
            let d = self.check_node(c, clo, chi, false);
            assert!(depth == 0 || depth == d, "leaves at different depths");
            depth = d;
        }
        depth + 1
    }
}

fn differential(b: usize, eps: f64, seed: u64) {
    let mut rng = Rng(seed);
    let mut t = BeTree::new(b, eps);
    let mut oracle: BTreeMap<u64, i64> = BTreeMap::new();
    let mut gets = 0;
    for step in 0..20_000 {
        let k = rng.below(3000);
        let r = rng.below(100);
        let m = if r < 35 {
            Msg::Put(rng.below(1000) as i64)
        } else if r < 60 {
            Msg::Del
        } else if r < 85 {
            Msg::Upsert(rng.below(21) as i64 - 10)
        } else {
            let (got, _) = t.get(k);
            assert_eq!(
                got,
                oracle.get(&k).copied(),
                "B={b} eps={eps}: get({k}) wrong at step {step}"
            );
            gets += 1;
            continue;
        };
        t.submit(k, m);
        match apply(oracle.get(&k).copied(), m) {
            Some(v) => {
                oracle.insert(k, v);
            }
            None => {
                oracle.remove(&k);
            }
        }
        if step % 100 == 0 {
            t.check();
        }
    }
    t.check();
    assert_eq!(
        t.dump(t.root),
        oracle,
        "B={b} eps={eps}: logical contents differ"
    );
    for k in 0..3000 {
        assert_eq!(t.get(k).0, oracle.get(&k).copied());
    }
    println!(
        "B={b:<3} eps={eps:<4} F={:<3} buf={:<3} height={} nodes={} flushes={} gets_checked={gets} live_keys={}",
        t.fanout,
        t.buf_cap,
        t.height(),
        t.nodes.len(),
        t.stats.flushes,
        oracle.len()
    );
}

// ---------------------------------------------------------------------------------------------------------------------
// I/O cost model. Unit: a "node write" is one B-record page written. Top-level cache assumed for the root only.
// ---------------------------------------------------------------------------------------------------------------------

/// B+tree leaf layer only: that is where random inserts pay (internal nodes change only on splits).
fn bplus_writes(n: u64, b: usize, rng: &mut Rng) -> (f64, usize) {
    let mut leaves: Vec<(u64, usize)> = vec![(0, 0)]; // (smallest key, count)
    let mut writes = 0u64;
    for _ in 0..n {
        let k = rng.next();
        let i = leaves.partition_point(|l| l.0 <= k) - 1;
        leaves[i].1 += 1;
        writes += 1; // the whole leaf page is rewritten for one record
        if leaves[i].1 > b {
            let half = leaves[i].1 / 2;
            leaves[i].1 -= half;
            leaves.insert(i + 1, (k, half)); // the key value only needs to be a valid separator for the model
            writes += 2; // new sibling + parent
        }
    }
    let height = 1
        + ((leaves.len() as f64).ln() / (0.7 * b as f64).ln())
            .ceil()
            .max(1.0) as usize;
    (writes as f64 / n as f64, height)
}

/// Leveled LSM-lite counting records rewritten by compactions (size ratio T, memtable M records).
fn lsm_writes(n: u64, m: u64, t: u64) -> (f64, usize) {
    let mut levels: Vec<u64> = vec![];
    let mut written = 0u64;
    for _ in 0..n / m {
        written += m; // memtable flush
        let mut carry = m;
        let mut i = 0;
        loop {
            if i == levels.len() {
                levels.push(0);
            }
            written += levels[i] + carry; // merging rewrites both inputs
            levels[i] += carry;
            if levels[i] <= m * t.pow(i as u32 + 1) {
                break;
            }
            carry = levels[i];
            levels[i] = 0;
            i += 1;
        }
    }
    (
        (written + n) as f64 / n as f64,
        levels.iter().filter(|&&s| s > 0).count(),
    ) // +n: the write-ahead log
}

fn cost_table() {
    let (n, b) = (300_000u64, 1024usize);
    println!("\n-- amplification, N={n} random inserts, node = {b} records --");
    println!(
        "{:<24} {:>14} {:>16} {:>14}",
        "structure", "node writes/ins", "write amp (rec)", "read amp (pgs)"
    );
    let mut rng = Rng(0xB0B);
    let (bw, bh) = bplus_writes(n, b, &mut rng);
    println!(
        "{:<24} {:>14.3} {:>16.0} {:>14}",
        "B+tree (eps=1)",
        bw,
        bw * b as f64,
        bh
    );
    let mut be_half = None;
    for eps in [0.3, 0.5, 0.7] {
        let mut t = BeTree::new(b, eps);
        let mut rng = Rng(0xB0B);
        for i in 0..n {
            t.submit(rng.next(), Msg::Put(i as i64));
        }
        let w = t.stats.node_writes as f64 / n as f64;
        let mut reads = 0;
        for _ in 0..2000 {
            reads += t.get(rng.next()).1;
        }
        println!(
            "{:<24} {:>14.3} {:>16.1} {:>14.1}",
            format!("B^eps eps={eps} (F={})", t.fanout),
            w,
            w * b as f64,
            reads as f64 / 2000.0
        );
        assert_eq!(
            reads,
            2000 * t.height(),
            "every get reads exactly one node per level"
        );
        if eps == 0.5 {
            be_half = Some(w);
        }
    }
    let (lw, lh) = lsm_writes(n, 16_384, 10);
    println!(
        "{:<24} {:>14.3} {:>16.1} {:>14}",
        "LSM-lite (T=10)",
        lw / b as f64,
        lw,
        lh + 1
    );
    let w = be_half.unwrap();
    assert!(
        w < bw / 4.0,
        "B^eps (eps=0.5) should write at least 4x fewer nodes per insert than a B+tree: {w} vs {bw}"
    );
    println!(
        "B^eps(0.5) writes {:.1}x fewer nodes per random insert than the B+tree; reads cost more levels (read amp) in exchange.",
        bw / w
    );
}

fn main() {
    println!(
        "-- differential tests against BTreeMap (Put / Delete / Upsert, tombstones, 20k ops each) --"
    );
    differential(16, 0.5, 0xE1);
    differential(32, 0.3, 0xE2);
    differential(64, 0.7, 0xE3);
    differential(8, 0.5, 0xE4);
    cost_table();
    println!("all B^epsilon-tree checks passed");
}
