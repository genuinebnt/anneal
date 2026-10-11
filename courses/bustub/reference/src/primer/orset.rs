//! An **observed-remove set**, a CRDT (conflict-free replicated data type). Replicas of the set on different machines accept adds and removes
//! independently, and merging any two replicas in any order, any number of times, gives the same set: it converges without a coordinator.
//!
//! The trick is to name every *add*: `add(elem, uid)` creates the pair `(elem, uid)`. A `remove(elem)` kills the pairs for `elem` that this
//! replica **has seen**, and remembers them as removed. An element is in the set while it has a pair that is not removed. A concurrent add
//! elsewhere has a pair this replica never saw, so it survives the merge: **add wins**.

use std::collections::BTreeSet;
use std::fmt::Display;

pub type Uid = i64;

#[derive(Clone, Debug, Default)]
pub struct ORSet<T: Ord + Clone> {
    // @begin 0d-04
    /// Every `(element, uid)` pair ever added.
    adds: BTreeSet<(T, Uid)>,
    /// The pairs that have been removed.
    removed: BTreeSet<(T, Uid)>,
    //~ _set: std::marker::PhantomData<T>,
    //~ // TODO(0d-04): your fields: what has been added (each add has a unique id) and what has been removed
    // @end
}

impl<T: Ord + Clone + Display> ORSet<T> {
    /// An empty set.
    pub fn new() -> ORSet<T> {
        // @begin 0d-04
        ORSet { adds: BTreeSet::new(), removed: BTreeSet::new() }
        //~ todo!("0d-04: an empty set")
        // @end
    }

    /// Whether `elem` has an add pair that is not removed.
    pub fn contains(&self, elem: &T) -> bool {
        // @begin 0d-04
        self.adds.iter().any(|pair| pair.0 == *elem && !self.removed.contains(pair))
        //~ todo!("0d-04: is there an add pair for elem that is not in the removed set?")
        // @end
    }

    /// Adds the pair `(elem, uid)`. A later `remove` only kills pairs it has seen, so an add with a new `uid` brings the element back.
    pub fn add(&mut self, elem: &T, uid: Uid) {
        // @begin 0d-04
        self.adds.insert((elem.clone(), uid));
        //~ todo!("0d-04: remember the pair (elem, uid)")
        // @end
    }

    /// Removes `elem` as far as this replica has seen it: every add pair for it that exists now is marked removed.
    pub fn remove(&mut self, elem: &T) {
        // @begin 0d-04
        let seen: Vec<(T, Uid)> = self.adds.iter().filter(|pair| pair.0 == *elem).cloned().collect();
        self.removed.extend(seen);
        //~ todo!("0d-04: mark every add pair of elem that is here as removed")
        // @end
    }

    /// Takes in what `other` knows: all its adds and all its removals.
    pub fn merge(&mut self, other: &ORSet<T>) {
        // @begin 0d-04
        self.adds.extend(other.adds.iter().cloned());
        self.removed.extend(other.removed.iter().cloned());
        //~ todo!("0d-04: the union of the add pairs and the union of the removed pairs")
        // @end
    }

    /// The elements in the set, each once.
    pub fn elements(&self) -> Vec<T> {
        // @begin 0d-04
        let mut out: Vec<T> = self.adds.iter().filter(|pair| !self.removed.contains(pair)).map(|pair| pair.0.clone()).collect();
        out.dedup(); // the pairs are ordered by element, so equal elements are next to each other
        out
        //~ todo!("0d-04: the elements of the pairs that are not removed, without repeats")
        // @end
    }

    /// `{a, b, c}` with the elements sorted.
    pub fn to_string(&self) -> String {
        let mut elements = self.elements();
        elements.sort();
        format!("{{{}}}", elements.iter().map(|e| e.to_string()).collect::<Vec<_>>().join(", "))
    }
}

/// A network of replicas in one process: nodes `save` their set to a shared place and `load` the copies the others saved; `sync` does both for
/// every node. Nodes can also be driven one at a time to simulate a lost network.
pub struct ORSetDriver<T: Ord + Clone> {
    nodes: Vec<ORSet<T>>,
    saved: Vec<ORSet<T>>,
    /// How many times each node saved.
    version: Vec<u32>,
    /// `last_read[node][peer]`: the version of the peer's copy that `node` merged last.
    last_read: Vec<Vec<u32>>,
    next_uid: Uid,
}

impl<T: Ord + Clone + Display> ORSetDriver<T> {
    pub fn new(n: usize) -> ORSetDriver<T> {
        ORSetDriver { nodes: vec![ORSet::new(); n], saved: vec![ORSet::new(); n], version: vec![0; n], last_read: vec![vec![0; n]; n], next_uid: 0 }
    }

    pub fn add(&mut self, node: usize, elem: &T) {
        let uid = self.next_uid;
        self.next_uid += 1;
        self.nodes[node].add(elem, uid);
    }

    pub fn remove(&mut self, node: usize, elem: &T) {
        self.nodes[node].remove(elem);
    }

    pub fn contains(&self, node: usize, elem: &T) -> bool {
        self.nodes[node].contains(elem)
    }

    pub fn set(&self, node: usize) -> &ORSet<T> {
        &self.nodes[node]
    }

    /// Publishes this node's set.
    pub fn save(&mut self, node: usize) {
        self.saved[node] = self.nodes[node].clone();
        self.version[node] += 1;
    }

    /// Merges the published copy of every other node that has changed since this node last read it.
    pub fn load(&mut self, node: usize) {
        for peer in 0..self.nodes.len() {
            if peer == node || self.last_read[node][peer] >= self.version[peer] {
                continue;
            }
            let copy = self.saved[peer].clone();
            self.nodes[node].merge(&copy);
            self.last_read[node][peer] = self.version[peer];
        }
    }

    /// Every node saves, then every node loads.
    pub fn sync(&mut self) {
        for node in 0..self.nodes.len() {
            self.save(node);
        }
        for node in 0..self.nodes.len() {
            self.load(node);
        }
    }
}
