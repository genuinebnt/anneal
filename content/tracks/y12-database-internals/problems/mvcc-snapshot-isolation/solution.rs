use std::collections::{BTreeMap, HashMap};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct TxId(pub u64);

#[derive(Debug, PartialEq, Eq)]
pub enum TxError {
    NotActive,
    Conflict,
}

struct Tx {
    /// The newest commit this transaction can see.
    start: u64,
    /// Pending writes; `None` is a delete.
    writes: BTreeMap<String, Option<i64>>,
}

#[derive(Default)]
pub struct Db {
    /// Per key, `(commit timestamp, value)` in commit order.
    rows: BTreeMap<String, Vec<(u64, Option<i64>)>>,
    active: HashMap<TxId, Tx>,
    last_commit: u64,
    next_tx: u64,
}

impl Db {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn begin(&mut self) -> TxId {
        self.next_tx += 1;
        let id = TxId(self.next_tx);
        self.active.insert(
            id,
            Tx {
                start: self.last_commit,
                writes: BTreeMap::new(),
            },
        );
        id
    }

    pub fn get(&self, tx: TxId, key: &str) -> Result<Option<i64>, TxError> {
        let t = self.active.get(&tx).ok_or(TxError::NotActive)?;
        if let Some(own) = t.writes.get(key) {
            return Ok(*own);
        }
        let visible = self
            .rows
            .get(key)
            .and_then(|v| v.iter().rev().find(|(at, _)| *at <= t.start));
        Ok(visible.and_then(|(_, value)| *value))
    }

    fn write(&mut self, tx: TxId, key: &str, value: Option<i64>) -> Result<(), TxError> {
        let t = self.active.get_mut(&tx).ok_or(TxError::NotActive)?;
        t.writes.insert(key.to_string(), value);
        Ok(())
    }

    pub fn put(&mut self, tx: TxId, key: &str, value: i64) -> Result<(), TxError> {
        self.write(tx, key, Some(value))
    }

    pub fn delete(&mut self, tx: TxId, key: &str) -> Result<(), TxError> {
        self.write(tx, key, None)
    }

    pub fn commit(&mut self, tx: TxId) -> Result<(), TxError> {
        let t = self.active.remove(&tx).ok_or(TxError::NotActive)?;
        let lost = t.writes.keys().any(|key| {
            self.rows
                .get(key)
                .and_then(|v| v.last())
                .is_some_and(|(at, _)| *at > t.start)
        });
        if lost {
            return Err(TxError::Conflict);
        }
        if !t.writes.is_empty() {
            self.last_commit += 1;
            for (key, value) in t.writes {
                self.rows
                    .entry(key)
                    .or_default()
                    .push((self.last_commit, value));
            }
        }
        Ok(())
    }

    pub fn abort(&mut self, tx: TxId) -> Result<(), TxError> {
        self.active.remove(&tx).map(drop).ok_or(TxError::NotActive)
    }

    pub fn versions(&self) -> usize {
        self.rows.values().map(Vec::len).sum()
    }

    pub fn vacuum(&mut self) -> usize {
        let horizon = self
            .active
            .values()
            .map(|t| t.start)
            .min()
            .unwrap_or(self.last_commit);
        let before = self.versions();
        self.rows.retain(|_, versions| {
            // Versions at or before the horizon: all but the last are hidden behind it.
            let old = versions.partition_point(|(at, _)| *at <= horizon);
            if old > 0 {
                let keep_last = versions[old - 1].1.is_some();
                versions.drain(..old - usize::from(keep_last));
            }
            !versions.is_empty()
        });
        before - self.versions()
    }
}
