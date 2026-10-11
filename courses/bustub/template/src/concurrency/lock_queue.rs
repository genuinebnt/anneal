//! The locks held on one thing (a table or a row), without any waiting: who holds which mode.

use crate::concurrency::lock_error::LockErrorKind;
use crate::concurrency::lock_mode::{can_upgrade, compatible, LockMode};
use crate::concurrency::transaction::TxnId;

/// The holders of one lock. All holders are always compatible with each other.
pub struct LockQueue {
    _holders: (),
}

impl LockQueue {
    /// A queue with no holder.
    pub fn new() -> LockQueue {
        todo!("4d-02: nobody holds anything")
    }

    /// `txn` asks for `mode`. `Ok(true)`: it holds it now (a new lock, the same lock again, or an upgrade of the one it had). `Ok(false)`: some other
    /// holder is in the way, and nothing changed. `Err(IncompatibleUpgrade)`: it holds a mode that `mode` is not an upgrade of.
    pub fn try_acquire(&mut self, txn: TxnId, mode: LockMode) -> Result<bool, LockErrorKind> {
        todo!("4d-02: granted when no other holder conflicts; the same mode again is fine; another mode only if it is an upgrade")
    }

    /// `txn` lets go. False when it held nothing.
    pub fn release(&mut self, txn: TxnId) -> bool {
        todo!("4d-02: forget the holder")
    }

    /// Everyone who holds the lock and in which mode, ordered by transaction id.
    pub fn holders(&self) -> Vec<(TxnId, LockMode)> {
        todo!("4d-02: the holders, by transaction id")
    }

    /// The mode `txn` holds, if any.
    pub fn mode_of(&self, txn: TxnId) -> Option<LockMode> {
        todo!("4d-02: the mode this transaction holds, if any")
    }

    pub fn is_empty(&self) -> bool {
        self.holders().is_empty()
    }
}

impl Default for LockQueue {
    fn default() -> Self {
        LockQueue::new()
    }
}
