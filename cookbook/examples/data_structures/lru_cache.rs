//! RECIPE 95 (advanced): an LRU cache: HashMap + a doubly linked list stored in a Vec with INDICES (no unsafe, no Rc<RefCell>).
//! LEVEL: advanced
//! Use in: caches (lookups, compiled plans, parsed files), and the "index graph" pattern for linked structures.   Run: cargo run --release --example lru_cache
//! COMMON USES: caches of every kind (LRU in browsers, DBs, CDNs); the index-linked-list pattern for any ordered structure.
#![allow(dead_code)]
//!
//! Operations must be O(1): the HashMap finds a node by key; the linked list orders nodes by recency; touching a node unlinks it and
//! relinks it at the head; evicting removes the tail. A linked list with pointers fights Rust ownership, so the nodes live in a
//! `Vec` and "pointers" are `usize` indices (index_graph, arena_ids). Generic over K, V with trait bounds.
//! Verified by a MODEL TEST: run random operations against both this cache and a trivially-correct slow model; they must agree.
//! Real code: https://github.com/jeromefroe/lru-rs (uses unsafe pointers) and https://github.com/moka-rs/moka (concurrent).
use std::collections::HashMap;
use std::hash::Hash;

const NIL: usize = usize::MAX;

struct Node<K, V> {
    key: K,
    val: V,
    prev: usize,
    next: usize,
}

struct Lru<K, V> {
    map: HashMap<K, usize>,
    nodes: Vec<Node<K, V>>,
    head: usize, // most recently used
    tail: usize, // least recently used
    cap: usize,
}

impl<K: Hash + Eq + Clone, V> Lru<K, V> {
    fn new(cap: usize) -> Self {
        assert!(cap > 0);
        Lru {
            map: HashMap::with_capacity(cap),
            nodes: Vec::with_capacity(cap),
            head: NIL,
            tail: NIL,
            cap,
        }
    }

    fn detach(&mut self, i: usize) {
        let (p, n) = (self.nodes[i].prev, self.nodes[i].next);
        if p != NIL {
            self.nodes[p].next = n
        } else {
            self.head = n
        }
        if n != NIL {
            self.nodes[n].prev = p
        } else {
            self.tail = p
        }
    }
    fn push_front(&mut self, i: usize) {
        self.nodes[i].prev = NIL;
        self.nodes[i].next = self.head;
        if self.head != NIL {
            let h = self.head;
            self.nodes[h].prev = i;
        }
        self.head = i;
        if self.tail == NIL {
            self.tail = i;
        }
    }

    fn get(&mut self, k: &K) -> Option<&V> {
        let &i = self.map.get(k)?;
        self.detach(i);
        self.push_front(i);
        Some(&self.nodes[i].val)
    }

    /// inserts or updates; returns the evicted (key, value) if the cache was full
    fn put(&mut self, k: K, v: V) -> Option<(K, V)> {
        if let Some(&i) = self.map.get(&k) {
            self.nodes[i].val = v;
            self.detach(i);
            self.push_front(i);
            return None;
        }
        if self.map.len() < self.cap {
            let i = self.nodes.len();
            self.nodes.push(Node {
                key: k.clone(),
                val: v,
                prev: NIL,
                next: NIL,
            });
            self.map.insert(k, i);
            self.push_front(i);
            return None;
        }
        // full: REUSE the tail node's slot for the new entry
        let t = self.tail;
        self.detach(t);
        let old_key = std::mem::replace(&mut self.nodes[t].key, k.clone());
        let old_val = std::mem::replace(&mut self.nodes[t].val, v);
        self.map.remove(&old_key);
        self.map.insert(k, t);
        self.push_front(t);
        Some((old_key, old_val))
    }

    /// keys from most to least recently used (for tests)
    fn keys_mru(&self) -> Vec<K> {
        let mut out = vec![];
        let mut i = self.head;
        while i != NIL {
            out.push(self.nodes[i].key.clone());
            i = self.nodes[i].next;
        }
        out
    }
    fn len(&self) -> usize {
        self.map.len()
    }
}

/// the obviously-correct O(n) reference: a Vec ordered most-recent-first
struct Model {
    items: Vec<(u32, u32)>,
    cap: usize,
}
impl Model {
    fn get(&mut self, k: u32) -> Option<u32> {
        let pos = self.items.iter().position(|(key, _)| *key == k)?;
        let e = self.items.remove(pos);
        self.items.insert(0, e);
        Some(e.1)
    }
    fn put(&mut self, k: u32, v: u32) -> Option<(u32, u32)> {
        if let Some(pos) = self.items.iter().position(|(key, _)| *key == k) {
            self.items.remove(pos);
            self.items.insert(0, (k, v));
            return None;
        }
        let evicted = if self.items.len() == self.cap {
            self.items.pop()
        } else {
            None
        };
        self.items.insert(0, (k, v));
        evicted
    }
}

fn main() {
    let mut c = Lru::new(2);
    assert_eq!(c.put("a", 1), None);
    assert_eq!(c.put("b", 2), None);
    assert_eq!(c.get(&"a"), Some(&1)); // a becomes most recent
    assert_eq!(c.put("c", 3), Some(("b", 2))); // b was least recent: evicted
    println!("{:?} len={}", c.keys_mru(), c.len());

    // model-based test: random operations, compare every result and the full recency order
    let mut x = 0x9E37_79B9_7F4A_7C15u64;
    let mut rnd = |n: u64| {
        x ^= x << 13;
        x ^= x >> 7;
        x ^= x << 17;
        (x % n) as u32
    };
    for cap in [1usize, 2, 4, 7] {
        let (mut fast, mut model) = (Lru::new(cap), Model { items: vec![], cap });
        for step in 0..5000 {
            let k = rnd(12);
            if rnd(3) == 0 {
                assert_eq!(
                    fast.get(&k).copied(),
                    model.get(k),
                    "get({k}) at step {step}, cap {cap}"
                );
            } else {
                let v = rnd(1000);
                assert_eq!(
                    fast.put(k, v),
                    model.put(k, v),
                    "put({k}) at step {step}, cap {cap}"
                );
            }
            assert_eq!(
                fast.keys_mru(),
                model.items.iter().map(|e| e.0).collect::<Vec<_>>(),
                "order at step {step}, cap {cap}"
            );
            assert!(fast.len() <= cap);
        }
    }
    println!(
        "model test passed: 4 capacities x 5000 random operations agree with the O(n) reference"
    );
}
