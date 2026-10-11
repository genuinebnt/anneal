//! A positive-negative counter CRDT.

use std::collections::BTreeMap;

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct PnCounter {
    _pn: (),
}

impl PnCounter {
    /// A counter nobody has changed.
    pub fn new() -> PnCounter {
        todo!("0d-c4: a counter nobody has changed")
    }

    /// Records that `node` added `n`.
    pub fn inc(&mut self, node: u32, n: u64) {
        todo!("0d-c4: this node added n")
    }

    /// Records that `node` took away `n`.
    pub fn dec(&mut self, node: u32, n: u64) {
        todo!("0d-c4: this node took n away")
    }

    /// All additions minus all removals, over every node.
    pub fn value(&self) -> i64 {
        todo!("0d-c4: all added minus all removed")
    }

    /// Takes, for each node, the larger count, separately for additions and removals. Merging is commutative, associative and idempotent.
    pub fn merge(&mut self, other: &PnCounter) {
        todo!("0d-c4: for each node the larger count, separately for added and removed")
    }
}
