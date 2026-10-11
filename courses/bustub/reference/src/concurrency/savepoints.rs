//! Savepoints over a map, by undo records.

use std::collections::BTreeMap;

#[derive(Debug, PartialEq, Eq)]
pub struct NoSuchSavepoint;

pub struct SavepointTxn {
    // @begin 4b-c5
    data: BTreeMap<i64, i64>,
    /// For each change, the previous state of the key (`None`: it was absent).
    undo: Vec<(i64, Option<i64>)>,
    /// (savepoint id, length of `undo` when it was taken), oldest first.
    marks: Vec<(u64, usize)>,
    next_id: u64,
    //~ _txn: (),
    // @end
}

impl SavepointTxn {
    /// A transaction over `initial` with nothing to undo.
    pub fn begin(initial: BTreeMap<i64, i64>) -> SavepointTxn {
        // @begin 4b-c5
        SavepointTxn { data: initial, undo: Vec::new(), marks: Vec::new(), next_id: 1 }
        //~ todo!("4b-c5: a transaction over the initial data with nothing to undo")
        // @end
    }

    /// Writes `value` at `key`. The write can be undone.
    pub fn set(&mut self, key: i64, value: i64) {
        // @begin 4b-c5
        let old = self.data.insert(key, value);
        self.undo.push((key, old));
        //~ todo!("4b-c5: remember what the key was, then write")
        // @end
    }

    /// Removes `key`. The removal can be undone.
    pub fn delete(&mut self, key: i64) {
        // @begin 4b-c5
        let old = self.data.remove(&key);
        self.undo.push((key, old));
        //~ todo!("4b-c5: remember what the key was, then remove")
        // @end
    }

    /// The current value at `key`.
    pub fn get(&self, key: i64) -> Option<i64> {
        // @begin 4b-c5
        self.data.get(&key).copied()
        //~ todo!("4b-c5: the current value")
        // @end
    }

    /// Marks the current state and returns the savepoint's id.
    pub fn savepoint(&mut self) -> u64 {
        // @begin 4b-c5
        let id = self.next_id;
        self.next_id += 1;
        self.marks.push((id, self.undo.len()));
        id
        //~ todo!("4b-c5: mark the current end of the undo log")
        // @end
    }

    /// Undoes every change made after savepoint `id`, newest first. The savepoint stays; later ones are dropped.
    ///
    /// # Errors
    ///
    /// [`NoSuchSavepoint`] if `id` was never made or is already gone.
    pub fn rollback_to(&mut self, id: u64) -> Result<(), NoSuchSavepoint> {
        // @begin 4b-c5
        let at = self.marks.iter().position(|m| m.0 == id).ok_or(NoSuchSavepoint)?;
        let keep = self.marks[at].1;
        while self.undo.len() > keep {
            let (key, old) = self.undo.pop().unwrap();
            match old {
                Some(v) => {
                    self.data.insert(key, v);
                }
                None => {
                    self.data.remove(&key);
                }
            }
        }
        self.marks.truncate(at + 1);
        Ok(())
        //~ todo!("4b-c5: undo back to the mark, newest first; drop the later savepoints")
        // @end
    }

    /// Forgets savepoint `id` and every later one, keeping their changes.
    ///
    /// # Errors
    ///
    /// [`NoSuchSavepoint`] if `id` was never made or is already gone.
    pub fn release(&mut self, id: u64) -> Result<(), NoSuchSavepoint> {
        // @begin 4b-c5
        let at = self.marks.iter().position(|m| m.0 == id).ok_or(NoSuchSavepoint)?;
        self.marks.truncate(at);
        Ok(())
        //~ todo!("4b-c5: forget the savepoint and every later one, keeping their changes")
        // @end
    }

    /// Undoes every change, newest first.
    pub fn rollback_all(&mut self) {
        // @begin 4b-c5
        while let Some((key, old)) = self.undo.pop() {
            match old {
                Some(v) => {
                    self.data.insert(key, v);
                }
                None => {
                    self.data.remove(&key);
                }
            }
        }
        self.marks.clear();
        //~ todo!("4b-c5: undo everything")
        // @end
    }

    /// Ends the transaction and returns the final data.
    pub fn commit(self) -> BTreeMap<i64, i64> {
        // @begin 4b-c5
        self.data
        //~ todo!("4b-c5: the final map")
        // @end
    }
}
