//! A tiny transactional record store on table pages: the thing the log protects. Records are fixed-size byte strings living in slots of a few
//! pre-allocated heap pages; a record is addressed by its [`Rid`]. Transactions write records (insert, update, delete); every write is described to
//! the log **before** it reaches the page; a commit makes its log records durable; pages go to disk when somebody calls [`Store::flush_page`], and
//! never otherwise. The store assumes the pool is big enough that nothing is evicted behind its back (a real system hooks eviction).
//!
//! The store is strict: a record written by an active transaction cannot be written by another until the first ends, so a crash never leaves a
//! committed change on top of an uncommitted one.

use std::collections::HashMap;
use std::io;
use std::sync::Mutex;

use crate::buffer::buffer_pool_manager::BufferPoolManager;
use crate::common::config::PageId;
use crate::common::exception::{Exception, ExceptionType};
use crate::common::rid::Rid;
use crate::storage::page::table_page::TablePage;
use crate::storage::table::tuple::{Tuple, TupleMeta};

use super::log_manager::{LogManager, Lsn};
use super::log_record::{LogRecord, TxnId};

/// Every record of the store has exactly this many bytes (the heap page overwrites a tuple only with one of the same size).
pub const RECORD_LEN: usize = 16;

pub type StoreResult<T> = Result<T, Exception>;

pub fn refused(msg: impl Into<String>) -> Exception {
    Exception::new(ExceptionType::Execution, msg.into())
}

pub struct Store<'a> {
    _store: std::marker::PhantomData<&'a ()>,
    // TODO(4c-03): your fields: the pool, the log and the pages (stage 4 adds the transactions' bookkeeping)
}

// TODO(4c-04): helper types of your own

impl<'a> Store<'a> {
    /// A new store of `page_count` empty pages, written to disk (so that they exist after a crash).
    pub fn create(bpm: &'a BufferPoolManager, log: &'a LogManager, page_count: usize) -> Store<'a> {
        let pages: Vec<PageId> = (0..page_count).map(|_| bpm.new_page()).collect();
        for p in &pages {
            let mut guard = bpm.write_page(*p);
            TablePage::new(&mut guard[..]).init();
        }
        for p in &pages {
            bpm.flush_page(*p);
        }
        Store::open(bpm, log, pages, 1)
    }

    /// The store whose pages are `pages` (after a restart: the pages are on disk, the log is what the log manager found). New
    /// transactions are numbered from `first_txn`.
    pub fn open(bpm: &'a BufferPoolManager, log: &'a LogManager, pages: Vec<PageId>, first_txn: TxnId) -> Store<'a> {
        todo!("4c-03: keep the pool, the log and the pages (and, from stage 4, the first transaction id)")
    }

    /// The pages the store was opened with.
    pub fn pages(&self) -> &[PageId] {
        todo!("4c-03: the pages the store was opened with")
    }

    /// The live record at `rid`, if there is one.
    pub fn get(&self, rid: Rid) -> Option<Vec<u8>> {
        todo!("4c-03: under the page's read latch: the tuple at the slot, unless the slot does not exist or is deleted")
    }

    /// Every live record, page by page and slot by slot.
    pub fn scan(&self) -> Vec<(Rid, Vec<u8>)> {
        todo!("4c-03: for every page of the store, every slot that is not deleted, in page order and slot order")
    }

    /// Puts the slot at `rid` in exactly this state, **whatever it holds now** (so doing it twice is the same as once, which is what lets
    /// recovery repeat it): `Some(bytes)`: a live record with those bytes; `None`: no live record (a deleted slot). A slot that does not
    /// exist yet is created, but only the next one in its page (`rid.slot_num() == number of slots`); anything else is an error.
    pub fn set_state(&self, rid: Rid, state: &Option<Vec<u8>>) -> StoreResult<()> {
        todo!("4c-03: makes the slot hold exactly this state, so that doing it twice is the same as once: Some(bytes) a live record, None a deleted slot; a slot beyond the next free one is an error")
    }

    /// Starts a transaction: logs `Begin` and returns its id.
    pub fn begin(&self) -> TxnId {
        todo!("4c-04: take the next id, remember the transaction as active with no changes, log Begin")
    }

    /// Changes the slot at `rid` from `before` to `after` for `txn`: the **write-ahead rule** says the log record comes first, then the
    /// page changes. Refused (and nothing happens) if another active transaction has written the record.
    fn change(&self, txn: TxnId, rid: Rid, before: Option<Vec<u8>>, after: Option<Vec<u8>>) -> StoreResult<()> {
        todo!("4c-04: refused if another transaction holds the record; otherwise the change is logged first, then applied, and remembered for the transaction and the page")
    }

    /// Inserts a record of `RECORD_LEN` bytes into the first page with a free slot, logging the change before the page sees it. Returns the new
    /// record's [`Rid`].
    pub fn insert(&self, txn: TxnId, data: &[u8]) -> StoreResult<Rid> {
        todo!("4c-04: the first page with room: the new record goes in the next slot; log and apply it with change(None -> Some)")
    }

    /// Replaces the live record at `rid`, logging the change first. The record must exist.
    pub fn update(&self, txn: TxnId, rid: Rid, data: &[u8]) -> StoreResult<()> {
        todo!("4c-04: the record must be live; change(Some(old) -> Some(new))")
    }

    /// Deletes the live record at `rid`, logging the change first. The record must exist.
    pub fn delete(&self, txn: TxnId, rid: Rid) -> StoreResult<()> {
        todo!("4c-04: the record must be live; change(Some(old) -> None)")
    }

    /// Commits: logs `Commit` and makes the log durable up to and including it. After this returns the transaction survives a crash.
    pub fn commit(&self, txn: TxnId) -> StoreResult<()> {
        todo!("4c-04: forget the transaction and its holds, log Commit and make it durable (flush_to)")
    }

    /// Rolls the transaction back: its changes are undone in reverse order, **each undo logged** (a change with the two states swapped)
    /// and applied; then `Abort` is logged. Nothing needs to be flushed.
    pub fn abort(&self, txn: TxnId) -> StoreResult<()> {
        todo!("4c-04: for every change, newest first: log the swapped Change, apply the before state; release the holds; log Abort")
    }

    /// Writes the page to disk. The write-ahead rule: the log records that describe the page's contents must be durable first.
    pub fn flush_page(&self, page: PageId) -> io::Result<()> {
        todo!("4c-04: flush the log up to the page's last change, then write the page")
    }

    /// Writes every page of the store to disk.
    pub fn flush_all_pages(&self) -> io::Result<()> {
        todo!("4c-04: flush_page for every page of the store")
    }

    /// A sharp checkpoint: the log and every page go to disk, then a `Checkpoint` record naming the active transactions is logged and made
    /// durable. Returns its LSN.
    pub fn checkpoint(&self) -> io::Result<Lsn> {
        todo!("4c-07: lock out writers; flush the log, then every page; log Checkpoint with the active transactions and flush; return its LSN")
    }

    /// The ids of the active transactions.
    pub fn active_transactions(&self) -> Vec<TxnId> {
        todo!("4c-04: the ids of the transactions that have begun and not ended, sorted")
    }
}
