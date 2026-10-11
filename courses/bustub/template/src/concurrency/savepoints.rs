//! Savepoints over a map, by undo records.

use std::collections::BTreeMap;

#[derive(Debug, PartialEq, Eq)]
pub struct NoSuchSavepoint;

pub struct SavepointTxn {
    _txn: (),
}

impl SavepointTxn {
    /// A transaction over `initial` with nothing to undo.
    pub fn begin(initial: BTreeMap<i64, i64>) -> SavepointTxn {
        todo!("4b-c5: a transaction over the initial data with nothing to undo")
    }

    /// Writes `value` at `key`. The write can be undone.
    pub fn set(&mut self, key: i64, value: i64) {
        todo!("4b-c5: remember what the key was, then write")
    }

    /// Removes `key`. The removal can be undone.
    pub fn delete(&mut self, key: i64) {
        todo!("4b-c5: remember what the key was, then remove")
    }

    /// The current value at `key`.
    pub fn get(&self, key: i64) -> Option<i64> {
        todo!("4b-c5: the current value")
    }

    /// Marks the current state and returns the savepoint's id.
    pub fn savepoint(&mut self) -> u64 {
        todo!("4b-c5: mark the current end of the undo log")
    }

    /// Undoes every change made after savepoint `id`, newest first. The savepoint stays; later ones are dropped.
    ///
    /// # Errors
    ///
    /// [`NoSuchSavepoint`] if `id` was never made or is already gone.
    pub fn rollback_to(&mut self, id: u64) -> Result<(), NoSuchSavepoint> {
        todo!("4b-c5: undo back to the mark, newest first; drop the later savepoints")
    }

    /// Forgets savepoint `id` and every later one, keeping their changes.
    ///
    /// # Errors
    ///
    /// [`NoSuchSavepoint`] if `id` was never made or is already gone.
    pub fn release(&mut self, id: u64) -> Result<(), NoSuchSavepoint> {
        todo!("4b-c5: forget the savepoint and every later one, keeping their changes")
    }

    /// Undoes every change, newest first.
    pub fn rollback_all(&mut self) {
        todo!("4b-c5: undo everything")
    }

    /// Ends the transaction and returns the final data.
    pub fn commit(self) -> BTreeMap<i64, i64> {
        todo!("4b-c5: the final map")
    }
}
