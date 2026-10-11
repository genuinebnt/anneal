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
    // @begin 4c-03
    bpm: &'a BufferPoolManager,
    log: &'a LogManager,
    pages: Vec<PageId>,
    // @begin 4c-04
    inner: Mutex<Inner>,
    //~ // TODO(4c-04): more fields: the open transactions and what each changed, who holds which record, the LSN of the last change of each page
    // @end
    //~ _store: std::marker::PhantomData<&'a ()>,
    //~ // TODO(4c-03): your fields: the pool, the log and the pages (stage 4 adds the transactions' bookkeeping)
    // @end
}

// @begin 4c-04
#[derive(Default)]
struct Inner {
    next_txn: TxnId,
    /// For every active transaction, its changes in order: (record, state before, state after).
    active: HashMap<TxnId, Vec<(Rid, Option<Vec<u8>>, Option<Vec<u8>>)>>,
    /// The active transaction that has written each record.
    held: HashMap<Rid, TxnId>,
    /// The LSN of the last change logged for each page.
    page_lsn: HashMap<PageId, Lsn>,
}
//~ // TODO(4c-04): helper types of your own
// @end

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
        // @begin 4c-03
        Store {
            bpm,
            log,
            pages,
            // @begin 4c-04
            inner: Mutex::new(Inner { next_txn: first_txn, ..Inner::default() }),
            // @end
        }
        //~ todo!("4c-03: keep the pool, the log and the pages (and, from stage 4, the first transaction id)")
        // @end
    }

    /// The pages the store was opened with.
    pub fn pages(&self) -> &[PageId] {
        // @begin 4c-03
        &self.pages
        //~ todo!("4c-03: the pages the store was opened with")
        // @end
    }

    /// The live record at `rid`, if there is one.
    pub fn get(&self, rid: Rid) -> Option<Vec<u8>> {
        // @begin 4c-03
        let guard = self.bpm.read_page(rid.page_id());
        let page = TablePage::new(&guard[..]);
        let (meta, tuple) = page.get_tuple(rid).ok()?;
        (!meta.is_deleted).then(|| tuple.data().to_vec())
        //~ todo!("4c-03: under the page's read latch: the tuple at the slot, unless the slot does not exist or is deleted")
        // @end
    }

    /// Every live record, page by page and slot by slot.
    pub fn scan(&self) -> Vec<(Rid, Vec<u8>)> {
        // @begin 4c-03
        let mut out = Vec::new();
        for page_id in &self.pages {
            let guard = self.bpm.read_page(*page_id);
            let page = TablePage::new(&guard[..]);
            for slot in 0..page.get_num_tuples() {
                let rid = Rid::new(*page_id, slot);
                if let Ok((meta, tuple)) = page.get_tuple(rid) {
                    if !meta.is_deleted {
                        out.push((rid, tuple.data().to_vec()));
                    }
                }
            }
        }
        out
        //~ todo!("4c-03: for every page of the store, every slot that is not deleted, in page order and slot order")
        // @end
    }

    /// Puts the slot at `rid` in exactly this state, **whatever it holds now** (so doing it twice is the same as once, which is what lets
    /// recovery repeat it): `Some(bytes)`: a live record with those bytes; `None`: no live record (a deleted slot). A slot that does not
    /// exist yet is created, but only the next one in its page (`rid.slot_num() == number of slots`); anything else is an error.
    pub fn set_state(&self, rid: Rid, state: &Option<Vec<u8>>) -> StoreResult<()> {
        // @begin 4c-03
        let mut guard = self.bpm.write_page(rid.page_id());
        let mut page = TablePage::new(&mut guard[..]);
        let slots = page.get_num_tuples();
        match state {
            Some(bytes) => {
                if bytes.len() != RECORD_LEN {
                    return Err(refused(format!("a record has {RECORD_LEN} bytes, not {}", bytes.len())));
                }
                let tuple = Tuple::from_bytes(rid, bytes);
                let live = TupleMeta { ts: 0, is_deleted: false };
                if rid.slot_num() < slots {
                    page.update_tuple_in_place_unsafe(&live, &tuple, rid)
                } else if rid.slot_num() == slots {
                    match page.insert_tuple(&live, &tuple) {
                        Some(slot) if slot as u32 == rid.slot_num() => Ok(()),
                        _ => Err(refused("the page has no room for the record")),
                    }
                } else {
                    Err(refused("a slot can only be created right after the last one"))
                }
            }
            None => {
                if rid.slot_num() >= slots {
                    return Err(refused("no such slot"));
                }
                page.update_tuple_meta(&TupleMeta { ts: 0, is_deleted: true }, rid)
            }
        }
        //~ todo!("4c-03: makes the slot hold exactly this state, so that doing it twice is the same as once: Some(bytes) a live record, None a deleted slot; a slot beyond the next free one is an error")
        // @end
    }

    /// Starts a transaction: logs `Begin` and returns its id.
    pub fn begin(&self) -> TxnId {
        // @begin 4c-04
        let mut inner = self.inner.lock().unwrap();
        let txn = inner.next_txn;
        inner.next_txn += 1;
        inner.active.insert(txn, Vec::new());
        self.log.append(&LogRecord::Begin { txn });
        txn
        //~ todo!("4c-04: take the next id, remember the transaction as active with no changes, log Begin")
        // @end
    }

    /// Changes the slot at `rid` from `before` to `after` for `txn`: the **write-ahead rule** says the log record comes first, then the
    /// page changes. Refused (and nothing happens) if another active transaction has written the record.
    fn change(&self, txn: TxnId, rid: Rid, before: Option<Vec<u8>>, after: Option<Vec<u8>>) -> StoreResult<()> {
        // @begin 4c-04
        let mut inner = self.inner.lock().unwrap();
        if !inner.active.contains_key(&txn) {
            return Err(refused("no such active transaction"));
        }
        if inner.held.get(&rid).is_some_and(|owner| *owner != txn) {
            return Err(refused("the record is written by another active transaction"));
        }
        let lsn = self.log.append(&LogRecord::Change { txn, rid, before: before.clone(), after: after.clone() });
        if let Err(e) = self.set_state(rid, &after) {
            // the log says it happened but the page refused: say the opposite right away so that redo and undo agree with the page
            self.log.append(&LogRecord::Change { txn, rid, before: after, after: before });
            return Err(e);
        }
        inner.page_lsn.insert(rid.page_id(), lsn);
        inner.held.insert(rid, txn);
        inner.active.get_mut(&txn).unwrap().push((rid, before, after));
        Ok(())
        //~ todo!("4c-04: refused if another transaction holds the record; otherwise the change is logged first, then applied, and remembered for the transaction and the page")
        // @end
    }

    /// Inserts a record of `RECORD_LEN` bytes into the first page with a free slot, logging the change before the page sees it. Returns the new
    /// record's [`Rid`].
    pub fn insert(&self, txn: TxnId, data: &[u8]) -> StoreResult<Rid> {
        // @begin 4c-04
        if data.len() != RECORD_LEN {
            return Err(refused(format!("a record has {RECORD_LEN} bytes, not {}", data.len())));
        }
        for page_id in &self.pages {
            let slot = {
                let guard = self.bpm.read_page(*page_id);
                let page = TablePage::new(&guard[..]);
                let fits = page.get_next_tuple_offset(&TupleMeta { ts: 0, is_deleted: false }, &Tuple::from_bytes(Rid::default(), data)).is_some();
                fits.then(|| page.get_num_tuples())
            };
            if let Some(slot) = slot {
                let rid = Rid::new(*page_id, slot);
                self.change(txn, rid, None, Some(data.to_vec()))?;
                return Ok(rid);
            }
        }
        Err(refused("the store is full"))
        //~ todo!("4c-04: the first page with room: the new record goes in the next slot; log and apply it with change(None -> Some)")
        // @end
    }

    /// Replaces the live record at `rid`, logging the change first. The record must exist.
    pub fn update(&self, txn: TxnId, rid: Rid, data: &[u8]) -> StoreResult<()> {
        // @begin 4c-04
        let before = self.get(rid).ok_or_else(|| refused("no such record"))?;
        self.change(txn, rid, Some(before), Some(data.to_vec()))
        //~ todo!("4c-04: the record must be live; change(Some(old) -> Some(new))")
        // @end
    }

    /// Deletes the live record at `rid`, logging the change first. The record must exist.
    pub fn delete(&self, txn: TxnId, rid: Rid) -> StoreResult<()> {
        // @begin 4c-04
        let before = self.get(rid).ok_or_else(|| refused("no such record"))?;
        self.change(txn, rid, Some(before), None)
        //~ todo!("4c-04: the record must be live; change(Some(old) -> None)")
        // @end
    }

    /// Commits: logs `Commit` and makes the log durable up to and including it. After this returns the transaction survives a crash.
    pub fn commit(&self, txn: TxnId) -> StoreResult<()> {
        // @begin 4c-04
        let mut inner = self.inner.lock().unwrap();
        if inner.active.remove(&txn).is_none() {
            return Err(refused("no such active transaction"));
        }
        inner.held.retain(|_, owner| *owner != txn);
        let lsn = self.log.append(&LogRecord::Commit { txn });
        self.log.flush_to(lsn).map_err(|e| refused(e.to_string()))
        //~ todo!("4c-04: forget the transaction and its holds, log Commit and make it durable (flush_to)")
        // @end
    }

    /// Rolls the transaction back: its changes are undone in reverse order, **each undo logged** (a change with the two states swapped)
    /// and applied; then `Abort` is logged. Nothing needs to be flushed.
    pub fn abort(&self, txn: TxnId) -> StoreResult<()> {
        // @begin 4c-04
        let mut inner = self.inner.lock().unwrap();
        let Some(changes) = inner.active.remove(&txn) else { return Err(refused("no such active transaction")) };
        for (rid, before, after) in changes.into_iter().rev() {
            let lsn = self.log.append(&LogRecord::Change { txn, rid, before: after.clone(), after: before.clone() });
            self.set_state(rid, &before)?;
            inner.page_lsn.insert(rid.page_id(), lsn);
        }
        inner.held.retain(|_, owner| *owner != txn);
        self.log.append(&LogRecord::Abort { txn });
        Ok(())
        //~ todo!("4c-04: for every change, newest first: log the swapped Change, apply the before state; release the holds; log Abort")
        // @end
    }

    /// Writes the page to disk. The write-ahead rule: the log records that describe the page's contents must be durable first.
    pub fn flush_page(&self, page: PageId) -> io::Result<()> {
        // @begin 4c-04
        let lsn = self.inner.lock().unwrap().page_lsn.get(&page).copied();
        if let Some(lsn) = lsn {
            self.log.flush_to(lsn)?;
        }
        self.bpm.flush_page(page);
        Ok(())
        //~ todo!("4c-04: flush the log up to the page's last change, then write the page")
        // @end
    }

    /// Writes every page of the store to disk.
    pub fn flush_all_pages(&self) -> io::Result<()> {
        // @begin 4c-04
        for page in self.pages().to_vec() {
            self.flush_page(page)?;
        }
        Ok(())
        //~ todo!("4c-04: flush_page for every page of the store")
        // @end
    }

    /// A sharp checkpoint: the log and every page go to disk, then a `Checkpoint` record naming the active transactions is logged and made
    /// durable. Returns its LSN.
    pub fn checkpoint(&self) -> io::Result<Lsn> {
        // @begin 4c-07
        let inner = self.inner.lock().unwrap();
        self.log.flush()?;
        for page in &self.pages {
            if let Some(lsn) = inner.page_lsn.get(page) {
                self.log.flush_to(*lsn)?;
            }
            self.bpm.flush_page(*page);
        }
        let mut active: Vec<TxnId> = inner.active.keys().copied().collect();
        active.sort();
        let lsn = self.log.append(&LogRecord::Checkpoint { active });
        self.log.flush()?;
        Ok(lsn)
        //~ todo!("4c-07: lock out writers; flush the log, then every page; log Checkpoint with the active transactions and flush; return its LSN")
        // @end
    }

    /// The ids of the active transactions.
    pub fn active_transactions(&self) -> Vec<TxnId> {
        // @begin 4c-04
        let mut v: Vec<TxnId> = self.inner.lock().unwrap().active.keys().copied().collect();
        v.sort();
        v
        //~ todo!("4c-04: the ids of the transactions that have begun and not ended, sorted")
        // @end
    }
}
