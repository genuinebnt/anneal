//! A key-value store with snapshot reads and locked writes.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use crate::catalog::catalog::TableOid;
use crate::common::config::PageId;
use crate::common::rid::Rid;
use crate::concurrency::lock_error::LockError;
use crate::concurrency::lock_manager::LockManager;
use crate::concurrency::lock_mode::LockMode;
use crate::concurrency::lock_txn::{LockIsolation, LockTxn};
use crate::concurrency::transaction::TxnId;

const TABLE: TableOid = 1;

/// A running transaction: its id, the commit timestamp it reads as of, and its handle for the lock manager.
pub struct HybridTxn {
    pub id: TxnId,
    pub read_ts: u64,
    pub lock: Arc<LockTxn>,
}

fn rid(key: u32) -> Rid {
    Rid::new(PageId(0), key)
}


pub struct HybridStore {
    _hybrid: (),
}

impl HybridStore {
    /// An empty store that locks through `locks`.
    pub fn new(locks: Arc<LockManager>) -> HybridStore {
        let _ = locks;
        todo!("4d-c6: an empty store that uses this lock manager")
    }

    /// Starts a transaction that reads as of the latest commit.
    pub fn begin(&self) -> HybridTxn {
        todo!("4d-c6: a new transaction that reads as of the latest commit")
    }

    /// The value `txn` sees: its own write, else the newest version committed at or before its read timestamp.
    pub fn get(&self, txn: &HybridTxn, key: u32) -> Option<i64> {
        todo!("4d-c6: a snapshot read: no lock, no waiting")
    }

    /// Takes IX on the table and X on the row; may wait, may be chosen as a deadlock victim.
    fn lock_for_write(&self, txn: &HybridTxn, key: u32) -> Result<(), LockError> {
        todo!("4d-c6: the table intention lock, then the row lock")
    }

    /// Locks the row, waiting if needed, then buffers the write until commit.
    ///
    /// # Errors
    ///
    /// The lock manager's error, for example when the transaction is chosen as a deadlock victim.
    pub fn put(&self, txn: &HybridTxn, key: u32, value: i64) -> Result<(), LockError> {
        todo!("4d-c6: lock the row (waiting if needed), then buffer the write")
    }

    /// Like `get`, but locks the row first and returns the newest committed value, not the snapshot value.
    pub fn get_for_update(&self, txn: &HybridTxn, key: u32) -> Result<Option<i64>, LockError> {
        todo!("4d-c6: lock first; then read the newest committed version, whatever the snapshot says")
    }

    /// Installs the writes at a new commit timestamp, then lets go of every lock. Returns the timestamp.
    pub fn commit(&self, txn: HybridTxn) -> u64 {
        todo!("4d-c6: install the buffered writes at a new timestamp, then release the locks")
    }

    /// Throws the writes away and lets go of every lock.
    pub fn abort(&self, txn: HybridTxn) {
        todo!("4d-c6: forget the writes, release the locks")
    }
}
