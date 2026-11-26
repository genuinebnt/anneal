//! RECIPE 128 (data-structures): skip list: a probabilistically balanced ordered map (arena, no unsafe).
//! LEVEL: advanced
//! Use in: ordered in-memory indexes, LSM memtables, sorted sets.   Run: cargo run --release --example skiplist
//!
//! WHAT: a sorted linked list with express lanes. Every node has a random "tower" height: level 1
//! with prob 3/4, level 2 with prob 3/16, ... (promotion probability p = 1/4). Level-0 links form the
//! full sorted list; higher links skip ahead. Search starts at the top lane, moves right while the
//! next key is smaller, drops down a level otherwise: expected O(log n) steps, no rebalancing.
//! HOW (Rust): nodes live in a `Vec<Option<Node>>` arena, links are `usize` indices (NIL = usize::MAX),
//! freed slots are recycled via a free list. This sidesteps the ownership problems of a graph of
//! pointers without `unsafe`/`Rc<RefCell>`. `head` holds the forward links of a virtual head node.
//! INSERT/REMOVE: first collect, per level, the last node before the key ("predecessors"), then
//! splice the node in or out at exactly the levels it occupies.
//! WHY SKIP LISTS ARE POPULAR: balancing a tree means rotations that touch many nodes; a skip list
//! insert only rewires a few local pointers, which makes lock-free and fine-grained concurrent
//! versions far simpler (Java's ConcurrentSkipListMap is one). They also give ordered iteration and
//! cheap range scans by walking level 0.
//!
//! COMMON USES: the default memtable in LevelDB and RocksDB is a skip list (writes land in a sorted
//! in-memory structure that is later flushed as a sorted file; LevelDB's design lets readers proceed
//! without locks while a single writer inserts); Redis sorted sets (ZSET) combine a skip list with a
//! hash table; in-memory ordered maps and priority/time-ordered indexes.
//! Read: https://en.wikipedia.org/wiki/Skip_list
#![allow(dead_code)]
use std::collections::BTreeMap;

const NIL: usize = usize::MAX;
const MAX_LEVEL: usize = 24;

struct Node<K, V> {
    key: K,
    val: V,
    next: Vec<usize>, // next[l] = successor at level l; tower height = next.len()
}

struct SkipList<K, V> {
    head: Vec<usize>, // forward links of the virtual head, one per active level
    nodes: Vec<Option<Node<K, V>>>,
    free: Vec<usize>,
    len: usize,
    rng: u64,
}

impl<K: Ord, V> SkipList<K, V> {
    fn new(seed: u64) -> Self {
        SkipList {
            head: vec![NIL],
            nodes: Vec::new(),
            free: Vec::new(),
            len: 0,
            rng: seed | 1,
        }
    }
    fn next_rand(&mut self) -> u64 {
        self.rng ^= self.rng << 13;
        self.rng ^= self.rng >> 7;
        self.rng ^= self.rng << 17;
        self.rng
    }
    /// Height h with P(h >= l+1) = (1/4)^l. Two random bits are zero => promote.
    fn random_level(&mut self) -> usize {
        let mut level = 1;
        while level < MAX_LEVEL && (self.next_rand() >> 33) & 3 == 0 {
            level += 1;
        }
        level
    }
    fn node(&self, i: usize) -> &Node<K, V> {
        self.nodes[i].as_ref().expect("live node")
    }
    /// Successor of `pred` (NIL = head) at `level`.
    fn next_of(&self, pred: usize, level: usize) -> usize {
        if pred == NIL {
            self.head[level]
        } else {
            self.node(pred).next[level]
        }
    }
    fn set_next(&mut self, pred: usize, level: usize, to: usize) {
        if pred == NIL {
            self.head[level] = to;
        } else {
            self.nodes[pred].as_mut().unwrap().next[level] = to;
        }
    }
    /// Per level, the last node whose key is < `key` (NIL = head).
    fn predecessors(&self, key: &K) -> Vec<usize> {
        let mut preds = vec![NIL; self.head.len()];
        let mut cur = NIL;
        for level in (0..self.head.len()).rev() {
            loop {
                let nx = self.next_of(cur, level);
                if nx != NIL && self.node(nx).key < *key {
                    cur = nx; // move right along this lane
                } else {
                    break; // next is too big: drop down a level
                }
            }
            preds[level] = cur;
        }
        preds
    }
    /// Index of the first node with key >= `key`, or NIL.
    fn lower_bound(&self, key: &K) -> usize {
        let preds = self.predecessors(key);
        self.next_of(preds[0], 0)
    }
    fn get(&self, key: &K) -> Option<&V> {
        let i = self.lower_bound(key);
        (i != NIL && self.node(i).key == *key).then(|| &self.node(i).val)
    }
    fn insert(&mut self, key: K, val: V) -> Option<V> {
        let mut preds = self.predecessors(&key);
        let cand = self.next_of(preds[0], 0);
        if cand != NIL && self.node(cand).key == key {
            return Some(std::mem::replace(
                &mut self.nodes[cand].as_mut().unwrap().val,
                val,
            ));
        }
        let level = self.random_level();
        while self.head.len() < level {
            self.head.push(NIL); // grow a new top lane; its predecessor is the head
            preds.push(NIL);
        }
        let next: Vec<usize> = (0..level).map(|l| self.next_of(preds[l], l)).collect();
        let node = Node { key, val, next };
        let idx = match self.free.pop() {
            Some(i) => {
                self.nodes[i] = Some(node);
                i
            }
            None => {
                self.nodes.push(Some(node));
                self.nodes.len() - 1
            }
        };
        for l in 0..level {
            self.set_next(preds[l], l, idx);
        }
        self.len += 1;
        None
    }
    fn remove(&mut self, key: &K) -> Option<V> {
        let preds = self.predecessors(key);
        let target = self.next_of(preds[0], 0);
        if target == NIL || self.node(target).key != *key {
            return None;
        }
        let node = self.nodes[target].take().unwrap();
        for l in 0..node.next.len() {
            if self.next_of(preds[l], l) == target {
                self.set_next(preds[l], l, node.next[l]); // bypass the node on each lane
            }
        }
        self.free.push(target);
        self.len -= 1;
        while self.head.len() > 1 && *self.head.last().unwrap() == NIL {
            self.head.pop(); // drop empty top lanes
        }
        Some(node.val)
    }
    fn iter(&self) -> Iter<'_, K, V> {
        Iter {
            list: self,
            cur: self.head[0],
            hi: None,
        }
    }
    /// Entries with lo <= key < hi, in order.
    fn range<'a>(&'a self, lo: &K, hi: &'a K) -> Iter<'a, K, V> {
        Iter {
            list: self,
            cur: self.lower_bound(lo),
            hi: Some(hi),
        }
    }
    fn height(&self) -> usize {
        self.head.len()
    }
    fn avg_tower(&self) -> f64 {
        let total: usize = self.nodes.iter().flatten().map(|n| n.next.len()).sum();
        total as f64 / self.len.max(1) as f64
    }
}

struct Iter<'a, K, V> {
    list: &'a SkipList<K, V>,
    cur: usize,
    hi: Option<&'a K>,
}

impl<'a, K: Ord, V> Iterator for Iter<'a, K, V> {
    type Item = (&'a K, &'a V);
    fn next(&mut self) -> Option<Self::Item> {
        if self.cur == NIL {
            return None;
        }
        let n = self.list.node(self.cur);
        if self.hi.is_some_and(|h| n.key >= *h) {
            return None;
        }
        self.cur = n.next[0];
        Some((&n.key, &n.val))
    }
}

struct Rng(u64);
impl Rng {
    fn next(&mut self) -> u64 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        self.0
    }
}

fn main() {
    // Differential test against BTreeMap: 20k seeded ops over a small key space (lots of collisions).
    let mut sl: SkipList<u32, u64> = SkipList::new(42);
    let mut oracle: BTreeMap<u32, u64> = BTreeMap::new();
    let mut rng = Rng(0xABCD_EF01_2345_6789);
    for step in 0..20_000u64 {
        let key = (rng.next() % 2_000) as u32;
        match rng.next() % 10 {
            0..=4 => assert_eq!(sl.insert(key, step), oracle.insert(key, step)),
            5..=6 => assert_eq!(sl.remove(&key), oracle.remove(&key)),
            7..=8 => assert_eq!(sl.get(&key), oracle.get(&key)),
            _ => {
                let hi = key + (rng.next() % 200) as u32;
                let got: Vec<_> = sl.range(&key, &hi).map(|(k, v)| (*k, *v)).collect();
                let want: Vec<_> = oracle.range(key..hi).map(|(k, v)| (*k, *v)).collect();
                assert_eq!(got, want, "range {key}..{hi} at step {step}");
            }
        }
        assert_eq!(sl.len, oracle.len());
    }
    let all: Vec<_> = sl.iter().map(|(k, v)| (*k, *v)).collect();
    assert!(all.iter().copied().eq(oracle.iter().map(|(k, v)| (*k, *v))));
    println!(
        "differential test passed: 20000 ops, {} live entries, ordered iteration identical",
        sl.len
    );

    // Height: expected ~ log_4(n); average tower ~ 1/(1-p) = 1.33 pointers per node.
    let mut big: SkipList<u32, ()> = SkipList::new(7);
    let n = 100_000u32;
    for i in 0..n {
        big.insert(i.wrapping_mul(2_654_435_761), ()); // scattered insertion order
    }
    let log4 = (n as f64).log(4.0);
    println!(
        "n={n}: height {} (log4 n = {log4:.1}), avg tower {:.2} (expect 1.33)",
        big.height(),
        big.avg_tower()
    );
    assert!(
        big.height() as f64 <= 2.0 * log4 + 3.0,
        "height should be O(log n)"
    );
    assert!(big.height() as f64 >= log4 - 4.0);
    assert!((1.2..1.5).contains(&big.avg_tower()));
    assert!(
        big.iter().zip(big.iter().skip(1)).all(|(a, b)| a.0 < b.0),
        "iteration must be strictly sorted"
    );

    // Slot recycling: removing everything empties the list and shrinks the lanes back.
    let keys: Vec<u32> = big.iter().map(|(k, _)| *k).collect();
    for k in &keys {
        big.remove(k).unwrap();
    }
    assert_eq!((big.len, big.height(), big.iter().count()), (0, 1, 0));
    println!(
        "emptied: arena kept {} slots on the free list for reuse",
        big.free.len()
    );
}
