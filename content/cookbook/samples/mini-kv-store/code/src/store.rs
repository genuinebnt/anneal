//! The store: open (= recover), put, delete, get, scan, flush, compact.

use crate::codec::{Entry, Key, KvError, Result, Val};
use crate::merge::merge_sources;
use crate::sstable::{sync_dir, DirSync, SsTable};
use crate::wal::Wal;
use std::collections::BTreeMap;
use std::fs;
use std::ops::Bound;
use std::path::{Path, PathBuf};

#[derive(Default, Debug, Clone, Copy)]
pub struct Stats {
    pub bloom_skips: u64,
    pub table_reads: u64,
    pub failed_auto_flushes: u64,
}

pub struct Store {
    dir: PathBuf,
    memtable: BTreeMap<Key, Val>,
    mem_bytes: usize,
    pub(crate) wal: Wal,
    tables: Vec<SsTable>, // oldest first
    next_id: u64,
    flush_threshold: usize,
    stats: Stats,
    pub(crate) dir_sync: DirSync,
}

impl Store {
    /// Open == recover: delete leftover temp files, load tables, replay the WAL into a fresh memtable.
    pub fn open(dir: &Path, flush_threshold: usize) -> Result<Store> {
        fs::create_dir_all(dir)?;
        let mut found = Vec::new();
        for ent in fs::read_dir(dir)? {
            let path = ent?.path();
            let name = path.file_name().and_then(|n| n.to_str()).unwrap_or("").to_string();
            if name.ends_with(".tmp") {
                fs::remove_file(&path)?; // crash during a flush or compaction: the table never became visible
            } else if let Some(id) = name.strip_prefix("sst_").and_then(|n| n.strip_suffix(".sst")) {
                found.push((id.parse::<u64>().map_err(|_| KvError::Corrupt("bad table name"))?, path));
            }
        }
        found.sort();
        let tables = found.iter().map(|(id, p)| SsTable::open(p, *id)).collect::<Result<Vec<_>>>()?;
        let next_id = tables.last().map_or(1, |t| t.id + 1);
        let (wal, entries) = Wal::open(&dir.join("wal.log"))?;
        let mut store = Store {
            dir: dir.to_path_buf(),
            memtable: BTreeMap::new(),
            mem_bytes: 0,
            wal,
            tables,
            next_id,
            flush_threshold,
            stats: Stats::default(),
            dir_sync: sync_dir,
        };
        for (k, v) in entries {
            store.mem_insert(k, v);
        }
        Ok(store)
    }

    fn mem_insert(&mut self, key: Key, val: Val) {
        // counts every write, including overwrites, so it grows with the log rather than with the number of live keys
        self.mem_bytes += key.len() + val.as_ref().map_or(0, Vec::len) + 16;
        self.memtable.insert(key, val);
    }

    /// Log first, memtable second. A read can never see a write that the log does not hold.
    fn apply(&mut self, key: &[u8], val: Option<&[u8]>) -> Result<()> {
        self.wal.append(key, val)?;
        self.mem_insert(key.to_vec(), val.map(<[u8]>::to_vec));
        // the write has happened whatever becomes of the flush: an Err here would tell the caller it had not.
        // The memtable and log keep everything, and the next write tries again.
        if self.mem_bytes >= self.flush_threshold && self.flush().is_err() {
            self.stats.failed_auto_flushes += 1;
        }
        Ok(())
    }

    pub fn put(&mut self, key: &[u8], val: &[u8]) -> Result<()> {
        self.apply(key, Some(val))
    }

    pub fn delete(&mut self, key: &[u8]) -> Result<()> {
        self.apply(key, None)
    }

    pub fn wal_len(&self) -> u64 {
        self.wal.len
    }

    pub fn tables(&self) -> &[SsTable] {
        &self.tables
    }

    pub fn stats(&self) -> Stats {
        self.stats
    }

    pub fn get(&mut self, key: &[u8]) -> Result<Option<Vec<u8>>> {
        if let Some(v) = self.memtable.get(key) {
            return Ok(v.clone());
        }
        for t in self.tables.iter().rev() {
            if !t.bloom.may_contain(key) {
                self.stats.bloom_skips += 1;
                continue;
            }
            self.stats.table_reads += 1;
            if let Some(v) = t.get(key)? {
                return Ok(v); // first hit wins, even a tombstone
            }
        }
        Ok(None)
    }

    /// Keys in [from, to): overlay oldest tables first and the memtable last, so newer versions overwrite older ones.
    pub fn scan(&self, from: &[u8], to: &[u8]) -> Result<Vec<(Key, Vec<u8>)>> {
        if from >= to {
            return Ok(Vec::new()); // BTreeMap::range panics when start > end
        }
        let mut merged: BTreeMap<Key, Val> = BTreeMap::new();
        for t in &self.tables {
            for (k, v) in t.read_all()? {
                if k.as_slice() >= from && k.as_slice() < to {
                    merged.insert(k, v);
                }
            }
        }
        for (k, v) in self.memtable.range::<[u8], _>((Bound::Included(from), Bound::Excluded(to))) {
            merged.insert(k.clone(), v.clone());
        }
        Ok(merged.into_iter().filter_map(|(k, v)| v.map(|v| (k, v))).collect())
    }

    /// Table first, WAL reset second: a crash in between replays records that are already in the table, which is harmless.
    pub fn flush(&mut self) -> Result<()> {
        if self.memtable.is_empty() {
            return Ok(());
        }
        let entries: Vec<Entry> = self.memtable.iter().map(|(k, v)| (k.clone(), v.clone())).collect();
        let table = SsTable::write(&self.dir, self.next_id, &entries, self.dir_sync)?;
        self.tables.push(table);
        self.next_id += 1;
        self.memtable.clear();
        self.mem_bytes = 0;
        self.wal.reset()
    }

    /// Full compaction: merge every table into one.
    pub fn compact(&mut self) -> Result<()> {
        self.compact_with(|path| fs::remove_file(path))
    }

    /// FAULT INJECTION for tests. Runs a compaction that "dies" after deleting `removes` of the old files. It takes the
    /// store by value because after a crash there is no store: reopen the directory.
    #[doc(hidden)]
    pub fn crash_during_compaction(mut self, removes: usize) -> Result<()> {
        let (mut done, mut died) = (0, false);
        let result = self.compact_with(|path| {
            if done == removes {
                died = true;
                return Err(std::io::Error::other("injected crash"));
            }
            done += 1;
            fs::remove_file(path)
        });
        if died {
            Ok(())
        } else {
            result
        }
    }

    /// `compact` with the delete passed in, so that a test can make it fail
    pub(crate) fn compact_with(&mut self, mut remove: impl FnMut(&Path) -> std::io::Result<()>) -> Result<()> {
        if self.tables.len() < 2 {
            return Ok(());
        }
        let sources = self.tables.iter().rev().map(SsTable::read_all).collect::<Result<Vec<_>>>()?;
        let merged = merge_sources(sources, true);
        let new_table = if merged.is_empty() {
            None
        } else {
            Some(SsTable::write(&self.dir, self.next_id, &merged, self.dir_sync)?)
        };
        self.next_id += 1;
        let old = self.tables.len();
        // the new table is the newest, and visible on disk already
        self.tables.extend(new_table);
        // OLDEST FIRST, and it matters: the new table holds no tombstones, so until an old value's file is gone, the
        // newer file with its tombstone must still be there. Deleting oldest first leaves only a newest-suffix behind.
        for _ in 0..old {
            // forget a table only after its file is gone; on an error the list still matches the directory
            remove(&self.tables[0].path)?;
            self.tables.remove(0);
            // make this removal durable before the next one: after a power cut, the order must still hold
            (self.dir_sync)(&self.dir)?;
        }
        Ok(())
    }
}
