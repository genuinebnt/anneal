//! A transaction, its timestamps, and the **undo logs** that let other transactions see older versions of the tuples it changed.
//!
//! A *committed* tuple carries the commit timestamp of the transaction that wrote it (small numbers: 0, 1, 2, ...). A tuple written by a
//! transaction that has not committed yet carries that transaction's **temporary timestamp**, its transaction id, which is at least
//! [`TXN_START_ID`] (2^62) so that it can never be mistaken for a commit timestamp.

use std::collections::{HashMap, HashSet};
use std::sync::atomic::{AtomicI64, Ordering};
use std::sync::{Arc, Mutex};

use crate::common::rid::Rid;
use crate::execution::expressions::abstract_expression::ExprRef;
use crate::catalog::catalog::TableOid;
use crate::storage::table::tuple::Tuple;

pub type TxnId = i64;
pub type Timestamp = i64;

/// The first transaction id; also the offset between a transaction's id and its "human readable" number.
pub const TXN_START_ID: TxnId = 1 << 62;
pub const INVALID_TXN_ID: TxnId = -1;
pub const INVALID_TS: Timestamp = -1;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TransactionState {
    Running,
    /// The transaction hit a write-write conflict: it can only be aborted.
    Tainted,
    Committed,
    Aborted,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum IsolationLevel {
    ReadUncommitted,
    SnapshotIsolation,
    Serializable,
}

/// Where an undo log is: transaction `prev_txn`'s log number `prev_log_idx`. An invalid link ends a version chain.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct UndoLink {
    pub prev_txn: TxnId,
    pub prev_log_idx: i32,
}

impl Default for UndoLink {
    fn default() -> UndoLink {
        UndoLink { prev_txn: INVALID_TXN_ID, prev_log_idx: 0 }
    }
}

impl UndoLink {
    pub fn is_valid(&self) -> bool {
        self.prev_txn != INVALID_TXN_ID
    }
}

/// How to turn the version of a tuple that follows this log back into the version before it.
#[derive(Clone, Debug)]
pub struct UndoLog {
    /// The older version is a **deleted** tuple (applying the log makes the tuple not exist).
    pub is_deleted: bool,
    /// Which columns of the tuple the log restores.
    pub modified_fields: Vec<bool>,
    /// The old values of exactly the modified columns, laid out as a tuple of those columns only (the *partial schema*).
    pub tuple: Tuple,
    /// The timestamp of the version this log restores.
    pub ts: Timestamp,
    /// The next older log of the same tuple.
    pub prev_version: UndoLink,
}

struct Inner {
    undo_logs: Vec<UndoLog>,
    write_set: HashMap<TableOid, HashSet<Rid>>,
    scan_predicates: HashMap<TableOid, Vec<ExprRef>>,
}

pub struct Transaction {
    isolation_level: IsolationLevel,
    txn_id: TxnId,
    state: Mutex<TransactionState>,
    read_ts: AtomicI64,
    commit_ts: AtomicI64,
    inner: Mutex<Inner>,
}

impl std::fmt::Debug for Transaction {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Transaction(txn{} {:?} read_ts={})", self.human_readable_id(), self.state(), self.read_ts())
    }
}

impl Transaction {
    pub fn new(txn_id: TxnId, isolation_level: IsolationLevel) -> Transaction {
        Transaction {
            isolation_level,
            txn_id,
            state: Mutex::new(TransactionState::Running),
            read_ts: AtomicI64::new(0),
            commit_ts: AtomicI64::new(INVALID_TS),
            inner: Mutex::new(Inner { undo_logs: vec![], write_set: HashMap::new(), scan_predicates: HashMap::new() }),
        }
    }

    pub fn id(&self) -> TxnId {
        self.txn_id
    }

    /// The id without the `TXN_START_ID` offset: 0, 1, 2, ... in order of beginning.
    pub fn human_readable_id(&self) -> TxnId {
        self.txn_id ^ TXN_START_ID
    }

    /// The timestamp this transaction's uncommitted writes carry: its id.
    pub fn temp_ts(&self) -> Timestamp {
        self.txn_id
    }

    pub fn isolation_level(&self) -> IsolationLevel {
        self.isolation_level
    }

    pub fn state(&self) -> TransactionState {
        *self.state.lock().unwrap()
    }

    pub(crate) fn set_state(&self, state: TransactionState) {
        *self.state.lock().unwrap() = state;
    }

    /// The commit timestamp of the newest transaction that had committed when this one began: what this transaction sees.
    pub fn read_ts(&self) -> Timestamp {
        self.read_ts.load(Ordering::SeqCst)
    }

    pub(crate) fn set_read_ts(&self, ts: Timestamp) {
        self.read_ts.store(ts, Ordering::SeqCst);
    }

    /// The timestamp this transaction committed at (`INVALID_TS` before it commits).
    pub fn commit_ts(&self) -> Timestamp {
        self.commit_ts.load(Ordering::SeqCst)
    }

    pub(crate) fn set_commit_ts(&self, ts: Timestamp) {
        self.commit_ts.store(ts, Ordering::SeqCst);
    }

    /// Marks the transaction as having hit a conflict (`Running` to `Tainted`). Anything else is a bug.
    pub fn set_tainted(&self) {
        let mut state = self.state.lock().unwrap();
        assert_eq!(*state, TransactionState::Running, "transaction not in running state: {:?}", *state);
        *state = TransactionState::Tainted;
    }

    pub fn modify_undo_log(&self, log_idx: usize, new_log: UndoLog) {
        self.inner.lock().unwrap().undo_logs[log_idx] = new_log;
    }

    /// Adds an undo log and returns the link to it.
    pub fn append_undo_log(&self, log: UndoLog) -> UndoLink {
        let mut inner = self.inner.lock().unwrap();
        inner.undo_logs.push(log);
        UndoLink { prev_txn: self.txn_id, prev_log_idx: inner.undo_logs.len() as i32 - 1 }
    }

    /// Remembers that this transaction wrote the tuple at `rid` (commit stamps it; abort undoes it).
    pub fn append_write_set(&self, table: TableOid, rid: Rid) {
        self.inner.lock().unwrap().write_set.entry(table).or_default().insert(rid);
    }

    pub fn write_sets(&self) -> HashMap<TableOid, HashSet<Rid>> {
        self.inner.lock().unwrap().write_set.clone()
    }

    /// Remembers a predicate this transaction scanned a table with (for serializable validation).
    pub fn append_scan_predicate(&self, table: TableOid, predicate: ExprRef) {
        let mut inner = self.inner.lock().unwrap();
        let predicates = inner.scan_predicates.entry(table).or_default();
        // a scan that is initialised again (the inner side of a join) records its predicate once
        if !predicates.iter().any(|p| Arc::ptr_eq(p, &predicate)) {
            predicates.push(predicate);
        }
    }

    pub fn scan_predicates(&self) -> HashMap<TableOid, Vec<ExprRef>> {
        self.inner.lock().unwrap().scan_predicates.clone()
    }

    pub fn get_undo_log(&self, log_id: usize) -> UndoLog {
        self.inner.lock().unwrap().undo_logs[log_id].clone()
    }

    pub fn undo_log_num(&self) -> usize {
        self.inner.lock().unwrap().undo_logs.len()
    }

    pub fn clear_undo_log(&self) {
        self.inner.lock().unwrap().undo_logs.clear();
    }
}
