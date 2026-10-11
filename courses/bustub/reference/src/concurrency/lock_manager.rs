//! A lock manager for a table-and-row hierarchy: blocking locks with a fair queue, upgrades, two-phase locking per isolation level,
//! and the hooks the deadlock detector needs.

use std::collections::{HashMap, HashSet};
use std::sync::{Condvar, Mutex};

use crate::catalog::catalog::TableOid;
use crate::common::rid::Rid;
use crate::concurrency::deadlock::WaitsForGraph;
use crate::concurrency::lock_error::{LockError, LockErrorKind};
use crate::concurrency::lock_mode::{compatible, LockMode};
use crate::concurrency::lock_queue::LockQueue;
use crate::concurrency::lock_txn::{LockIsolation, LockState, LockTxn};
use crate::concurrency::transaction::TxnId;

/// Something that can be locked: a whole table, or one row of it.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Resource {
    Table(TableOid),
    Row(TableOid, Rid),
}

// @begin 4d-03
/// A request that has to wait.
struct Waiter {
    txn: TxnId,
    mode: LockMode,
    /// The transaction already holds a weaker mode and wants to replace it: it goes before everyone who is not upgrading.
    upgrade: bool,
}

/// Everything about one resource: who holds it, who waits (in order), and who is waiting to upgrade.
#[derive(Default)]
struct Queue {
    holders: LockQueue,
    waiting: Vec<Waiter>,
    upgrading: Option<TxnId>,
}

/// What a transaction holds, for the two-phase-locking rules.
#[derive(Default)]
struct Held {
    tables: HashMap<TableOid, LockMode>,
    rows: HashMap<(TableOid, Rid), LockMode>,
}

#[derive(Default)]
struct State {
    queues: HashMap<Resource, Queue>,
    held: HashMap<TxnId, Held>,
    /// Transactions the deadlock detector chose; each one's waiting `lock` call gives up with `Deadlock`.
    victims: HashSet<TxnId>,
}
//~ // TODO(4d-03): the state behind your lock manager: the locks of each resource, who waits for them in which order, and later what each transaction holds
// @end

/// The lock manager. All methods take `&self`: one manager is shared by every transaction (thread).
pub struct LockManager {
    // @begin 4d-03
    state: Mutex<State>,
    changed: Condvar,
    //~ _state: (),
    // @end
}

impl LockManager {
    /// A lock manager with nothing locked.
    pub fn new() -> LockManager {
        // @begin 4d-03
        LockManager { state: Mutex::new(State::default()), changed: Condvar::new() }
        //~ todo!("4d-03: a lock manager with nothing locked")
        // @end
    }

    /// Takes the lock, waiting if it is not available: `txn` gets `mode` on `resource` once every other holder is compatible with it and no
    /// earlier waiter is incompatible with it, so a stream of readers never starves a waiting writer.
    ///
    /// Asking again for the mode already held is a no-op; asking for a stronger one is an upgrade.
    pub fn lock(&self, txn: TxnId, resource: Resource, mode: LockMode) -> Result<(), LockError> {
        // @begin 4d-03
        let mut st = self.state.lock().unwrap();
        // @begin 4d-04
        let held = st.queues.get(&resource).and_then(|q| q.holders.mode_of(txn));
        let upgrade = match held {
            Some(h) if h == mode => return Ok(()),
            Some(h) if crate::concurrency::lock_mode::can_upgrade(h, mode) => {
                let q = st.queues.get_mut(&resource).unwrap();
                if q.upgrading.is_some_and(|u| u != txn) {
                    return Err(LockError::new(txn, LockErrorKind::UpgradeConflict));
                }
                q.upgrading = Some(txn);
                true
            }
            Some(_) => return Err(LockError::new(txn, LockErrorKind::IncompatibleUpgrade)),
            None => false,
        };
        //~ let upgrade = false;
        // @end
        {
            let q = st.queues.entry(resource).or_default();
            // an upgrade goes in front of everybody who is not upgrading
            if upgrade {
                q.waiting.insert(0, Waiter { txn, mode, upgrade });
            } else {
                q.waiting.push(Waiter { txn, mode, upgrade });
            }
        }
        loop {
            // @begin 4d-06
            if st.victims.remove(&txn) {
                let q = st.queues.get_mut(&resource).unwrap();
                q.waiting.retain(|w| w.txn != txn);
                if q.upgrading == Some(txn) {
                    q.upgrading = None;
                }
                self.changed.notify_all();
                return Err(LockError::new(txn, LockErrorKind::Deadlock));
            }
            // @end
            let q = st.queues.get_mut(&resource).unwrap();
            let me = q.waiting.iter().position(|w| w.txn == txn).expect("a waiting request stays queued until it is granted");
            let others_ok = q.holders.holders().iter().all(|&(t, m)| t == txn || compatible(m, mode));
            let earlier_ok = q.waiting[..me].iter().all(|w| compatible(w.mode, mode));
            if others_ok && earlier_ok {
                q.waiting.remove(me);
                q.holders.try_acquire(txn, mode).expect("the request is compatible with every other holder");
                if upgrade {
                    q.upgrading = None;
                }
                // @begin 4d-05
                let h = st.held.entry(txn).or_default();
                match resource {
                    Resource::Table(t) => {
                        h.tables.insert(t, mode);
                    }
                    Resource::Row(t, r) => {
                        h.rows.insert((t, r), mode);
                    }
                }
                // @end
                return Ok(());
            }
            st = self.changed.wait(st).unwrap();
        }
        //~ todo!("4d-03: queue the request, wait on the condition variable until it can be granted, then take it")
        // @end
    }

    /// Takes the lock if that is possible right now, without waiting and without jumping the queue. `Ok(false)` when it is not possible.
    pub fn try_lock(&self, txn: TxnId, resource: Resource, mode: LockMode) -> Result<bool, LockError> {
        // @begin 4d-03
        let mut st = self.state.lock().unwrap();
        let q = st.queues.entry(resource).or_default();
        let others_ok = q.holders.holders().iter().all(|&(t, m)| t == txn || compatible(m, mode));
        if !others_ok || q.waiting.iter().any(|w| !compatible(w.mode, mode)) {
            if q.holders.is_empty() && q.waiting.is_empty() {
                st.queues.remove(&resource);
            }
            return Ok(false);
        }
        match q.holders.try_acquire(txn, mode) {
            Ok(true) => {}
            Ok(false) => return Ok(false),
            Err(kind) => return Err(LockError::new(txn, kind)),
        }
        // @begin 4d-05
        let h = st.held.entry(txn).or_default();
        match resource {
            Resource::Table(t) => {
                h.tables.insert(t, mode);
            }
            Resource::Row(t, r) => {
                h.rows.insert((t, r), mode);
            }
        }
        // @end
        Ok(true)
        //~ todo!("4d-03: grant now or say no, never wait")
        // @end
    }

    /// Lets go of the lock `txn` holds on `resource` and wakes the waiters.
    pub fn unlock(&self, txn: TxnId, resource: Resource) -> Result<(), LockError> {
        // @begin 4d-03
        let mut st = self.state.lock().unwrap();
        let released = st.queues.get_mut(&resource).is_some_and(|q| q.holders.release(txn));
        if !released {
            return Err(LockError::new(txn, LockErrorKind::AttemptedUnlockButNoLockHeld));
        }
        if st.queues.get(&resource).is_some_and(|q| q.holders.is_empty() && q.waiting.is_empty()) {
            st.queues.remove(&resource);
        }
        // @begin 4d-05
        if let Some(h) = st.held.get_mut(&txn) {
            match resource {
                Resource::Table(t) => {
                    h.tables.remove(&t);
                }
                Resource::Row(t, r) => {
                    h.rows.remove(&(t, r));
                }
            }
        }
        // @end
        self.changed.notify_all();
        Ok(())
        //~ todo!("4d-03: release and wake the transactions that are waiting")
        // @end
    }

    /// Who holds `resource` and in which mode, ordered by transaction id.
    pub fn holders(&self, resource: Resource) -> Vec<(TxnId, LockMode)> {
        // @begin 4d-03
        self.state.lock().unwrap().queues.get(&resource).map(|q| q.holders.holders()).unwrap_or_default()
        //~ todo!("4d-03: the holders of the resource")
        // @end
    }

    /// How many requests are waiting on `resource`.
    pub fn waiting(&self, resource: Resource) -> usize {
        // @begin 4d-03
        self.state.lock().unwrap().queues.get(&resource).map_or(0, |q| q.waiting.len())
        //~ todo!("4d-03: the number of requests waiting")
        // @end
    }

    // ---- two-phase locking and isolation levels (4d-05) ----------------------------------------------------------------------------------

    fn abort(txn: &LockTxn, kind: LockErrorKind) -> LockError {
        txn.set_state(LockState::Aborted);
        LockError::new(txn.id(), kind)
    }

    /// May `txn` ask for `mode` now? The isolation level and the phase of two-phase locking decide.
    fn allowed(txn: &LockTxn, mode: LockMode) -> Result<(), LockError> {
        use LockMode::*;
        match txn.state() {
            LockState::Committed | LockState::Aborted => return Err(LockError::new(txn.id(), LockErrorKind::NotRunning)),
            _ => {}
        }
        // @begin 4d-05
        if txn.isolation() == LockIsolation::ReadUncommitted && matches!(mode, Shared | IntentionShared | SharedIntentionExclusive) {
            return Err(Self::abort(txn, LockErrorKind::LockSharedOnReadUncommitted));
        }
        if txn.state() == LockState::Shrinking {
            let fine = txn.isolation() == LockIsolation::ReadCommitted && matches!(mode, IntentionShared | Shared);
            if !fine {
                return Err(Self::abort(txn, LockErrorKind::LockOnShrinking));
            }
        }
        Ok(())
        //~ todo!("4d-05: the rules of each isolation level while growing and while shrinking")
        // @end
    }

    /// Takes a table lock for `txn` under the rules of its isolation level, waiting if it is not available.
    ///
    /// # Errors
    ///
    /// Aborts `txn` and returns the reason when a rule is broken (a lock during the shrinking phase, a mode the isolation level forbids, an
    /// invalid upgrade) or when `txn` is chosen as a deadlock victim.
    pub fn lock_table(&self, txn: &LockTxn, mode: LockMode, table: TableOid) -> Result<(), LockError> {
        // @begin 4d-05
        Self::allowed(txn, mode)?;
        self.lock(txn.id(), Resource::Table(table), mode).map_err(|e| match e.kind {
            LockErrorKind::UpgradeConflict | LockErrorKind::IncompatibleUpgrade | LockErrorKind::Deadlock => Self::abort(txn, e.kind),
            _ => e,
        })
        //~ todo!("4d-05: check the rules, then take the table lock")
        // @end
    }

    /// Takes a shared or exclusive lock on a row, waiting if it is not available. The table must already be locked in a mode that covers it.
    ///
    /// # Errors
    ///
    /// Aborts `txn` and returns the reason for an intention lock on a row, a missing or too weak table lock, or a broken isolation rule.
    pub fn lock_row(&self, txn: &LockTxn, mode: LockMode, table: TableOid, rid: Rid) -> Result<(), LockError> {
        use LockMode::*;
        // @begin 4d-05
        if !matches!(mode, Shared | Exclusive) {
            return Err(Self::abort(txn, LockErrorKind::AttemptedIntentionLockOnRow));
        }
        Self::allowed(txn, mode)?;
        let on_table = self.state.lock().unwrap().held.get(&txn.id()).and_then(|h| h.tables.get(&table).copied());
        let enough = match (mode, on_table) {
            (_, None) => false,
            (Exclusive, Some(t)) => matches!(t, IntentionExclusive | SharedIntentionExclusive | Exclusive),
            (_, Some(_)) => true,
        };
        if !enough {
            return Err(Self::abort(txn, LockErrorKind::TableLockNotPresent));
        }
        self.lock(txn.id(), Resource::Row(table, rid), mode).map_err(|e| match e.kind {
            LockErrorKind::UpgradeConflict | LockErrorKind::IncompatibleUpgrade | LockErrorKind::Deadlock => Self::abort(txn, e.kind),
            _ => e,
        })
        //~ todo!("4d-05: only shared or exclusive on a row, and only under a table lock that is enough; then take it")
        // @end
    }

    /// After a lock is released, the transaction may have to start shrinking: any isolation level after an exclusive lock, and repeatable read
    /// after a shared one too.
    fn after_unlock(txn: &LockTxn, mode: LockMode) {
        // @begin 4d-05
        if txn.state() != LockState::Growing {
            return;
        }
        let shrinks = match txn.isolation() {
            LockIsolation::RepeatableRead => matches!(mode, LockMode::Shared | LockMode::Exclusive),
            _ => mode == LockMode::Exclusive,
        };
        if shrinks {
            txn.set_state(LockState::Shrinking);
        }
        //~ todo!("4d-05: move to the shrinking phase when the isolation level says so")
        // @end
    }

    /// Releases `txn`'s lock on the row. Depending on the isolation level, this may start the shrinking phase.
    ///
    /// # Errors
    ///
    /// Aborts `txn` and returns `AttemptedUnlockButNoLockHeld` if it holds no such lock.
    pub fn unlock_row(&self, txn: &LockTxn, table: TableOid, rid: Rid) -> Result<(), LockError> {
        // @begin 4d-05
        let mode = self.state.lock().unwrap().held.get(&txn.id()).and_then(|h| h.rows.get(&(table, rid)).copied());
        let Some(mode) = mode else {
            return Err(Self::abort(txn, LockErrorKind::AttemptedUnlockButNoLockHeld));
        };
        self.unlock(txn.id(), Resource::Row(table, rid))?;
        Self::after_unlock(txn, mode);
        Ok(())
        //~ todo!("4d-05: release the row lock; it may start the shrinking phase")
        // @end
    }

    /// Releases `txn`'s lock on the table. Depending on the isolation level, this may start the shrinking phase.
    ///
    /// # Errors
    ///
    /// Aborts `txn` if it holds no such lock, or still holds row locks on the table.
    pub fn unlock_table(&self, txn: &LockTxn, table: TableOid) -> Result<(), LockError> {
        // @begin 4d-05
        let (mode, rows_left) = {
            let st = self.state.lock().unwrap();
            let h = st.held.get(&txn.id());
            (h.and_then(|h| h.tables.get(&table).copied()), h.is_some_and(|h| h.rows.keys().any(|(t, _)| *t == table)))
        };
        let Some(mode) = mode else {
            return Err(Self::abort(txn, LockErrorKind::AttemptedUnlockButNoLockHeld));
        };
        if rows_left {
            return Err(Self::abort(txn, LockErrorKind::TableUnlockedBeforeUnlockingRows));
        }
        self.unlock(txn.id(), Resource::Table(table))?;
        Self::after_unlock(txn, mode);
        Ok(())
        //~ todo!("4d-05: refuse while rows of the table are locked; otherwise release it; it may start the shrinking phase")
        // @end
    }

    /// Everything the transaction holds, rows first, without touching its phase: what commit and abort do.
    pub fn release_all(&self, txn: &LockTxn) {
        // @begin 4d-05
        let (rows, tables): (Vec<_>, Vec<_>) = {
            let st = self.state.lock().unwrap();
            match st.held.get(&txn.id()) {
                Some(h) => (h.rows.keys().copied().collect(), h.tables.keys().copied().collect()),
                None => (Vec::new(), Vec::new()),
            }
        };
        for (t, r) in rows {
            let _ = self.unlock(txn.id(), Resource::Row(t, r));
        }
        for t in tables {
            let _ = self.unlock(txn.id(), Resource::Table(t));
        }
        //~ todo!("4d-05: let go of every row lock, then every table lock")
        // @end
    }

    // ---- deadlocks (4d-06) -------------------------------------------------------------------------------------------------------------

    /// Looks for deadlocks among the waiting requests and breaks each cycle by choosing its youngest transaction as the victim: its waiting
    /// `lock` call returns `Deadlock`. Returns the victims, in increasing order. A request waits for the holders it is incompatible with.
    pub fn detect_deadlocks(&self) -> Vec<TxnId> {
        // @begin 4d-06
        let mut st = self.state.lock().unwrap();
        let mut graph = WaitsForGraph::new();
        for q in st.queues.values() {
            for w in &q.waiting {
                for (holder, held) in q.holders.holders() {
                    if holder != w.txn && !compatible(w.mode, held) {
                        graph.add_edge(w.txn, holder);
                    }
                }
            }
        }
        let mut victims = Vec::new();
        while let Some(v) = graph.has_cycle() {
            victims.push(v);
            for (a, b) in graph.edge_list() {
                if a == v || b == v {
                    graph.remove_edge(a, b);
                }
            }
        }
        victims.sort_unstable();
        for &v in &victims {
            st.victims.insert(v);
        }
        if !victims.is_empty() {
            self.changed.notify_all();
        }
        victims
        //~ todo!("4d-06: build the waits-for graph from the waiting requests, break each cycle at its youngest transaction, wake the victims")
        // @end
    }
}

impl Default for LockManager {
    fn default() -> Self {
        LockManager::new()
    }
}
