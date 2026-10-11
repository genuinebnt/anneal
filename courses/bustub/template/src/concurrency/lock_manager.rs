//! A lock manager for a table-and-row hierarchy: blocking locks with a fair queue, upgrades, two-phase locking per isolation level,
//! and the hooks the deadlock detector needs.

use std::collections::{HashMap, HashSet};
use std::sync::{Condvar, Mutex};

use crate::catalog::catalog::TableOid;
use crate::common::rid::Rid;
use crate::concurrency::deadlock::WaitsForGraph;
use crate::concurrency::lock_error::{LockError, LockErrorKind};
use crate::concurrency::lock_mode::{compatible, LockMode};
use crate::concurrency::lock_queue::LockQueue;
use crate::concurrency::lock_txn::{LockIsolation, LockState, LockTxn};
use crate::concurrency::transaction::TxnId;

/// Something that can be locked: a whole table, or one row of it.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Resource {
    Table(TableOid),
    Row(TableOid, Rid),
}

// TODO(4d-03): the state behind your lock manager: the locks of each resource, who waits for them in which order, and later what each transaction holds

/// The lock manager. All methods take `&self`: one manager is shared by every transaction (thread).
pub struct LockManager {
    _state: (),
}

impl LockManager {
    /// A lock manager with nothing locked.
    pub fn new() -> LockManager {
        todo!("4d-03: a lock manager with nothing locked")
    }

    /// Takes the lock, waiting if it is not available: `txn` gets `mode` on `resource` once every other holder is compatible with it and no
    /// earlier waiter is incompatible with it, so a stream of readers never starves a waiting writer.
    ///
    /// Asking again for the mode already held is a no-op; asking for a stronger one is an upgrade.
    pub fn lock(&self, txn: TxnId, resource: Resource, mode: LockMode) -> Result<(), LockError> {
        todo!("4d-03: queue the request, wait on the condition variable until it can be granted, then take it")
    }

    /// Takes the lock if that is possible right now, without waiting and without jumping the queue. `Ok(false)` when it is not possible.
    pub fn try_lock(&self, txn: TxnId, resource: Resource, mode: LockMode) -> Result<bool, LockError> {
        todo!("4d-03: grant now or say no, never wait")
    }

    /// Lets go of the lock `txn` holds on `resource` and wakes the waiters.
    pub fn unlock(&self, txn: TxnId, resource: Resource) -> Result<(), LockError> {
        todo!("4d-03: release and wake the transactions that are waiting")
    }

    /// Who holds `resource` and in which mode, ordered by transaction id.
    pub fn holders(&self, resource: Resource) -> Vec<(TxnId, LockMode)> {
        todo!("4d-03: the holders of the resource")
    }

    /// How many requests are waiting on `resource`.
    pub fn waiting(&self, resource: Resource) -> usize {
        todo!("4d-03: the number of requests waiting")
    }

    // ---- two-phase locking and isolation levels (4d-05) ----------------------------------------------------------------------------------

    fn abort(txn: &LockTxn, kind: LockErrorKind) -> LockError {
        txn.set_state(LockState::Aborted);
        LockError::new(txn.id(), kind)
    }

    /// May `txn` ask for `mode` now? The isolation level and the phase of two-phase locking decide.
    fn allowed(txn: &LockTxn, mode: LockMode) -> Result<(), LockError> {
        use LockMode::*;
        match txn.state() {
            LockState::Committed | LockState::Aborted => return Err(LockError::new(txn.id(), LockErrorKind::NotRunning)),
            _ => {}
        }
        todo!("4d-05: the rules of each isolation level while growing and while shrinking")
    }

    /// Takes a table lock for `txn` under the rules of its isolation level, waiting if it is not available.
    ///
    /// # Errors
    ///
    /// Aborts `txn` and returns the reason when a rule is broken (a lock during the shrinking phase, a mode the isolation level forbids, an
    /// invalid upgrade) or when `txn` is chosen as a deadlock victim.
    pub fn lock_table(&self, txn: &LockTxn, mode: LockMode, table: TableOid) -> Result<(), LockError> {
        todo!("4d-05: check the rules, then take the table lock")
    }

    /// Takes a shared or exclusive lock on a row, waiting if it is not available. The table must already be locked in a mode that covers it.
    ///
    /// # Errors
    ///
    /// Aborts `txn` and returns the reason for an intention lock on a row, a missing or too weak table lock, or a broken isolation rule.
    pub fn lock_row(&self, txn: &LockTxn, mode: LockMode, table: TableOid, rid: Rid) -> Result<(), LockError> {
        use LockMode::*;
        todo!("4d-05: only shared or exclusive on a row, and only under a table lock that is enough; then take it")
    }

    /// After a lock is released, the transaction may have to start shrinking: any isolation level after an exclusive lock, and repeatable read
    /// after a shared one too.
    fn after_unlock(txn: &LockTxn, mode: LockMode) {
        todo!("4d-05: move to the shrinking phase when the isolation level says so")
    }

    /// Releases `txn`'s lock on the row. Depending on the isolation level, this may start the shrinking phase.
    ///
    /// # Errors
    ///
    /// Aborts `txn` and returns `AttemptedUnlockButNoLockHeld` if it holds no such lock.
    pub fn unlock_row(&self, txn: &LockTxn, table: TableOid, rid: Rid) -> Result<(), LockError> {
        todo!("4d-05: release the row lock; it may start the shrinking phase")
    }

    /// Releases `txn`'s lock on the table. Depending on the isolation level, this may start the shrinking phase.
    ///
    /// # Errors
    ///
    /// Aborts `txn` if it holds no such lock, or still holds row locks on the table.
    pub fn unlock_table(&self, txn: &LockTxn, table: TableOid) -> Result<(), LockError> {
        todo!("4d-05: refuse while rows of the table are locked; otherwise release it; it may start the shrinking phase")
    }

    /// Everything the transaction holds, rows first, without touching its phase: what commit and abort do.
    pub fn release_all(&self, txn: &LockTxn) {
        todo!("4d-05: let go of every row lock, then every table lock")
    }

    // ---- deadlocks (4d-06) -------------------------------------------------------------------------------------------------------------

    /// Looks for deadlocks among the waiting requests and breaks each cycle by choosing its youngest transaction as the victim: its waiting
    /// `lock` call returns `Deadlock`. Returns the victims, in increasing order. A request waits for the holders it is incompatible with.
    pub fn detect_deadlocks(&self) -> Vec<TxnId> {
        todo!("4d-06: build the waits-for graph from the waiting requests, break each cycle at its youngest transaction, wake the victims")
    }
}

impl Default for LockManager {
    fn default() -> Self {
        LockManager::new()
    }
}
