//! A snapshot defined by transaction ids.

use std::collections::BTreeSet;

pub struct ReadView {
    _view: (),
}

impl ReadView {
    /// A snapshot for transaction `own_id`: `active_ids` are the transactions that were running, `next_id` the next id to be handed out.
    pub fn new(own_id: u64, active_ids: &[u64], next_id: u64) -> ReadView {
        todo!("4a-c1: remember who was active and where ids stood")
    }

    /// Is a version written by transaction `writer` visible to this snapshot?
    pub fn visible(&self, writer: u64) -> bool {
        todo!("4a-c1: your own writes; older and not active")
    }
}
