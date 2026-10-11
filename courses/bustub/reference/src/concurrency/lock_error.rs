//! Why a lock request or release was refused.

use std::fmt;

use crate::concurrency::transaction::TxnId;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LockErrorKind {
    /// A lock the isolation level does not allow once the transaction has started releasing locks.
    LockOnShrinking,
    /// Read uncommitted never takes shared locks.
    LockSharedOnReadUncommitted,
    /// Another transaction is already waiting to upgrade the same lock.
    UpgradeConflict,
    /// The requested mode is not a stronger mode than the one held.
    IncompatibleUpgrade,
    /// A row can only be locked shared or exclusive.
    AttemptedIntentionLockOnRow,
    /// A row lock needs a suitable lock on its table first.
    TableLockNotPresent,
    /// Unlocking something that is not locked by the transaction.
    AttemptedUnlockButNoLockHeld,
    /// A table cannot be unlocked while rows of it are still locked.
    TableUnlockedBeforeUnlockingRows,
    /// The transaction was chosen to be aborted to break a deadlock.
    Deadlock,
    /// The transaction is already committed or aborted.
    NotRunning,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LockError {
    pub txn: TxnId,
    pub kind: LockErrorKind,
}

impl LockError {
    pub fn new(txn: TxnId, kind: LockErrorKind) -> LockError {
        LockError { txn, kind }
    }
}

impl fmt::Display for LockError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "transaction {} aborted: {:?}", self.txn, self.kind)
    }
}

impl std::error::Error for LockError {}
