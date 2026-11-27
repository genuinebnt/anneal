use solution::*;
use std::cell::Cell;
use std::collections::{BTreeMap, BTreeSet};

/// A pool that checks how it is used and counts reads.
struct Checked {
    fan: usize,
    limit: usize,
    nodes: BTreeMap<NodeId, Vec<u32>>,
    next: NodeId,
    reads: Cell<u64>,
    /// Reads still allowed before the test stops: a guard against walking every index.
    allowed: Cell<u64>,
}

impl Checked {
    fn new(fan: usize, limit: usize) -> Checked {
        Checked { fan, limit, nodes: BTreeMap::new(), next: 1, reads: Cell::new(0), allowed: Cell::new(u64::MAX) }
    }
    /// Reads made by `call`, which may make at most `budget`.
    fn reads_of<T>(&mut self, budget: u64, call: impl FnOnce(&mut Checked) -> T) -> (T, u64) {
        let before = self.reads.get();
        self.allowed.set(budget);
        let out = call(self);
        self.allowed.set(u64::MAX);
        (out, self.reads.get() - before)
    }
}

impl Pool for Checked {
    fn fan(&self) -> usize {
        self.fan
    }
    fn alloc(&mut self) -> Option<NodeId> {
        if self.nodes.len() == self.limit {
            return None;
        }
        let id = self.next;
        self.next += 1;
        self.nodes.insert(id, vec![0; self.fan]);
        Some(id)
    }
    fn free(&mut self, node: NodeId) {
        assert!(self.nodes.remove(&node).is_some(), "node {node} is not in use: freed twice?");
    }
    fn get(&self, node: NodeId, slot: usize) -> u32 {
        self.reads.set(self.reads.get() + 1);
        assert!(self.allowed.get() > 0, "too many reads: see the limits in the statement");
        self.allowed.set(self.allowed.get() - 1);
        self.nodes.get(&node).unwrap_or_else(|| panic!("node {node} is not in use"))[slot]
    }
    fn set(&mut self, node: NodeId, slot: usize, value: u32) {
        self.nodes.get_mut(&node).unwrap_or_else(|| panic!("node {node} is not in use"))[slot] = value;
    }
}

/// How many nodes a tree needs to hold values at exactly these indices.
fn needed(fan: u64, levels: u32, indices: impl Iterator<Item = u64> + Clone) -> usize {
    (1..=levels)
        .map(|height| indices.clone().map(|i| i / fan.pow(height)).collect::<BTreeSet<u64>>().len())
        .sum()
}

#[test]
fn an_empty_list_is_all_zeros_and_uses_nothing() {
    let pool = Checked::new(4, 100);
    let list = List::new(3);
    check!("values", [list.get(&pool, 0), list.get(&pool, 63), list.get(&pool, 64), list.get(&pool, u64::MAX)], [0; 4]);
    check!("root and nodes", (list.root(), pool.nodes.len()), (0, 0));
}

#[test]
fn one_value_needs_one_node_per_level() {
    let mut pool = Checked::new(4, 100);
    let mut list = List::new(3);
    check!("set 27", list.set(&mut pool, 27, 9), Ok(()));
    check!("get", (list.get(&pool, 27), list.get(&pool, 26)), (9, 0));
    check!("nodes", pool.nodes.len(), 3);
    // 27 is 1, 2, 3 in base 4
    let middle = pool.nodes[&list.root()][1];
    let leaf = pool.nodes[&middle][2];
    check!("the leaf's slot 3", pool.nodes[&leaf][3], 9);
}

#[test]
fn neighbours_share_nodes() {
    let mut pool = Checked::new(4, 100);
    let mut list = List::new(3);
    for (i, v) in [(0, 1), (1, 2), (4, 3), (16, 4)] {
        list.set(&mut pool, i, v).unwrap();
    }
    // the root, two middle nodes and three leaves
    check!("nodes", pool.nodes.len(), 6);
}

#[test]
fn erasing_gives_nodes_back() {
    let mut pool = Checked::new(4, 100);
    let mut list = List::new(3);
    list.set(&mut pool, 5, 1).unwrap();
    list.set(&mut pool, 60, 2).unwrap();
    list.set(&mut pool, 5, 0).unwrap();
    check!("one path is left", pool.nodes.len(), 3);
    list.set(&mut pool, 60, 0).unwrap();
    check!("nothing is left", (pool.nodes.len(), list.root()), (0, 0));
}

#[test]
fn cutting_drops_the_end() {
    let mut pool = Checked::new(4, 100);
    let mut list = List::new(3);
    for i in 0..40 {
        list.set(&mut pool, i, i as u32 + 1).unwrap();
    }
    list.cut(&mut pool, 6);
    check!("kept and dropped", (list.get(&pool, 5), list.get(&pool, 6), list.get(&pool, 39)), (6, 0, 0));
    check!("the root, one middle node, two leaves", pool.nodes.len(), 4);
}
