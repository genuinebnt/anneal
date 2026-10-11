//! Lock escalation: many row locks become one table lock.

use std::collections::{BTreeMap, BTreeSet};

#[derive(Debug, PartialEq, Eq)]
pub enum RowLockResult {
    Granted,
    Covered,
    Escalate { table: u32, released: Vec<u64> },
}

pub struct Escalator {
    _esc: (),
}

impl Escalator {
    /// An escalator that has locked nothing. More than `threshold` row locks by one transaction on one table become a table lock.
    pub fn new(threshold: usize) -> Escalator {
        todo!("4d-c5: nothing locked")
    }

    /// Records a row lock by `txn` on `table`.
    ///
    /// Returns [`RowLockResult::Covered`] if the transaction already holds the table lock, [`RowLockResult::Granted`] for a row lock (asking
    /// again for a row it holds is granted again), and [`RowLockResult::Escalate`] when this lock would make more than `threshold` row
    /// locks on the table: those row locks are released and replaced by one table lock.
    pub fn row_lock(&mut self, txn: u32, table: u32, row: u64) -> RowLockResult {
        todo!("4d-c5: count the transaction's row locks on the table; past the threshold replace them by a table lock")
    }

    /// Whether `txn` holds the table lock on `table`.
    pub fn holds_table(&self, txn: u32, table: u32) -> bool {
        todo!("4d-c5: does the transaction hold the table lock")
    }

    /// How many row locks `txn` holds on `table`.
    pub fn row_count(&self, txn: u32, table: u32) -> usize {
        todo!("4d-c5: how many row locks")
    }

    /// Forgets every lock `txn` holds.
    pub fn release_all(&mut self, txn: u32) {
        todo!("4d-c5: forget every lock of the transaction")
    }
}
