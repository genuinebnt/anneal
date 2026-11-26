from y17_parts.common import broken, TEMP_DIR
from y18_parts.code import TYPES, CODEC, LOG_FILE_TRAIT, LOG_FILE_IMPL, WAL, FILTER, TABLE_HEAD, TABLE_IMPL, STORE_HEAD, STORE_OPEN_FRESH, STORE_OPS, impl_store

USES = r'''use std::collections::BTreeMap;
use std::fs::{self, File, OpenOptions};
use std::io::{self, Read, Seek, SeekFrom, Write};
use std::ops::Bound;
use std::path::{Path, PathBuf};
use thiserror::Error;
'''

BELOW = TYPES + CODEC + LOG_FILE_TRAIT + LOG_FILE_IMPL + WAL + FILTER + TABLE_HEAD + TABLE_IMPL

SOLUTION = USES + BELOW + STORE_HEAD + impl_store(STORE_OPEN_FRESH, STORE_OPS)

STARTER = USES + BELOW + r'''
// ---------- step 4: your work starts here ----------

/// The store.
pub struct Store {
    dir: PathBuf,
}

impl Store {
    /// Opens a store in `dir`, which is new or empty.
    pub fn open(dir: &Path, flush_threshold: usize) -> Result<Store> {
        todo!()
    }

    pub fn put(&mut self, key: &[u8], value: &[u8]) -> Result<()> {
        todo!()
    }

    pub fn delete(&mut self, key: &[u8]) -> Result<()> {
        todo!()
    }

    pub fn get(&self, key: &[u8]) -> Result<Option<Vec<u8>>> {
        todo!()
    }

    /// The live entries with `from <= key < to`, in key order.
    pub fn scan(&self, from: &[u8], to: &[u8]) -> Result<Vec<(Key, Vec<u8>)>> {
        todo!()
    }

    pub fn flush(&mut self) -> Result<()> {
        todo!()
    }

    pub fn memtable_bytes(&self) -> usize {
        todo!()
    }

    pub fn table_count(&self) -> usize {
        todo!()
    }
}
'''

HARNESS = r'''
#[allow(dead_code)]
const NEVER: usize = usize::MAX;

#[allow(dead_code)]
fn got(s: &Store, k: &str) -> Option<Option<String>> {
    s.get(k.as_bytes()).ok().map(|v| v.map(|v| String::from_utf8_lossy(&v).into_owned()))
}

#[allow(dead_code)]
fn value(v: &str) -> Option<Option<String>> {
    Some(Some(v.to_string()))
}

#[allow(dead_code)]
const GONE: Option<Option<String>> = Some(None);

#[allow(dead_code)]
fn scanned(s: &Store, from: &str, to: &str) -> Vec<(String, String)> {
    s.scan(from.as_bytes(), to.as_bytes()).expect("scan").into_iter().map(|(k, v)| (String::from_utf8_lossy(&k).into_owned(), String::from_utf8_lossy(&v).into_owned())).collect()
}

#[allow(dead_code)]
fn pairs(list: &[(&str, &str)]) -> Vec<(String, String)> {
    list.iter().map(|(k, v)| (k.to_string(), v.to_string())).collect()
}

#[allow(dead_code)]
fn names(dir: &std::path::Path) -> Vec<String> {
    let mut names: Vec<String> = std::fs::read_dir(dir).unwrap().map(|e| e.unwrap().file_name().into_string().unwrap()).collect();
    names.sort();
    names
}

/// Overwrites every data byte of a table file in place. It stays the same file, so a store that holds it open sees the
/// change, as does one that opens it again.
#[allow(dead_code)]
fn ruin_data(path: &std::path::Path) {
    let mut bytes = std::fs::read(path).unwrap();
    let footer = bytes.len() - 32;
    let data_end = u64::from_le_bytes(bytes[footer..footer + 8].try_into().unwrap()) as usize;
    bytes[..data_end].fill(0xee);
    std::fs::write(path, bytes).unwrap();
}

/// What the log file holds now.
#[allow(dead_code)]
fn logged(dir: &std::path::Path) -> Vec<Entry> {
    replay(&std::fs::read(dir.join("wal.log")).unwrap()).expect("replay").0
}
'''

VISIBLE = HARNESS + TEMP_DIR + r'''
#[test]
fn put_get_delete_scan() {
    let dir = TempDir::new("store-basic");
    let mut s = Store::open(dir.path(), NEVER).unwrap();
    s.put(b"b", b"2").unwrap();
    s.put(b"a", b"1").unwrap();
    s.put(b"c", b"3").unwrap();
    s.delete(b"b").unwrap();
    check!("get a, b and zz", (got(&s, "a"), got(&s, "b"), got(&s, "zz")), (value("1"), GONE, GONE));
    check!("scan from a up to c", scanned(&s, "a", "c"), pairs(&[("a", "1")]));
}

#[test]
fn every_write_is_in_the_log_first() {
    let dir = TempDir::new("store-log");
    let mut s = Store::open(dir.path(), NEVER).unwrap();
    s.put(b"k", b"v").unwrap();
    s.delete(b"k").unwrap();
    check!("the log", logged(dir.path()), vec![(b"k".to_vec(), Some(b"v".to_vec())), (b"k".to_vec(), None)]);
}

#[test]
fn newer_layers_win() {
    let dir = TempDir::new("store-layers");
    let mut s = Store::open(dir.path(), NEVER).unwrap();
    s.put(b"apple", b"red").unwrap();
    s.put(b"banana", b"yellow").unwrap();
    s.flush().unwrap();
    s.put(b"apple", b"green").unwrap();
    s.delete(b"banana").unwrap();
    s.flush().unwrap();
    check!("two tables, the second newer", (s.table_count(), got(&s, "apple"), got(&s, "banana")), (2, value("green"), GONE));
    check!("scan everything", scanned(&s, "a", "z"), pairs(&[("apple", "green")]));
}

#[test]
fn a_flush_writes_a_table_and_empties_the_log() {
    let dir = TempDir::new("store-flush");
    let mut s = Store::open(dir.path(), NEVER).unwrap();
    s.put(b"x", b"1").unwrap();
    s.flush().unwrap();
    check!("files", names(dir.path()), vec!["sst_000001.sst".to_string(), "wal.log".to_string()]);
    check!("log and memory are empty", (logged(dir.path()), s.memtable_bytes()), (vec![], 0));
}

#[test]
fn a_full_memtable_flushes_itself() {
    let dir = TempDir::new("store-auto");
    // each of these writes counts 1 + 3 + 16 = 20 bytes
    let mut s = Store::open(dir.path(), 60).unwrap();
    s.put(b"a", b"111").unwrap();
    s.put(b"b", b"222").unwrap();
    let before = (s.table_count(), s.memtable_bytes());
    s.put(b"c", b"333").unwrap();
    check!("two writes, then a third that reaches 60", (before, s.table_count(), s.memtable_bytes()), ((0, 40), 1, 0));
}
'''

HIDDEN = HARNESS + TEMP_DIR + r'''
#[test]
fn what_counts_towards_the_threshold() {
    let dir = TempDir::new("store-count");
    let mut s = Store::open(dir.path(), NEVER).unwrap();
    s.put(b"same", b"12").unwrap();
    s.put(b"same", b"1234").unwrap();
    s.delete(b"same").unwrap();
    check!("overwrites and deletes count too: 22 + 24 + 20", s.memtable_bytes(), 66);
}

#[test]
fn a_delete_hides_a_value_in_a_table() {
    let dir = TempDir::new("store-tomb");
    let mut s = Store::open(dir.path(), NEVER).unwrap();
    s.put(b"k", b"old").unwrap();
    s.flush().unwrap();
    s.delete(b"k").unwrap();
    check!("deleted in memory", (got(&s, "k"), scanned(&s, "a", "z")), (GONE, vec![]));
    s.flush().unwrap();
    check!("deleted in a newer table", (got(&s, "k"), scanned(&s, "a", "z")), (GONE, vec![]));
    let newest = SsTable::open(&dir.join("sst_000002.sst"), 2).unwrap();
    check!("the second table records the delete", newest.read_all().unwrap(), vec![(b"k".to_vec(), None)]);
}

#[test]
fn an_empty_flush_writes_nothing() {
    let dir = TempDir::new("store-noflush");
    let mut s = Store::open(dir.path(), NEVER).unwrap();
    s.flush().unwrap();
    s.put(b"a", b"1").unwrap();
    s.flush().unwrap();
    s.flush().unwrap();
    check!("one table", (s.table_count(), names(dir.path()).len()), (1, 2));
}

#[test]
fn scan_bounds() {
    let dir = TempDir::new("store-scan");
    let mut s = Store::open(dir.path(), NEVER).unwrap();
    s.put(b"b", b"B").unwrap();
    s.flush().unwrap();
    s.put(b"d", b"D").unwrap();
    check!("from included, to excluded", scanned(&s, "b", "d"), pairs(&[("b", "B")]));
    check!("empty and reversed ranges", (scanned(&s, "b", "b"), scanned(&s, "d", "b"), scanned(&s, "z", "a")), (vec![], vec![], vec![]));
    s.put(b"m", b"M").unwrap();
    check!("reversed with entries in memory", scanned(&s, "z", "a"), vec![]);
}

#[test]
fn a_zero_threshold_flushes_every_write() {
    let dir = TempDir::new("store-zero");
    let mut s = Store::open(dir.path(), 0).unwrap();
    for i in 0..5u8 {
        s.put(&[i], &[i]).unwrap();
    }
    check!("five tables", (s.table_count(), s.get(&[3]).ok()), (5, Some(Some(vec![3]))));
}

#[test]
fn a_failed_automatic_flush_does_not_fail_the_write() {
    let dir = TempDir::new("store-failflush");
    let mut s = Store::open(dir.path(), 30).unwrap();
    // a directory where the temporary file would go
    std::fs::create_dir(dir.join("sst_000001.tmp")).unwrap();
    s.put(b"a", b"1").unwrap();
    check!("the write that triggers the failing flush", s.put(b"b", b"2").map_err(|e| e.to_string()), Ok(()));
    check!("both writes can be read", (got(&s, "a"), got(&s, "b"), s.table_count()), (value("1"), value("2"), 0));
    check!("and are still in the log", logged(dir.path()).len(), 2);
    check!("an explicit flush reports the failure", matches!(s.flush(), Err(KvError::Io(_))), true);
    check!("nothing was lost", (got(&s, "a"), got(&s, "b"), logged(dir.path()).len()), (value("1"), value("2"), 2));
    std::fs::remove_dir(dir.join("sst_000001.tmp")).unwrap();
    s.put(b"c", b"3").unwrap();
    check!("the next write flushes", (s.table_count() >= 1, s.memtable_bytes(), logged(dir.path())), (true, 0, vec![]));
    check!("everything is there", scanned(&s, "a", "z"), pairs(&[("a", "1"), ("b", "2"), ("c", "3")]));
}

#[test]
fn a_damaged_read_is_an_error_not_an_older_value() {
    let dir = TempDir::new("store-readerr");
    let mut s = Store::open(dir.path(), NEVER).unwrap();
    s.put(b"k", b"old").unwrap();
    s.flush().unwrap();
    s.put(b"k", b"new").unwrap();
    s.flush().unwrap();
    ruin_data(&dir.join("sst_000002.sst"));
    check!("the newer table cannot be read", s.get(b"k").is_err(), true);
    check!("nor can a scan pretend", s.scan(b"a", b"z").is_err(), true);
}

#[test]
fn random_operations_match_a_map() {
    let mut rng = anneal_prelude::Rng::new(180401);
    for case in 0..20 {
        let dir = TempDir::new("store-random");
        let threshold = [0, 50, 120, 400, NEVER][case % 5];
        let mut s = Store::open(dir.path(), threshold).unwrap();
        let mut model = std::collections::BTreeMap::new();
        let mut story = format!("case {case}, threshold {threshold}:");
        for step in 0..60 {
            let len = rng.below(3);
            let key = rng.string(len, "abc");
            match rng.below(10) {
                0..=3 => {
                    let v = step.to_string();
                    s.put(key.as_bytes(), v.as_bytes()).unwrap();
                    model.insert(key.clone(), v);
                    story.push_str(&format!(" put {key:?},"));
                }
                4 | 5 => {
                    s.delete(key.as_bytes()).unwrap();
                    model.remove(&key);
                    story.push_str(&format!(" delete {key:?},"));
                }
                6 => {
                    s.flush().unwrap();
                    story.push_str(" flush,");
                }
                7 => {
                    let tlen = rng.below(4);
                    let to = rng.string(tlen, "abcd");
                    let want: Vec<(String, String)> = model.iter().filter(|(k, _)| **k >= key && **k < to).map(|(k, v)| (k.clone(), v.clone())).collect();
                    check!(format!("{story} scan {key:?}..{to:?}"), scanned(&s, &key, &to), want);
                }
                _ => check!(format!("{story} get {key:?}"), got(&s, &key), Some(model.get(&key).cloned())),
            }
        }
    }
}
'''

PROBLEM = dict(
    slug="memtable-and-flush", title="Capstone 4: the store, from memory to tables", level="hard", stage="the-store",
    tags=["LSM tree", "memtable", "delete markers", "flush", "layered reads"],
    statement=r'''
**Capstone, step 4 of 7.** Now the pieces become a store. The starter holds the reference code of steps 1 to 3. `Store::open` gets a new or empty directory in this step; reopening comes next.

**Spoiler warning.** The starter gives away work from these parts of the series. Solve them first if you have not.

- Part 5, *An append that fails half-way*: the solution, in the log
- Part 7, *Save a file that a crash cannot tear*: the publishing, in the table code
- Part 8, *A bundle you read from the back*: the footer reading, in the table code
- Part 10, *One block is enough*: the lookup, in the table code
- Part 12, *Fix: a saved filter that forgets its keys*: the solution, in the table code

Recent writes live in memory, sorted by key (the **memtable**), and in the log `wal.log`. Older writes live in tables. `flush` moves the memtable into a new table.

**Writes.** `put` and `delete` add the change to the log first, and only then to memory. If the log write fails, the error is returned and nothing else changes.

**Reads.** `get` and `scan` answer as if all writes had gone to one map. The newest mention of a key decides, also when it is a delete. A table that cannot be read is an error, never a reason to answer from an older one. `scan(from, to)` covers `from <= key < to`. An empty or reversed range gives nothing.

**`flush()`** does nothing when the memtable is empty. Otherwise it writes the memtable as the next table (ids 1, 2, 3, and so on), and only after that empties the log. If the table cannot be written, it returns the error, nothing is lost, and the id is not used up. The store keeps no files besides the log and the tables.

**Automatic flush.** `memtable_bytes()` adds `key length + value length + 16` for **every** write since the last flush (a delete has value length 0). After a write, if it is at least `flush_threshold`, the store flushes. A failure of **this** flush does not make the write fail, because the write has already happened. The next write tries again.

**`table_count()`** is the number of tables.
''',
    starter=STARTER, solution=SOLUTION, visible=[VISIBLE], hidden=[HIDDEN], use="use solution::*;", crates=["thiserror"],
    hints=[
        ("approach", "Write down what each layer must remember about a deleted key. Then decide, for flush, which step must have succeeded before the next may throw anything away."),
        ("rust", "`BTreeMap<Vec<u8>, Option<Vec<u8>>>` can be ranged with `range::<[u8], _>((Bound::Included(from), Bound::Excluded(to)))`. Read the docs of `range` for when it panics."),
        ("edge", "What should `put` return when the log took the write but the flush it triggered failed? A caller who gets an error may try again or give up."),
    ],
    notes=("A write goes to the log and then the memtable, where a delete is stored as a marker. Reads check the memtable and then tables from newest to oldest, stopping at the first mention. A flush publishes the table before it empties the log, and a failed automatic flush leaves the write in place.", "put O(log n) plus flushes; get O(tables); scan O(all records)", "O(memtable + table indexes)"),
    follow_up="1) `scan` reads every table completely. What would you use to read only the part in range, and to merge without collecting everything? 2) Writes wait while a flush runs. What has to be true of the old memtable if new writes continue during the flush? 3) Why does the threshold count overwrites?",
    teaches=["In layered storage a delete is data: it must travel down with the flush.", "Publish the new copy before discarding the old one.", "Do not report failure for work that has already taken effect."],
    wrong={
        "delete_removes_from_memory": broken(SOLUTION, ("        self.memtable.insert(key, value);\n    }", "        match value {\n            Some(value) => self.memtable.insert(key, Some(value)),\n            None => self.memtable.remove(&key),\n        };\n    }")),
        "reads_the_oldest_table_first": broken(SOLUTION, ("        for table in self.tables.iter().rev() {", "        for table in &self.tables {")),
        "flush_error_fails_the_write": broken(SOLUTION, ("            let _ = self.flush();\n", "            self.flush()?;\n")),
        "empties_the_log_first": broken(SOLUTION, ("        let table = SsTable::write(&self.dir, self.next_id, &entries)?;\n", "        self.wal.reset()?;\n        let table = SsTable::write(&self.dir, self.next_id, &entries)?;\n"), ("        self.memtable_bytes = 0;\n        self.wal.reset()\n", "        self.memtable_bytes = 0;\n        Ok(())\n")),
        "reversed_scan_panics": broken(SOLUTION, ("        if from >= to {\n            return Ok(Vec::new());\n        }\n", "")),
        "read_errors_fall_through": broken(SOLUTION, ("            if let Some(value) = table.get(key)? {", "            if let Some(value) = table.get(key).unwrap_or(None) {")),
        "memtable_is_cleared_before_the_table_is_safe": broken(SOLUTION, ("            .map(|(k, v)| (k.clone(), v.clone()))\n            .collect();\n        let table", "            .map(|(k, v)| (k.clone(), v.clone()))\n            .collect();\n        self.memtable.clear();\n        let table")),
    },
)
