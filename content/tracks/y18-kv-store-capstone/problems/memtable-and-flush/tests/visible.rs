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
