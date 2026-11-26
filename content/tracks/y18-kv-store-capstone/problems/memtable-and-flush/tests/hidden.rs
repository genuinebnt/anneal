use solution::*;

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

#[allow(dead_code)]
struct TempDir(std::path::PathBuf);

#[allow(dead_code)]
impl TempDir {
    fn new(tag: &str) -> TempDir {
        static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
        let n = NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        let path = std::env::temp_dir().join(format!("anneal-{tag}-{}-{n}", std::process::id()));
        let _ = std::fs::remove_dir_all(&path);
        std::fs::create_dir_all(&path).unwrap();
        TempDir(path)
    }
    fn path(&self) -> &std::path::Path {
        &self.0
    }
    fn join(&self, name: &str) -> std::path::PathBuf {
        self.0.join(name)
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

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
