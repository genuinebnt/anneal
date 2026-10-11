//! A snapshot defined by transaction ids.

use std::collections::BTreeSet;

pub struct ReadView {
    // @begin 4a-c1
    own: u64,
    active: BTreeSet<u64>,
    next_id: u64,
    //~ _view: (),
    // @end
}

impl ReadView {
    /// A snapshot for transaction `own_id`: `active_ids` are the transactions that were running, `next_id` the next id to be handed out.
    pub fn new(own_id: u64, active_ids: &[u64], next_id: u64) -> ReadView {
        // @begin 4a-c1
        ReadView { own: own_id, active: active_ids.iter().copied().collect(), next_id }
        //~ todo!("4a-c1: remember who was active and where ids stood")
        // @end
    }

    /// Is a version written by transaction `writer` visible to this snapshot?
    pub fn visible(&self, writer: u64) -> bool {
        // @begin 4a-c1
        if writer == self.own {
            return true;
        }
        writer < self.next_id && !self.active.contains(&writer)
        //~ todo!("4a-c1: your own writes; older and not active")
        // @end
    }
}
