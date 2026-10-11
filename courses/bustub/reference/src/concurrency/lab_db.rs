//! A tiny database that runs at one of five isolation levels, one step at a time.
//!
//! There are no threads: a scenario plays several transactions by calling them in the order it likes. A step that would have to wait
//! returns `Blocked` instead and changes nothing.

use std::collections::{BTreeMap, BTreeSet};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Level {
    ReadUncommitted,
    ReadCommitted,
    /// Shared row locks on everything read, held to the end (strict two-phase locking on rows).
    RepeatableRead,
    /// Reads as of the start; first committer wins.
    Snapshot,
    /// Repeatable read plus a lock on the whole range a scan covered.
    Serializable,
}

impl Level {
    pub const ALL: [Level; 5] = [Level::ReadUncommitted, Level::ReadCommitted, Level::RepeatableRead, Level::Snapshot, Level::Serializable];
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LabError {
    /// The step would have to wait for another transaction. Nothing changed; the transaction is still open.
    Blocked,
    /// The transaction is finished (aborted by the database, or already ended).
    Aborted,
}

/// A transaction handle.
pub type Tx = usize;

#[derive(Default)]
struct Txn {
    read_ts: u64,
    writes: BTreeMap<i64, i64>,
    read_locks: BTreeSet<i64>,
    scanned: bool,
    open: bool,
}

pub struct LabDb {
    level: Level,
    clock: u64,
    versions: BTreeMap<i64, Vec<(u64, i64)>>,
    txns: Vec<Txn>,
}

impl LabDb {
    /// A database at `level` holding `rows`, all committed before any transaction begins.
    pub fn new(level: Level, rows: &[(i64, i64)]) -> LabDb {
        let versions = rows.iter().map(|&(k, v)| (k, vec![(0, v)])).collect();
        LabDb { level, clock: 0, versions, txns: Vec::new() }
    }

    pub fn begin(&mut self) -> Tx {
        self.txns.push(Txn { read_ts: self.clock, open: true, ..Txn::default() });
        self.txns.len() - 1
    }

    fn check(&self, tx: Tx) -> Result<(), LabError> {
        match self.txns.get(tx) {
            Some(t) if t.open => Ok(()),
            _ => Err(LabError::Aborted),
        }
    }

    fn latest(&self, key: i64) -> Option<i64> {
        self.versions.get(&key).and_then(|v| v.last()).map(|x| x.1)
    }

    fn latest_ts(&self, key: i64) -> u64 {
        self.versions.get(&key).and_then(|v| v.last()).map_or(0, |x| x.0)
    }

    fn as_of(&self, key: i64, ts: u64) -> Option<i64> {
        self.versions.get(&key).and_then(|v| v.iter().rev().find(|(t, _)| *t <= ts)).map(|x| x.1)
    }

    fn others(&self, tx: Tx) -> impl Iterator<Item = &Txn> {
        self.txns.iter().enumerate().filter(move |(i, t)| *i != tx && t.open).map(|(_, t)| t)
    }

    fn locks_reads(&self) -> bool {
        matches!(self.level, Level::RepeatableRead | Level::Serializable)
    }

    /// The value of `key` as `tx` sees it.
    pub fn read(&mut self, tx: Tx, key: i64) -> Result<Option<i64>, LabError> {
        self.check(tx)?;
        if let Some(&v) = self.txns[tx].writes.get(&key) {
            return Ok(Some(v));
        }
        match self.level {
            Level::ReadUncommitted => {
                if let Some(v) = self.others(tx).find_map(|t| t.writes.get(&key).copied()) {
                    return Ok(Some(v));
                }
                Ok(self.latest(key))
            }
            Level::ReadCommitted => Ok(self.latest(key)),
            Level::RepeatableRead | Level::Serializable => {
                if self.others(tx).any(|t| t.writes.contains_key(&key)) {
                    return Err(LabError::Blocked);
                }
                self.txns[tx].read_locks.insert(key);
                Ok(self.latest(key))
            }
            Level::Snapshot => Ok(self.as_of(key, self.txns[tx].read_ts)),
        }
    }

    /// Sets `key` (an insert when it does not exist yet).
    pub fn write(&mut self, tx: Tx, key: i64, value: i64) -> Result<(), LabError> {
        self.check(tx)?;
        if self.others(tx).any(|t| t.writes.contains_key(&key)) {
            return Err(LabError::Blocked);
        }
        if self.locks_reads() && self.others(tx).any(|t| t.read_locks.contains(&key)) {
            return Err(LabError::Blocked);
        }
        if self.level == Level::Serializable && self.latest(key).is_none() && self.others(tx).any(|t| t.scanned) {
            return Err(LabError::Blocked);
        }
        if self.level == Level::Snapshot && self.latest_ts(key) > self.txns[tx].read_ts {
            self.finish(tx);
            return Err(LabError::Aborted);
        }
        self.txns[tx].writes.insert(key, value);
        Ok(())
    }

    /// All rows with `lo <= key <= hi`, in key order, as `tx` sees them.
    pub fn scan(&mut self, tx: Tx, lo: i64, hi: i64) -> Result<Vec<(i64, i64)>, LabError> {
        self.check(tx)?;
        let mut rows: BTreeMap<i64, i64> = BTreeMap::new();
        for &k in self.versions.keys().filter(|k| (lo..=hi).contains(*k)) {
            let v = if self.level == Level::Snapshot { self.as_of(k, self.txns[tx].read_ts) } else { self.latest(k) };
            if let Some(v) = v {
                rows.insert(k, v);
            }
        }
        match self.level {
            Level::ReadUncommitted => {
                for t in self.others(tx) {
                    for (&k, &v) in t.writes.range(lo..=hi) {
                        rows.insert(k, v);
                    }
                }
            }
            Level::RepeatableRead | Level::Serializable => {
                if self.others(tx).any(|t| t.writes.range(lo..=hi).next().is_some()) {
                    return Err(LabError::Blocked);
                }
                let keys: Vec<i64> = rows.keys().copied().collect();
                self.txns[tx].read_locks.extend(keys);
                if self.level == Level::Serializable {
                    self.txns[tx].scanned = true;
                }
            }
            _ => {}
        }
        for (&k, &v) in self.txns[tx].writes.range(lo..=hi) {
            rows.insert(k, v);
        }
        Ok(rows.into_iter().collect())
    }

    fn finish(&mut self, tx: Tx) {
        let t = &mut self.txns[tx];
        t.open = false;
        t.writes.clear();
        t.read_locks.clear();
        t.scanned = false;
    }

    /// Makes the writes visible and ends the transaction. At snapshot a row committed over since the start aborts it instead.
    pub fn commit(&mut self, tx: Tx) -> Result<(), LabError> {
        self.check(tx)?;
        if self.level == Level::Snapshot {
            let read_ts = self.txns[tx].read_ts;
            if self.txns[tx].writes.keys().any(|&k| self.latest_ts(k) > read_ts) {
                self.finish(tx);
                return Err(LabError::Aborted);
            }
        }
        self.clock += 1;
        let writes = std::mem::take(&mut self.txns[tx].writes);
        for (k, v) in writes {
            self.versions.entry(k).or_default().push((self.clock, v));
        }
        self.finish(tx);
        Ok(())
    }

    /// Ends the transaction and throws its writes away. Does nothing for a transaction that is already finished.
    pub fn abort(&mut self, tx: Tx) {
        if self.txns.get(tx).is_some_and(|t| t.open) {
            self.finish(tx);
        }
    }

    /// The newest committed value of `key`, for checking the outcome.
    pub fn committed(&self, key: i64) -> Option<i64> {
        self.latest(key)
    }

    /// How many transactions are still open.
    pub fn open_count(&self) -> usize {
        self.txns.iter().filter(|t| t.open).count()
    }
}
