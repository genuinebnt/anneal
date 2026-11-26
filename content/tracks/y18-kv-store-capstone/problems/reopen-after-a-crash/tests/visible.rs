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
fn unflushed_writes_survive() {
    let dir = TempDir::new("reopen-log");
    let mut s = Store::open(dir.path(), NEVER).unwrap();
    s.put(b"a", b"1").unwrap();
    s.put(b"b", b"2").unwrap();
    s.delete(b"a").unwrap();
    drop(s);
    let s = Store::open(dir.path(), NEVER).unwrap();
    check!("put a, put b, delete a, reopen", (got(&s, "a"), got(&s, "b")), (GONE, value("2")));
}

#[test]
fn tables_are_found_again() {
    let dir = TempDir::new("reopen-tables");
    let mut s = Store::open(dir.path(), NEVER).unwrap();
    s.put(b"k", b"old").unwrap();
    s.flush().unwrap();
    s.put(b"k", b"new").unwrap();
    s.flush().unwrap();
    drop(s);
    let mut s = Store::open(dir.path(), NEVER).unwrap();
    check!("two tables", (s.table_count(), got(&s, "k")), (2, value("new")));
    s.put(b"j", b"1").unwrap();
    s.flush().unwrap();
    check!("the next table gets a new id", names(dir.path()), ["sst_000001.sst", "sst_000002.sst", "sst_000003.sst", "wal.log"].map(String::from).to_vec());
}

#[test]
fn a_torn_last_write_is_dropped() {
    let dir = TempDir::new("reopen-torn");
    let mut s = Store::open(dir.path(), NEVER).unwrap();
    s.put(b"kept", b"1").unwrap();
    s.put(b"torn", b"2").unwrap();
    drop(s);
    let log = std::fs::read(dir.join("wal.log")).unwrap();
    std::fs::write(dir.join("wal.log"), &log[..log.len() - 1]).unwrap();
    let s = Store::open(dir.path(), NEVER).unwrap();
    check!("the last frame lost one byte", (got(&s, "kept"), got(&s, "torn")), (value("1"), GONE));
}

#[test]
fn an_unfinished_table_is_removed() {
    let dir = TempDir::new("reopen-tmp");
    std::fs::write(dir.join("sst_000001.tmp"), b"half a table").unwrap();
    let s = Store::open(dir.path(), NEVER).unwrap();
    check!("a leftover temporary file", (s.table_count(), names(dir.path())), (0, vec!["wal.log".to_string()]));
}

#[test]
fn a_damaged_table_is_an_error() {
    let dir = TempDir::new("reopen-bad");
    std::fs::write(dir.join("sst_000001.sst"), b"not a table at all, but long enough to have a footer").unwrap();
    check!("open", matches!(Store::open(dir.path(), NEVER), Err(KvError::Corrupt(_))), true);
}
