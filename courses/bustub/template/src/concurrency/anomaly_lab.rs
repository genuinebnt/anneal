//! The anomaly lab: five scenarios and a table.

use crate::concurrency::lab_db::{LabDb, Level};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Anomaly {
    DirtyRead,
    NonRepeatableRead,
    LostUpdate,
    WriteSkew,
    Phantom,
}

/// Rows `{1: 100}`. True when a transaction read a value another one wrote and never committed.
pub fn dirty_read(db: &mut LabDb) -> bool {
    let _ = db;
    todo!("4d-c7: one transaction writes and does not commit; another reads; did it see the write?")
}

/// Rows `{1: 100}`. True when one transaction read the same row twice and got two different answers.
pub fn non_repeatable_read(db: &mut LabDb) -> bool {
    let _ = db;
    todo!("4d-c7: read, let another transaction change the row and commit, read again")
}

/// Rows `{1: 0}`. Two transactions each read the row and write it back plus one. True when both commit and the row ended at 1, not 2.
pub fn lost_update(db: &mut LabDb) -> bool {
    let _ = db;
    todo!("4d-c7: both read, both write the value plus one, both commit; what is the row now?")
}

/// Rows `{1: 1, 2: 1}` (two doctors on call; at least one must stay). Each transaction reads both rows and, seeing two on call, takes one
/// itself off. True when both commit and nobody is on call.
pub fn write_skew(db: &mut LabDb) -> bool {
    let _ = db;
    todo!("4d-c7: both check the constraint, each writes a different row, both commit")
}

/// Rows `{1: 10, 2: 20}`. One transaction scans keys 0 to 100 twice; between the scans another inserts key 50 and commits. True when the
/// two scans differ.
pub fn phantom(db: &mut LabDb) -> bool {
    let _ = db;
    todo!("4d-c7: scan a range, let another transaction insert into it and commit, scan again")
}

/// Whether `anomaly` can occur under `level`.
pub fn possible(level: Level, anomaly: Anomaly) -> bool {
    let _ = (level, anomaly);
    todo!("4d-c7: from the mechanism of each level, not from memory of a table")
}
