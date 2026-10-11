//! Crash recovery, ARIES-style but small. After a restart the pages on disk hold *some* of the changes the log describes (any page may
//! have been written before the crash or not) and the log holds every change whose records were flushed. Recovery **repeats history**
//! (redo: put every logged change in place, in log order, whether or not its transaction finished) and then **undoes the losers**
//! (the transactions that neither committed nor finished rolling back), logging each undo like a rollback does.

use std::collections::HashSet;
use std::io;

use super::log_manager::{LogManager, Lsn};
use super::log_record::{LogRecord, TxnId};
use super::store::{Store, StoreResult};

/// What recovery did.
#[derive(Debug, Default, PartialEq, Eq)]
pub struct Recovery {
    /// Transactions that finished before the crash (committed or rolled back).
    pub finished: Vec<TxnId>,
    /// Transactions that were still active: rolled back by recovery.
    pub losers: Vec<TxnId>,
    /// How many logged changes were re-applied.
    pub redone: usize,
    /// How many changes were undone.
    pub undone: usize,
}

/// What the log says about who was running.
#[derive(Debug, Default, PartialEq, Eq)]
pub struct Analysis {
    /// Index (into the records) of the first record redo has to apply: 0, or the last checkpoint's.
    pub redo_from: usize,
    /// Transactions with a `Commit` or an `Abort` record.
    pub finished: Vec<TxnId>,
    /// Transactions that began and did not finish, in the order they began.
    pub losers: Vec<TxnId>,
}

/// Reads the records and decides who finished and who did not, and where redo starts: at record 0, or at the last checkpoint.
pub fn analyse(records: &[(Lsn, LogRecord)]) -> Analysis {
    todo!("4c-05: who appears in the log and never finished (Commit or Abort); redo starts at record 0")
}

/// Re-applies every change from `records[from..]`, in order, so that the pages end up as if nothing had been lost. Returns how many.
pub fn redo(store: &Store, records: &[(Lsn, LogRecord)], from: usize) -> StoreResult<usize> {
    todo!("4c-05: for every Change record from `from` on, in order: put the slot in its `after` state")
}

/// Rolls back every loser: their changes, newest first across all losers, each undo logged as a change with the states swapped and applied;
/// then `Abort` is logged for each, and the log is flushed. Returns how many changes were undone.
pub fn undo(store: &Store, log: &LogManager, records: &[(Lsn, LogRecord)], losers: &[TxnId]) -> StoreResult<usize> {
    todo!("4c-06: every loser's changes are undone newest first, each undo logged as the swapped change and applied; then Abort is logged and the log flushed")
}

/// Brings the store back to the state of the committed transactions: analyse, redo, undo, then write everything to disk.
pub fn recover(store: &Store, log: &LogManager) -> io::Result<Recovery> {
    let records = log.records()?;
    let analysis = analyse(&records);
    let fail = |e: crate::common::exception::Exception| io::Error::other(e.to_string());
    let redone = redo(store, &records, analysis.redo_from).map_err(fail)?;
    let undone = undo(store, log, &records, &analysis.losers).map_err(fail)?;
    store.flush_all_pages()?;
    Ok(Recovery { finished: analysis.finished, losers: analysis.losers, redone, undone })
}
