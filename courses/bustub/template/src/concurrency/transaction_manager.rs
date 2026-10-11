//! Begins, commits and aborts transactions, and keeps for every tuple the **link to its newest undo log**: the head of its version chain.
//!
//! The heads live in a side table owned by the manager (page id, then slot, then [`UndoLink`]), not in the tuple, whose bytes and metadata
//! are all the table page stores. A tuple and its link are always changed together while the page is write-latched
//! ([`update_tuple_and_undo_link`]) and read together under the read latch ([`get_tuple_and_undo_link`]).

use std::collections::{HashMap, HashSet};
use std::sync::atomic::{AtomicI64, Ordering};
use std::sync::{Arc, Mutex, RwLock};

use super::transaction::{INVALID_TXN_ID, IsolationLevel, Timestamp, Transaction, TransactionState, TxnId, UndoLink, UndoLog, TXN_START_ID};
use super::watermark::Watermark;
use crate::catalog::catalog::{Catalog, TableInfo};
use crate::common::config::PageId;
use crate::common::exception::{Exception, ExceptionType, Result};
use crate::common::rid::Rid;
use crate::execution::execution_common::{collect_undo_logs, reconstruct_tuple};
use crate::storage::table::tuple::{Tuple, TupleMeta};

pub struct TransactionManager {
    /// Every transaction, running or finished (until garbage collection forgets the finished ones).
    pub txn_map: RwLock<HashMap<TxnId, Arc<Transaction>>>,
    /// For every tuple that has older versions: page, then slot, then the newest undo log.
    version_info: Mutex<HashMap<PageId, HashMap<u32, UndoLink>>>,
    /// The read timestamps of the running transactions.
    running_txns: Mutex<Watermark>,
    /// Only one transaction commits at a time.
    commit_mutex: Mutex<()>,
    /// The commit timestamp of the newest commit.
    last_commit_ts: AtomicI64,
    next_txn_id: AtomicI64,
    catalog: &'static RwLock<Catalog<'static>>,
}

impl TransactionManager {
    pub fn new(catalog: &'static RwLock<Catalog<'static>>) -> TransactionManager {
        TransactionManager {
            txn_map: RwLock::new(HashMap::new()),
            version_info: Mutex::new(HashMap::new()),
            running_txns: Mutex::new(Watermark::new(0)),
            commit_mutex: Mutex::new(()),
            last_commit_ts: AtomicI64::new(0),
            next_txn_id: AtomicI64::new(TXN_START_ID),
            catalog,
        }
    }

    pub fn catalog(&self) -> &'static RwLock<Catalog<'static>> {
        self.catalog
    }

    pub fn last_commit_ts(&self) -> Timestamp {
        self.last_commit_ts.load(Ordering::SeqCst)
    }

    /// The smallest read timestamp in the system (the last commit timestamp when no transaction runs).
    pub fn get_watermark(&self) -> Timestamp {
        self.running_txns.lock().unwrap().get_watermark()
    }

    /// Begins a transaction: it reads the database as of the newest commit.
    pub fn begin(&self, isolation_level: IsolationLevel) -> Result<Arc<Transaction>> {
        let txn_id = self.next_txn_id.fetch_add(1, Ordering::SeqCst);
        let txn = Arc::new(Transaction::new(txn_id, isolation_level));
        self.txn_map.write().unwrap().insert(txn_id, txn.clone());
        todo!("4a-02: read_ts = the last commit timestamp; register it with the watermark, all under the watermark's lock")
    }

    /// Commits a transaction. Returns `false` (and leaves the transaction as it is) if it is tainted; for a serializable transaction that
    /// fails verification, aborts it and returns `false`.
    pub fn commit(&self, txn: &Arc<Transaction>) -> Result<bool> {
        let _commit = self.commit_mutex.lock().unwrap();
        if txn.state() == TransactionState::Tainted {
            return Ok(false);
        }
        if txn.state() != TransactionState::Running {
            return Err(Exception::new(ExceptionType::Execution, "txn not in running state"));
        }
        if txn.isolation_level() == IsolationLevel::Serializable && !self.verify_txn(txn) {
            drop(_commit);
            self.abort(txn)?;
            return Ok(false);
        }
        todo!("4a-02: the next commit timestamp stamps every tuple of the write set; the transaction becomes committed and the watermark moves on")
    }

    /// Aborts a running or tainted transaction and undoes its writes.
    pub fn abort(&self, txn: &Arc<Transaction>) -> Result<()> {
        if !matches!(txn.state(), TransactionState::Running | TransactionState::Tainted) {
            return Err(Exception::new(ExceptionType::Execution, "txn not in running / tainted state"));
        }
        // 4b-04: for every rid in the write set (the table from the catalog), under the page's write latch (with_page_mut): if the tuple's head link is this transaction's own log, rebuild the old version with reconstruct_tuple from that one log and write it with meta ts = the log's ts (is_deleted if the log says so), then set the link to the log's prev_version (None if invalid); with no own log the tuple was created by this transaction: make it a deleted tuple with ts 0
        todo!("4a-02: mark the transaction aborted and remove its read timestamp from the watermark")
    }

    /// Whether `txn` passes serializable validation and may commit.
    fn verify_txn(&self, txn: &Arc<Transaction>) -> bool {
        true // 4b-07: a transaction with no writes or no scans passes. Otherwise for every transaction that committed after txn.read_ts(), for each tuple in its write set, rebuild the tuple as of commit_ts - 1 and as of commit_ts (a throwaway Transaction with that read ts and collect_undo_logs + reconstruct_tuple); if one of txn's predicates on that table is true for either version, fail
    }

    /// Stop-the-world garbage collection: call it when no transaction is executing. Forgets every finished transaction whose undo logs no
    /// reader can still need.
    pub fn garbage_collection(&self) {
        // 4b-05: watermark = get_watermark(); for every tuple of every table (the catalog's table names, make_eager_iterator): a tuple at or below the watermark needs no chain (clear its link); otherwise walk the chain collecting the owner of each log, stopping after the first log whose ts <= watermark. Then txn_map.retain: keep running and tainted transactions and those collected as needed
    }

    // ---- version chains (given) ------------------------------------------------------------------------------------------------

    /// Points the tuple at `rid` to a new newest undo log (`None`: it has no older versions).
    ///
    /// If `check` is given it sees the current link first and may refuse, in which case nothing changes and the function returns `false`.
    pub fn update_undo_link(&self, rid: Rid, prev_link: Option<UndoLink>, check: Option<&dyn Fn(Option<UndoLink>) -> bool>) -> bool {
        let mut info = self.version_info.lock().unwrap();
        let page = info.entry(rid.page_id()).or_default();
        if let Some(check) = check {
            if !check(page.get(&rid.slot_num()).copied()) {
                return false;
            }
        }
        match prev_link {
            Some(link) => {
                page.insert(rid.slot_num(), link);
            }
            None => {
                page.remove(&rid.slot_num());
            }
        }
        true
    }

    /// The newest undo log of the tuple at `rid`, if it has older versions.
    pub fn get_undo_link(&self, rid: Rid) -> Option<UndoLink> {
        self.version_info.lock().unwrap().get(&rid.page_id())?.get(&rid.slot_num()).copied()
    }

    /// The undo log `link` points to; `None` if the transaction that owns it has been garbage collected.
    pub fn get_undo_log_optional(&self, link: UndoLink) -> Option<UndoLog> {
        let txn = self.txn_map.read().unwrap().get(&link.prev_txn).cloned()?;
        Some(txn.get_undo_log(link.prev_log_idx as usize))
    }

    /// Like [`get_undo_log_optional`](Self::get_undo_log_optional) but a missing log is an error.
    pub fn get_undo_log(&self, link: UndoLink) -> Result<UndoLog> {
        self.get_undo_log_optional(link).ok_or_else(|| Exception::new(ExceptionType::Execution, "undo log not exist"))
    }

    /// The transaction with this id, if it is still remembered.
    pub fn get_txn(&self, id: TxnId) -> Option<Arc<Transaction>> {
        self.txn_map.read().unwrap().get(&id).cloned()
    }

    /// Every rid that has a link, with the link (for debugging and garbage collection).
    pub fn all_undo_links(&self) -> Vec<(Rid, UndoLink)> {
        let info = self.version_info.lock().unwrap();
        info.iter().flat_map(|(page, slots)| slots.iter().map(move |(slot, link)| (Rid::new(*page, *slot), *link))).collect()
    }
}

/// Changes the tuple at `rid` and its undo link **atomically** (under the page's write latch). `check` sees the tuple as it is now and
/// its link and can refuse: then nothing changes and the result is `false`. A tuple that is already equal to the new one is not rewritten.
pub fn update_tuple_and_undo_link(
    txn_mgr: &TransactionManager,
    table: &TableInfo<'_>,
    rid: Rid,
    undo_link: Option<UndoLink>,
    meta: &TupleMeta,
    tuple: &Tuple,
    check: Option<&dyn Fn(&TupleMeta, &Tuple, Rid, Option<UndoLink>) -> bool>,
) -> Result<bool> {
    table.table.with_page_mut(rid, |page| {
        let (base_meta, base_tuple) = page.get_tuple(rid)?;
        if let Some(check) = check {
            if !check(&base_meta, &base_tuple, rid, txn_mgr.get_undo_link(rid)) {
                return Ok(false);
            }
        }
        if *meta != base_meta || tuple.data() != base_tuple.data() {
            page.update_tuple_in_place_unsafe(meta, tuple, rid)?;
        }
        txn_mgr.update_undo_link(rid, undo_link, None);
        Ok(true)
    })
}

/// Reads the tuple at `rid` and its undo link **atomically** (under the page's read latch).
pub fn get_tuple_and_undo_link(txn_mgr: &TransactionManager, table: &TableInfo<'_>, rid: Rid) -> Result<(TupleMeta, Tuple, Option<UndoLink>)> {
    table.table.with_page(rid, |page| {
        let (meta, tuple) = page.get_tuple(rid)?;
        Ok((meta, tuple, txn_mgr.get_undo_link(rid)))
    })
}
