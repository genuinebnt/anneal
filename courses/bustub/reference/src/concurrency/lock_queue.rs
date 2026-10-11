//! The locks held on one thing (a table or a row), without any waiting: who holds which mode.

use crate::concurrency::lock_error::LockErrorKind;
use crate::concurrency::lock_mode::{can_upgrade, compatible, LockMode};
use crate::concurrency::transaction::TxnId;

/// The holders of one lock. All holders are always compatible with each other.
pub struct LockQueue {
    // @begin 4d-02
    holders: Vec<(TxnId, LockMode)>,
    //~ _holders: (),
    // @end
}

impl LockQueue {
    /// A queue with no holder.
    pub fn new() -> LockQueue {
        // @begin 4d-02
        LockQueue { holders: Vec::new() }
        //~ todo!("4d-02: nobody holds anything")
        // @end
    }

    /// `txn` asks for `mode`. `Ok(true)`: it holds it now (a new lock, the same lock again, or an upgrade of the one it had). `Ok(false)`: some other
    /// holder is in the way, and nothing changed. `Err(IncompatibleUpgrade)`: it holds a mode that `mode` is not an upgrade of.
    pub fn try_acquire(&mut self, txn: TxnId, mode: LockMode) -> Result<bool, LockErrorKind> {
        // @begin 4d-02
        let held = self.mode_of(txn);
        match held {
            Some(h) if h == mode => return Ok(true),
            Some(h) if !can_upgrade(h, mode) => return Err(LockErrorKind::IncompatibleUpgrade),
            _ => {}
        }
        if self.holders.iter().any(|&(t, m)| t != txn && !compatible(m, mode)) {
            return Ok(false);
        }
        match self.holders.iter_mut().find(|(t, _)| *t == txn) {
            Some(entry) => entry.1 = mode,
            None => self.holders.push((txn, mode)),
        }
        Ok(true)
        //~ todo!("4d-02: granted when no other holder conflicts; the same mode again is fine; another mode only if it is an upgrade")
        // @end
    }

    /// `txn` lets go. False when it held nothing.
    pub fn release(&mut self, txn: TxnId) -> bool {
        // @begin 4d-02
        let before = self.holders.len();
        self.holders.retain(|&(t, _)| t != txn);
        self.holders.len() != before
        //~ todo!("4d-02: forget the holder")
        // @end
    }

    /// Everyone who holds the lock and in which mode, ordered by transaction id.
    pub fn holders(&self) -> Vec<(TxnId, LockMode)> {
        // @begin 4d-02
        let mut v = self.holders.clone();
        v.sort_by_key(|&(t, _)| t);
        v
        //~ todo!("4d-02: the holders, by transaction id")
        // @end
    }

    /// The mode `txn` holds, if any.
    pub fn mode_of(&self, txn: TxnId) -> Option<LockMode> {
        // @begin 4d-02
        self.holders.iter().find(|&&(t, _)| t == txn).map(|&(_, m)| m)
        //~ todo!("4d-02: the mode this transaction holds, if any")
        // @end
    }

    pub fn is_empty(&self) -> bool {
        self.holders().is_empty()
    }
}

impl Default for LockQueue {
    fn default() -> Self {
        LockQueue::new()
    }
}
