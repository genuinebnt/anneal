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
    // @begin 4d-c7
    let (writer, reader) = (db.begin(), db.begin());
    let mut saw_uncommitted = false;
    if db.write(writer, 1, 999).is_ok() {
        if let Ok(Some(v)) = db.read(reader, 1) {
            saw_uncommitted = v == 999;
        }
    }
    db.abort(writer);
    db.abort(reader);
    saw_uncommitted
    //~ let _ = db;
    //~ todo!("4d-c7: one transaction writes and does not commit; another reads; did it see the write?")
    // @end
}

/// Rows `{1: 100}`. True when one transaction read the same row twice and got two different answers.
pub fn non_repeatable_read(db: &mut LabDb) -> bool {
    // @begin 4d-c7
    let (a, b) = (db.begin(), db.begin());
    let first = db.read(a, 1);
    if db.write(b, 1, 200).is_ok() {
        let _ = db.commit(b);
    }
    let second = db.read(a, 1);
    db.abort(a);
    db.abort(b);
    matches!((first, second), (Ok(x), Ok(y)) if x != y)
    //~ let _ = db;
    //~ todo!("4d-c7: read, let another transaction change the row and commit, read again")
    // @end
}

/// Rows `{1: 0}`. Two transactions each read the row and write it back plus one. True when both commit and the row ended at 1, not 2.
pub fn lost_update(db: &mut LabDb) -> bool {
    // @begin 4d-c7
    let (a, b) = (db.begin(), db.begin());
    let (Ok(x), Ok(y)) = (db.read(a, 1), db.read(b, 1)) else {
        db.abort(a);
        db.abort(b);
        return false;
    };
    let (x, y) = (x.unwrap_or(0), y.unwrap_or(0));
    let a_done = db.write(a, 1, x + 1).is_ok() && db.commit(a).is_ok();
    if !a_done {
        db.abort(a);
    }
    let b_done = db.write(b, 1, y + 1).is_ok() && db.commit(b).is_ok();
    if !b_done {
        db.abort(b);
    }
    a_done && b_done && db.committed(1) == Some(1)
    //~ let _ = db;
    //~ todo!("4d-c7: both read, both write the value plus one, both commit; what is the row now?")
    // @end
}

/// Rows `{1: 1, 2: 1}` (two doctors on call; at least one must stay). Each transaction reads both rows and, seeing two on call, takes one
/// itself off. True when both commit and nobody is on call.
pub fn write_skew(db: &mut LabDb) -> bool {
    // @begin 4d-c7
    fn on_call(db: &mut LabDb, t: usize) -> Option<i64> {
        Some(db.read(t, 1).ok()?.unwrap_or(0) + db.read(t, 2).ok()?.unwrap_or(0))
    }
    let (a, b) = (db.begin(), db.begin());
    let (sa, sb) = (on_call(db, a), on_call(db, b));
    let a_done = sa >= Some(2) && db.write(a, 1, 0).is_ok() && db.commit(a).is_ok();
    if !a_done {
        db.abort(a);
    }
    let b_done = sb >= Some(2) && db.write(b, 2, 0).is_ok() && db.commit(b).is_ok();
    if !b_done {
        db.abort(b);
    }
    a_done && b_done && db.committed(1).unwrap_or(0) + db.committed(2).unwrap_or(0) == 0
    //~ let _ = db;
    //~ todo!("4d-c7: both check the constraint, each writes a different row, both commit")
    // @end
}

/// Rows `{1: 10, 2: 20}`. One transaction scans keys 0 to 100 twice; between the scans another inserts key 50 and commits. True when the
/// two scans differ.
pub fn phantom(db: &mut LabDb) -> bool {
    // @begin 4d-c7
    let (a, b) = (db.begin(), db.begin());
    let first = db.scan(a, 0, 100);
    if db.write(b, 50, 30).is_ok() {
        let _ = db.commit(b);
    }
    let second = db.scan(a, 0, 100);
    db.abort(a);
    db.abort(b);
    matches!((first, second), (Ok(x), Ok(y)) if x != y)
    //~ let _ = db;
    //~ todo!("4d-c7: scan a range, let another transaction insert into it and commit, scan again")
    // @end
}

/// Whether `anomaly` can occur under `level`.
pub fn possible(level: Level, anomaly: Anomaly) -> bool {
    // @begin 4d-c7
    use Anomaly::*;
    use Level::*;
    match (level, anomaly) {
        (ReadUncommitted, _) => true,
        (_, DirtyRead) => false,
        (ReadCommitted, _) => true,
        (_, NonRepeatableRead | LostUpdate) => false,
        (Snapshot, WriteSkew) => true,
        (RepeatableRead, Phantom) => true,
        _ => false,
    }
    //~ let _ = (level, anomaly);
    //~ todo!("4d-c7: from the mechanism of each level, not from memory of a table")
    // @end
}
