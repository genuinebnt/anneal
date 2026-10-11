//! What the lock manager needs to know about a transaction: its id, its isolation level and its place in two-phase locking.

use std::sync::{Arc, Mutex};

use crate::concurrency::transaction::TxnId;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LockIsolation {
    ReadUncommitted,
    ReadCommitted,
    RepeatableRead,
}

/// Two-phase locking: a transaction first only acquires locks (growing), then only releases them (shrinking).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LockState {
    Growing,
    Shrinking,
    Committed,
    Aborted,
}

pub struct LockTxn {
    id: TxnId,
    isolation: LockIsolation,
    state: Mutex<LockState>,
}

impl LockTxn {
    pub fn new(id: TxnId, isolation: LockIsolation) -> Arc<LockTxn> {
        Arc::new(LockTxn { id, isolation, state: Mutex::new(LockState::Growing) })
    }

    pub fn id(&self) -> TxnId {
        self.id
    }

    pub fn isolation(&self) -> LockIsolation {
        self.isolation
    }

    pub fn state(&self) -> LockState {
        *self.state.lock().unwrap()
    }

    pub fn set_state(&self, state: LockState) {
        *self.state.lock().unwrap() = state;
    }
}
