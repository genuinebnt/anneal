//! An ordered set as a **skip list**, a linked list with express lanes. Every node has a random height; a node of height `h` is linked into the
//! lists of levels `0..h`; searching starts on the top level and drops down a level whenever the next node is too far. Expected `O(log n)` search,
//! insert and erase, without rebalancing.
//!
//! The nodes live in a `Vec` (an arena) and a link is the **index** of the next node, which is how Rust usually builds linked structures without
//! `Rc<RefCell<..>>`. The whole list is behind one reader-writer lock.

use std::sync::RwLock;

use super::mt19937::Mt19937;

pub struct SkipList<K, const MAX_HEIGHT: usize = 14, const SEED: u32 = 15445> {
    _list: std::marker::PhantomData<K>,
    // TODO(0b-01): your fields: the order, the nodes (an arena of nodes linked by index is the intended design), the height, the size, the random generator
}

// TODO(0b-01): the node type and the helpers of your own: a search that remembers the last node before the key on every level (the update vector), a random height (BusTub's rule: 1, plus one for every draw divisible by 4, up to MAX_HEIGHT), a way to reuse the slots of erased nodes

impl<K: Clone + Ord + Send + Sync + 'static, const MAX_HEIGHT: usize, const SEED: u32> SkipList<K, MAX_HEIGHT, SEED> {
    /// An empty list ordered by `<`.
    pub fn new() -> Self {
        Self::with_compare(|a: &K, b: &K| a < b)
    }
}

impl<K: Clone + Send + Sync + 'static, const MAX_HEIGHT: usize, const SEED: u32> SkipList<K, MAX_HEIGHT, SEED> {
    /// An empty list ordered by `compare` ("is `a` before `b`?"). The random generator is a fixed-seed `Mt19937`, so the shape of a list depends only
    /// on the operations applied to it.
    pub fn with_compare(compare: impl Fn(&K, &K) -> bool + Send + Sync + 'static) -> Self {
        todo!("0b-01: an empty list: the header node with MAX_HEIGHT empty links, height 1, size 0, a generator seeded with SEED")
    }

    pub fn is_empty(&self) -> bool {
        self.size() == 0
    }

    /// The number of elements.
    pub fn size(&self) -> usize {
        todo!("0b-01: the element count, under the read lock")
    }

    /// Forgets every element (the arena restarts with just the header).
    pub fn clear(&self) {
        todo!("0b-02: under the write lock: forget every element; the list is as new (height 1, size 0)")
    }

    /// Adds `key`. Returns `false`, changing nothing, if an equivalent key is already there (neither is before the other).
    pub fn insert(&self, key: &K) -> bool {
        todo!("0b-01: false if an equivalent key is there; otherwise the node gets a random height and is linked in order on every level it reaches")
    }

    /// Removes `key`. Returns `false` if it is not there.
    pub fn erase(&self, key: &K) -> bool {
        todo!("0b-02: false if not found; the node is unlinked on every level and empty top levels are dropped")
    }

    /// Is an equivalent key in the list? Neither `compare(a, b)` nor `compare(b, a)` for an equivalent key.
    pub fn contains(&self, key: &K) -> bool {
        todo!("0b-01: read lock; search; is there a node?")
    }

    /// The keys with their heights, in order.
    pub fn nodes(&self) -> Vec<(K, usize)> {
        todo!("0b-01: walk level 0 from the start: every key with the height of its node")
    }

    /// The keys linked on `level`, in order.
    pub fn level(&self, level: usize) -> Vec<K> {
        todo!("0b-01: walk the given level from the start: the keys of the nodes linked on it")
    }
}
