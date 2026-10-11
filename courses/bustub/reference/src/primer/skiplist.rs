//! An ordered set as a **skip list**, a linked list with express lanes. Every node has a random height; a node of height `h` is linked into the
//! lists of levels `0..h`; searching starts on the top level and drops down a level whenever the next node is too far. Expected `O(log n)` search,
//! insert and erase, without rebalancing.
//!
//! The nodes live in a `Vec` (an arena) and a link is the **index** of the next node, which is how Rust usually builds linked structures without
//! `Rc<RefCell<..>>`. The whole list is behind one reader-writer lock.

use std::sync::RwLock;

use super::mt19937::Mt19937;

pub struct SkipList<K, const MAX_HEIGHT: usize = 14, const SEED: u32 = 15445> {
    // @begin 0b-01
    compare: Box<dyn Fn(&K, &K) -> bool + Send + Sync>,
    inner: RwLock<Inner<K>>,
    //~ _list: std::marker::PhantomData<K>,
    //~ // TODO(0b-01): your fields: the order, the nodes (an arena of nodes linked by index is the intended design), the height, the size, the random generator
    // @end
}

// @begin 0b-01
/// The index of a node in the arena; the header is always node 0.
type NodeId = usize;
const HEADER: NodeId = 0;
const LOWEST_LEVEL: usize = 0;

/// A node of the skip list: its key (the header has none) and one forward link per level it takes part in.
struct SkipNode<K> {
    key: Option<K>,
    /// `links[0]` is the lowest level; `links.len()` is the node's height.
    links: Vec<Option<NodeId>>,
}

impl<K> SkipNode<K> {
    fn new(height: usize, key: Option<K>) -> SkipNode<K> {
        SkipNode { key, links: vec![None; height] }
    }

    fn height(&self) -> usize {
        self.links.len()
    }

    fn next(&self, level: usize) -> Option<NodeId> {
        self.links.get(level).copied().flatten()
    }

    fn set_next(&mut self, level: usize, next: Option<NodeId>) {
        self.links[level] = next;
    }
}

struct Inner<K> {
    nodes: Vec<SkipNode<K>>,
    /// Arena slots of erased nodes, to be reused.
    free: Vec<NodeId>,
    /// The highest level in use: at least 1, at most `MAX_HEIGHT`.
    height: usize,
    size: usize,
    rng: Mt19937,
}

impl<K> Inner<K> {
    /// A new node's slot: a freed one if there is one.
    fn alloc(&mut self, node: SkipNode<K>) -> NodeId {
        match self.free.pop() {
            Some(id) => {
                self.nodes[id] = node;
                id
            }
            None => {
                self.nodes.push(node);
                self.nodes.len() - 1
            }
        }
    }

    /// Marks the slot of an erased node as free (its key is dropped).
    fn release(&mut self, id: NodeId) {
        self.nodes[id] = SkipNode { key: None, links: vec![] };
        self.free.push(id);
    }

    /// Draws a height like BusTub: 1, plus one for every draw divisible by 4 (a 1-in-4 chance, Pugh's branching factor), capped.
    fn random_height(&mut self, max_height: usize) -> usize {
        let mut height = 1;
        while height < max_height && self.rng.next_u32() % 4 == 0 {
            height += 1;
        }
        height
    }

    /// Walks from the top level: on each level go right while the next key is before `key`, then drop down. Returns the **update vector**
    /// (the last node before `key` on every level, index = level) and the node holding an equivalent key, if any.
    fn search(&self, compare: &dyn Fn(&K, &K) -> bool, key: &K) -> (Vec<NodeId>, Option<NodeId>) {
        let mut update = vec![HEADER; self.nodes[HEADER].links.len()];
        let mut cur = HEADER;
        for level in (0..self.height).rev() {
            while let Some(next) = self.nodes[cur].next(level) {
                if compare(self.nodes[next].key.as_ref().unwrap(), key) {
                    cur = next;
                } else {
                    break;
                }
            }
            update[level] = cur;
        }
        let candidate = self.nodes[cur].next(LOWEST_LEVEL);
        let found = candidate.filter(|&id| !compare(key, self.nodes[id].key.as_ref().unwrap()));
        (update, found)
    }
}
//~ // TODO(0b-01): the node type and the helpers of your own: a search that remembers the last node before the key on every level (the update vector), a random height (BusTub's rule: 1, plus one for every draw divisible by 4, up to MAX_HEIGHT), a way to reuse the slots of erased nodes
// @end

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
        // @begin 0b-01
        let header = SkipNode { key: None, links: vec![None; MAX_HEIGHT] };
        SkipList { compare: Box::new(compare), inner: RwLock::new(Inner { nodes: vec![header], free: vec![], height: 1, size: 0, rng: Mt19937::new(SEED) }) }
        //~ todo!("0b-01: an empty list: the header node with MAX_HEIGHT empty links, height 1, size 0, a generator seeded with SEED")
        // @end
    }

    pub fn is_empty(&self) -> bool {
        self.size() == 0
    }

    /// The number of elements.
    pub fn size(&self) -> usize {
        // @begin 0b-01
        self.inner.read().unwrap().size
        //~ todo!("0b-01: the element count, under the read lock")
        // @end
    }

    /// Forgets every element (the arena restarts with just the header).
    pub fn clear(&self) {
        // @begin 0b-02
        let mut inner = self.inner.write().unwrap();
        inner.nodes.truncate(1);
        inner.nodes[HEADER].links.iter_mut().for_each(|l| *l = None);
        inner.free.clear();
        inner.height = 1;
        inner.size = 0;
        //~ todo!("0b-02: under the write lock: forget every element; the list is as new (height 1, size 0)")
        // @end
    }

    /// Adds `key`. Returns `false`, changing nothing, if an equivalent key is already there (neither is before the other).
    pub fn insert(&self, key: &K) -> bool {
        // @begin 0b-01
        let mut inner = self.inner.write().unwrap();
        let (update, found) = inner.search(&self.compare, key);
        if found.is_some() {
            return false;
        }
        let new_height = inner.random_height(MAX_HEIGHT);
        // levels above the current height start at the header
        let old_height = inner.height;
        if new_height > old_height {
            inner.height = new_height;
        }
        let id = inner.alloc(SkipNode::new(new_height, Some(key.clone())));
        for level in 0..new_height {
            let prev = if level < old_height { update[level] } else { HEADER };
            let next = inner.nodes[prev].next(level);
            inner.nodes[id].set_next(level, next);
            inner.nodes[prev].set_next(level, Some(id));
        }
        inner.size += 1;
        true
        //~ todo!("0b-01: write lock; search with the update vector (the last node before the key on every level); an equal key: false; draw a height; if taller than the list raise it; allocate the node; on each level 0..height link it between update[level] and its next; size + 1")
        // @end
    }

    /// Removes `key`. Returns `false` if it is not there.
    pub fn erase(&self, key: &K) -> bool {
        // @begin 0b-02
        let mut inner = self.inner.write().unwrap();
        let (update, found) = inner.search(&self.compare, key);
        let Some(id) = found else { return false };
        for level in 0..inner.nodes[id].height() {
            let next = inner.nodes[id].next(level);
            inner.nodes[update[level]].set_next(level, next);
        }
        inner.release(id);
        while inner.height > 1 && inner.nodes[HEADER].next(inner.height - 1).is_none() {
            inner.height -= 1;
        }
        inner.size -= 1;
        true
        //~ todo!("0b-02: write lock; search; not found: false; on each level of the node make update[level] skip it; free the slot; lower the list's height while the top level is empty; size - 1")
        // @end
    }

    /// Is an equivalent key in the list? Neither `compare(a, b)` nor `compare(b, a)` for an equivalent key.
    pub fn contains(&self, key: &K) -> bool {
        // @begin 0b-01
        let inner = self.inner.read().unwrap();
        inner.search(&self.compare, key).1.is_some()
        //~ todo!("0b-01: read lock; search; is there a node?")
        // @end
    }

    /// The keys with their heights, in order.
    pub fn nodes(&self) -> Vec<(K, usize)> {
        // @begin 0b-01
        let inner = self.inner.read().unwrap();
        let mut out = vec![];
        let mut cur = inner.nodes[HEADER].next(LOWEST_LEVEL);
        while let Some(id) = cur {
            out.push((inner.nodes[id].key.clone().expect("only the header has no key"), inner.nodes[id].height()));
            cur = inner.nodes[id].next(LOWEST_LEVEL);
        }
        out
        //~ todo!("0b-01: walk level 0 from the start: every key with the height of its node")
        // @end
    }

    /// The keys linked on `level`, in order.
    pub fn level(&self, level: usize) -> Vec<K> {
        // @begin 0b-01
        let inner = self.inner.read().unwrap();
        let mut out = vec![];
        let mut cur = inner.nodes[HEADER].next(level);
        while let Some(id) = cur {
            out.push(inner.nodes[id].key.clone().unwrap());
            cur = inner.nodes[id].next(level);
        }
        out
        //~ todo!("0b-01: walk the given level from the start: the keys of the nodes linked on it")
        // @end
    }
}
