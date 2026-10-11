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

// @begin 4d-c6
struct State {
    next_id: TxnId,
    clock: u64,
    /// Committed versions per key, in commit order: `(commit_ts, value)`.
    versions: HashMap<u32, Vec<(u64, i64)>>,
    /// Buffered writes of running transactions.
    pending: HashMap<TxnId, HashMap<u32, i64>>,
}
// @end

pub struct HybridStore {
    // @begin 4d-c6
    locks: Arc<LockManager>,
    state: Mutex<State>,
    //~ _hybrid: (),
    // @end
}

impl HybridStore {
    /// An empty store that locks through `locks`.
    pub fn new(locks: Arc<LockManager>) -> HybridStore {
        // @begin 4d-c6
        HybridStore { locks, state: Mutex::new(State { next_id: 1, clock: 0, versions: HashMap::new(), pending: HashMap::new() }) }
        //~ let _ = locks;
        //~ todo!("4d-c6: an empty store that uses this lock manager")
        // @end
    }

    /// Starts a transaction that reads as of the latest commit.
    pub fn begin(&self) -> HybridTxn {
        // @begin 4d-c6
        let mut st = self.state.lock().unwrap();
        let id = st.next_id;
        st.next_id += 1;
        st.pending.insert(id, HashMap::new());
        HybridTxn { id, read_ts: st.clock, lock: LockTxn::new(id, LockIsolation::RepeatableRead) }
        //~ todo!("4d-c6: a new transaction that reads as of the latest commit")
        // @end
    }

    /// The value `txn` sees: its own write, else the newest version committed at or before its read timestamp.
    pub fn get(&self, txn: &HybridTxn, key: u32) -> Option<i64> {
        // @begin 4d-c6
        let st = self.state.lock().unwrap();
        if let Some(v) = st.pending.get(&txn.id).and_then(|w| w.get(&key)) {
            return Some(*v);
        }
        st.versions.get(&key)?.iter().rev().find(|(ts, _)| *ts <= txn.read_ts).map(|(_, v)| *v)
        //~ todo!("4d-c6: a snapshot read: no lock, no waiting")
        // @end
    }

    /// Takes IX on the table and X on the row; may wait, may be chosen as a deadlock victim.
    fn lock_for_write(&self, txn: &HybridTxn, key: u32) -> Result<(), LockError> {
        // @begin 4d-c6
        self.locks.lock_table(&txn.lock, LockMode::IntentionExclusive, TABLE)?;
        self.locks.lock_row(&txn.lock, LockMode::Exclusive, TABLE, rid(key))
        //~ todo!("4d-c6: the table intention lock, then the row lock")
        // @end
    }

    /// Locks the row, waiting if needed, then buffers the write until commit.
    ///
    /// # Errors
    ///
    /// The lock manager's error, for example when the transaction is chosen as a deadlock victim.
    pub fn put(&self, txn: &HybridTxn, key: u32, value: i64) -> Result<(), LockError> {
        // @begin 4d-c6
        self.lock_for_write(txn, key)?;
        self.state.lock().unwrap().pending.entry(txn.id).or_default().insert(key, value);
        Ok(())
        //~ todo!("4d-c6: lock the row (waiting if needed), then buffer the write")
        // @end
    }

    /// Like `get`, but locks the row first and returns the newest committed value, not the snapshot value.
    pub fn get_for_update(&self, txn: &HybridTxn, key: u32) -> Result<Option<i64>, LockError> {
        // @begin 4d-c6
        self.lock_for_write(txn, key)?;
        let st = self.state.lock().unwrap();
        if let Some(v) = st.pending.get(&txn.id).and_then(|w| w.get(&key)) {
            return Ok(Some(*v));
        }
        Ok(st.versions.get(&key).and_then(|v| v.last()).map(|(_, v)| *v))
        //~ todo!("4d-c6: lock first; then read the newest committed version, whatever the snapshot says")
        // @end
    }

    /// Installs the writes at a new commit timestamp, then lets go of every lock. Returns the timestamp.
    pub fn commit(&self, txn: HybridTxn) -> u64 {
        // @begin 4d-c6
        let ts = {
            let mut st = self.state.lock().unwrap();
            st.clock += 1;
            let ts = st.clock;
            for (key, value) in st.pending.remove(&txn.id).unwrap_or_default() {
                st.versions.entry(key).or_default().push((ts, value));
            }
            ts
        };
        txn.lock.set_state(crate::concurrency::lock_txn::LockState::Committed);
        self.locks.release_all(&txn.lock);
        ts
        //~ todo!("4d-c6: install the buffered writes at a new timestamp, then release the locks")
        // @end
    }

    /// Throws the writes away and lets go of every lock.
    pub fn abort(&self, txn: HybridTxn) {
        // @begin 4d-c6
        self.state.lock().unwrap().pending.remove(&txn.id);
        txn.lock.set_state(crate::concurrency::lock_txn::LockState::Aborted);
        self.locks.release_all(&txn.lock);
        //~ todo!("4d-c6: forget the writes, release the locks")
        // @end
    }
}
