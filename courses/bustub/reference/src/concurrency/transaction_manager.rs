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
        // @begin 4a-02
        // The watermark's lock is held from reading the last commit timestamp to registering the reader: a commit that slips in between
        // would move the watermark past the timestamp this transaction is about to read at.
        let mut running = self.running_txns.lock().unwrap();
        let read_ts = self.last_commit_ts.load(Ordering::SeqCst);
        txn.set_read_ts(read_ts);
        running.add_txn(read_ts)?;
        Ok(txn)
        //~ todo!("4a-02: read_ts = the last commit timestamp; register it with the watermark, all under the watermark's lock")
        // @end
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
        // @begin 4a-02
        let commit_ts = self.last_commit_ts.load(Ordering::SeqCst) + 1;
        let catalog = self.catalog.read().unwrap();
        for (table_oid, rids) in txn.write_sets() {
            let table = catalog.table_info(table_oid).ok_or_else(|| Exception::new(ExceptionType::Execution, "a written table no longer exists"))?;
            for rid in rids {
                table.table.with_page_mut(rid, |page| -> Result<()> {
                    let meta = page.get_tuple_meta(rid)?;
                    page.update_tuple_meta(&TupleMeta { ts: commit_ts, is_deleted: meta.is_deleted }, rid)
                })?;
            }
        }
        txn.set_commit_ts(commit_ts);
        txn.set_state(TransactionState::Committed);
        let mut running = self.running_txns.lock().unwrap();
        // the order matters: a transaction that begins from now on reads at commit_ts, so the watermark must know about the commit first
        running.update_commit_ts(commit_ts);
        self.last_commit_ts.store(commit_ts, Ordering::SeqCst);
        running.remove_txn(txn.read_ts());
        Ok(true)
        //~ todo!("4a-02: commit_ts = last commit + 1; stamp every tuple in the write set with it (keep is_deleted); set the transaction's commit ts and state; then, under the watermark's lock: tell it the commit, publish last_commit_ts, remove this reader")
        // @end
    }

    /// Aborts a running or tainted transaction and undoes its writes.
    pub fn abort(&self, txn: &Arc<Transaction>) -> Result<()> {
        if !matches!(txn.state(), TransactionState::Running | TransactionState::Tainted) {
            return Err(Exception::new(ExceptionType::Execution, "txn not in running / tainted state"));
        }
        // @begin 4b-04
        let catalog = self.catalog.read().unwrap();
        for (table_oid, rids) in txn.write_sets() {
            let Some(table) = catalog.table_info(table_oid) else { continue };
            for rid in rids {
                table.table.with_page_mut(rid, |page| -> Result<()> {
                    let (meta, tuple) = page.get_tuple(rid)?;
                    match self.get_undo_link(rid).filter(|l| l.is_valid() && l.prev_txn == txn.id()) {
                        // changed a tuple that existed: apply its log for good and drop it from the chain
                        Some(link) => {
                            let log = txn.get_undo_log(link.prev_log_idx as usize);
                            let (new_meta, new_tuple) = match reconstruct_tuple(&table.schema, &tuple, &meta, std::slice::from_ref(&log)) {
                                Some(old) => (TupleMeta { ts: log.ts, is_deleted: false }, old),
                                None => (TupleMeta { ts: log.ts, is_deleted: true }, tuple),
                            };
                            page.update_tuple_in_place_unsafe(&new_meta, &new_tuple, rid)?;
                            self.update_undo_link(rid, Some(log.prev_version).filter(|l| l.is_valid()), None);
                        }
                        // a tuple this transaction created: nobody saw it, so it just becomes a tombstone
                        None => page.update_tuple_meta(&TupleMeta { ts: 0, is_deleted: true }, rid)?,
                    }
                    Ok(())
                })?;
            }
        }
        drop(catalog);
        //~ // 4b-04: for every rid in the write set (the table from the catalog), under the page's write latch (with_page_mut): if the tuple's head link is this transaction's own log, rebuild the old version with reconstruct_tuple from that one log and write it with meta ts = the log's ts (is_deleted if the log says so), then set the link to the log's prev_version (None if invalid); with no own log the tuple was created by this transaction: make it a deleted tuple with ts 0
        // @end
        // @begin 4a-02
        txn.set_state(TransactionState::Aborted);
        self.running_txns.lock().unwrap().remove_txn(txn.read_ts());
        Ok(())
        //~ todo!("4a-02: mark the transaction aborted and remove its read timestamp from the watermark")
        // @end
    }

    /// Whether `txn` passes serializable validation and may commit.
    fn verify_txn(&self, txn: &Arc<Transaction>) -> bool {
        // @begin 4b-07
        // a transaction that wrote nothing read one consistent snapshot: it can be serialised at its read timestamp
        let predicates = txn.scan_predicates();
        if txn.write_sets().is_empty() || predicates.is_empty() {
            return true;
        }
        let committed: Vec<Arc<Transaction>> =
            self.txn_map.read().unwrap().values().filter(|t| t.state() == TransactionState::Committed && t.commit_ts() > txn.read_ts()).cloned().collect();
        let catalog = self.catalog.read().unwrap();
        for other in committed {
            for (table_oid, rids) in other.write_sets() {
                let (Some(preds), Some(table)) = (predicates.get(&table_oid), catalog.table_info(table_oid)) else { continue };
                for rid in rids {
                    let Ok((meta, tuple, link)) = get_tuple_and_undo_link(self, table, rid) else { continue };
                    // the tuple as it was just before `other` committed, and as `other` left it
                    for ts in [other.commit_ts() - 1, other.commit_ts()] {
                        let reader = Transaction::new(INVALID_TXN_ID, IsolationLevel::SnapshotIsolation);
                        reader.set_read_ts(ts);
                        let Some(logs) = collect_undo_logs(rid, &meta, &tuple, link, &reader, self) else { continue };
                        let Some(version) = reconstruct_tuple(&table.schema, &tuple, &meta, &logs) else { continue };
                        if preds.iter().any(|p| p.evaluate(&version, &table.schema).map_or(true, |v| v.as_bool() == Some(true))) {
                            return false;
                        }
                    }
                }
            }
        }
        true
        //~ true // 4b-07: a transaction with no writes or no scans passes. Otherwise for every transaction that committed after txn.read_ts(), for each tuple in its write set, rebuild the tuple as of commit_ts - 1 and as of commit_ts (a throwaway Transaction with that read ts and collect_undo_logs + reconstruct_tuple); if one of txn's predicates on that table is true for either version, fail
        // @end
    }

    /// Stop-the-world garbage collection: call it when no transaction is executing. Forgets every finished transaction whose undo logs no
    /// reader can still need.
    pub fn garbage_collection(&self) {
        // @begin 4b-05
        let watermark = self.get_watermark();
        let mut needed: HashSet<TxnId> = HashSet::new();
        let catalog = self.catalog.read().unwrap();
        for name in catalog.get_table_names() {
            let Some(table) = catalog.get_table(&name) else { continue };
            let mut iter = table.table.make_eager_iterator();
            while !iter.is_end() {
                let rid = iter.get_rid();
                iter.advance();
                let Ok((meta, _, link)) = get_tuple_and_undo_link(self, &table, rid) else { continue };
                if meta.ts <= watermark {
                    // every reader sees this version: no log below it matters
                    if link.is_some() {
                        self.update_undo_link(rid, None, None);
                    }
                    continue;
                }
                let mut next = link;
                while let Some(l) = next.filter(|l| l.is_valid()) {
                    let Some(log) = self.get_undo_log_optional(l) else { break };
                    needed.insert(l.prev_txn);
                    // the first log at or below the watermark is the oldest version anybody can ask for
                    if log.ts <= watermark {
                        break;
                    }
                    next = Some(log.prev_version);
                }
            }
        }
        self.txn_map.write().unwrap().retain(|id, t| matches!(t.state(), TransactionState::Running | TransactionState::Tainted) || needed.contains(id));
        //~ // 4b-05: watermark = get_watermark(); for every tuple of every table (the catalog's table names, make_eager_iterator): a tuple at or below the watermark needs no chain (clear its link); otherwise walk the chain collecting the owner of each log, stopping after the first log whose ts <= watermark. Then txn_map.retain: keep running and tainted transactions and those collected as needed
        // @end
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
