//! A positive-negative counter CRDT.

use std::collections::BTreeMap;

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct PnCounter {
    // @begin 0d-c4
    added: BTreeMap<u32, u64>,
    removed: BTreeMap<u32, u64>,
    //~ _pn: (),
    // @end
}

impl PnCounter {
    /// A counter nobody has changed.
    pub fn new() -> PnCounter {
        // @begin 0d-c4
        PnCounter::default()
        //~ todo!("0d-c4: a counter nobody has changed")
        // @end
    }

    /// Records that `node` added `n`.
    pub fn inc(&mut self, node: u32, n: u64) {
        // @begin 0d-c4
        *self.added.entry(node).or_insert(0) += n;
        //~ todo!("0d-c4: this node added n")
        // @end
    }

    /// Records that `node` took away `n`.
    pub fn dec(&mut self, node: u32, n: u64) {
        // @begin 0d-c4
        *self.removed.entry(node).or_insert(0) += n;
        //~ todo!("0d-c4: this node took n away")
        // @end
    }

    /// All additions minus all removals, over every node.
    pub fn value(&self) -> i64 {
        // @begin 0d-c4
        self.added.values().sum::<u64>() as i64 - self.removed.values().sum::<u64>() as i64
        //~ todo!("0d-c4: all added minus all removed")
        // @end
    }

    /// Takes, for each node, the larger count, separately for additions and removals. Merging is commutative, associative and idempotent.
    pub fn merge(&mut self, other: &PnCounter) {
        // @begin 0d-c4
        for (&n, &v) in &other.added {
            let e = self.added.entry(n).or_insert(0);
            *e = (*e).max(v);
        }
        for (&n, &v) in &other.removed {
            let e = self.removed.entry(n).or_insert(0);
            *e = (*e).max(v);
        }
        //~ todo!("0d-c4: for each node the larger count, separately for added and removed")
        // @end
    }
}
