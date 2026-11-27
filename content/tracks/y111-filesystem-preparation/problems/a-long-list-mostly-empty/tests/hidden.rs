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
fn a_full_pool_changes_nothing() {
    let mut pool = Checked::new(4, 4);
    let mut list = List::new(3);
    list.set(&mut pool, 0, 7).unwrap();
    // index 20 needs two new nodes and only one is left
    check!("set", list.set(&mut pool, 20, 1), Err(SetError::Full));
    check!("no node was kept", pool.nodes.len(), 3);
    check!("values", (list.get(&pool, 0), list.get(&pool, 20)), (7, 0));
    check!("a neighbour needs only a leaf", list.set(&mut pool, 4, 2), Ok(()));
    check!("overwrite when full", list.set(&mut pool, 4, 3), Ok(()));
    check!("erase when full", list.set(&mut pool, 4, 0), Ok(()));
    check!("nodes", pool.nodes.len(), 3);
}

#[test]
fn a_pool_too_small_for_the_first_value() {
    for limit in 0..4 {
        let mut pool = Checked::new(2, limit);
        let mut list = List::new(4);
        check!(format!("limit {limit}"), (list.set(&mut pool, 9, 1), pool.nodes.len(), list.root()), (Err(SetError::Full), 0, 0));
    }
}

#[test]
fn erasing_what_is_not_there_allocates_nothing() {
    let mut pool = Checked::new(4, 0);
    let mut list = List::new(3);
    check!("erase in an empty list", list.set(&mut pool, 10, 0), Ok(()));
    check!("out of range", (list.set(&mut pool, 64, 0), list.set(&mut pool, u64::MAX, 1)), (Err(SetError::OutOfRange), Err(SetError::OutOfRange)));
    pool.limit = 3;
    list.set(&mut pool, 10, 5).unwrap();
    check!("erase a missing neighbour", list.set(&mut pool, 40, 0), Ok(()));
    check!("nodes", pool.nodes.len(), 3);
}

#[test]
fn one_level_is_one_node() {
    let mut pool = Checked::new(5, 1);
    let mut list = List::new(1);
    for i in 0..5 {
        check!(format!("set {i}"), list.set(&mut pool, i, 100 - i as u32), Ok(()));
    }
    check!("past the end", list.set(&mut pool, 5, 1), Err(SetError::OutOfRange));
    list.cut(&mut pool, 2);
    check!("values", (0..5).map(|i| list.get(&pool, i)).collect::<Vec<_>>(), vec![100, 99, 0, 0, 0]);
    list.cut(&mut pool, 0);
    check!("empty", pool.nodes.len(), 0);
}

#[test]
fn a_value_may_be_any_non_zero_number() {
    let mut pool = Checked::new(2, 100);
    let mut list = List::new(5);
    list.set(&mut pool, 31, u32::MAX).unwrap();
    list.set(&mut pool, 0, 1).unwrap();
    check!("values", (list.get(&pool, 31), list.get(&pool, 0)), (u32::MAX, 1));
}

#[test]
fn a_tree_that_covers_more_than_64_bits() {
    // 256 to the power 9 does not fit in a u64: every index is in range
    let mut pool = Checked::new(256, 100);
    let mut list = List::new(9);
    check!("set the last index", list.set(&mut pool, u64::MAX, 4), Ok(()));
    check!("set the first", list.set(&mut pool, 0, 3), Ok(()));
    check!("get", (list.get(&pool, u64::MAX), list.get(&pool, 0), list.get(&pool, 1 << 40)), (4, 3, 0));
    let (_, _) = pool.reads_of(40_000, |pool| list.cut(pool, u64::MAX));
    check!("only the first is left", (list.get(&pool, u64::MAX), list.get(&pool, 0), pool.nodes.len()), (0, 3, 9));
}

#[test]
fn work_does_not_grow_with_the_size_of_the_list() {
    // four thousand million indices
    let mut pool = Checked::new(256, 1000);
    let mut list = List::new(4);
    for i in [0, 255, 1 << 20, (1 << 32) - 1] {
        let (result, reads) = pool.reads_of(4 * 257, |pool| list.set(pool, i, 1));
        check!(format!("set {i}: {reads} reads"), result, Ok(()));
        let (value, _) = pool.reads_of(4, |pool| list.get(pool, i));
        check!(format!("get {i}"), value, 1);
    }
    for len in [1 << 31, 1 << 20, 100, 1, 0] {
        let budget = 2 * 256 * pool.nodes.len() as u64 + 1;
        let (_, reads) = pool.reads_of(budget, |pool| list.cut(pool, len));
        check!(format!("cut at {len}: {reads} reads"), list.get(&pool, 1 << 31), 0);
    }
    check!("empty", pool.nodes.len(), 0);
}

#[test]
fn random_use_matches_a_map() {
    let mut rng = anneal_prelude::Rng::new(111_301);
    for round in 0..150 {
        let fan = 2 + rng.below(4);
        let levels = 1 + rng.below(4) as u32;
        let capacity = (fan as u64).pow(levels);
        let limit = 1 + rng.below(30);
        let mut pool = Checked::new(fan, limit);
        let mut list = List::new(levels);
        let mut model: BTreeMap<u64, u32> = BTreeMap::new();
        for step in 0..80 {
            let index = rng.below(capacity as usize + 2) as u64;
            let what = format!("round {round} step {step} (fan {fan}, levels {levels}, {limit} nodes, index {index})");
            if rng.below(8) == 0 {
                let budget = 2 * fan as u64 * pool.nodes.len() as u64 + 1;
                pool.reads_of(budget, |pool| list.cut(pool, index));
                model.retain(|i, _| *i < index);
            } else {
                let value = if rng.below(3) == 0 { 0 } else { 1 + rng.below(100) as u32 };
                let with = model.keys().copied().chain((value != 0).then_some(index));
                let want = if index >= capacity {
                    Err(SetError::OutOfRange)
                } else if value != 0 && needed(fan as u64, levels, with.collect::<Vec<_>>().into_iter()) > limit {
                    Err(SetError::Full)
                } else {
                    Ok(())
                };
                let budget = u64::from(levels) * (fan as u64 + 1);
                let (got, _) = pool.reads_of(budget, |pool| list.set(pool, index, value));
                check!(format!("{what}: set {value}"), got, want);
                if want.is_ok() {
                    if value == 0 {
                        model.remove(&index);
                    } else {
                        model.insert(index, value);
                    }
                }
            }
            check!(format!("{what}: nodes in use"), pool.nodes.len(), needed(fan as u64, levels, model.keys().copied()));
            let probe = rng.below(capacity as usize) as u64;
            check!(format!("{what}: get {probe}"), list.get(&pool, probe), model.get(&probe).copied().unwrap_or(0));
        }
        for (i, v) in &model {
            check!(format!("round {round}: value {i}"), list.get(&pool, *i), *v);
        }
    }
}
