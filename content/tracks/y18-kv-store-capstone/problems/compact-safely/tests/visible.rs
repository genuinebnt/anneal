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

/// Three tables in which later ones override and delete what earlier ones hold. With `leave_something` false,
/// everything ends up deleted.
#[allow(dead_code)]
fn layered(dir: &std::path::Path, leave_something: bool) -> (Store, Vec<(String, String)>) {
    let mut s = Store::open(dir, NEVER).unwrap();
    s.put(b"a", b"a1").unwrap();
    s.put(b"b", b"b1").unwrap();
    s.put(b"c", b"c1").unwrap();
    s.flush().unwrap();
    s.delete(b"a").unwrap();
    s.put(b"b", b"b2").unwrap();
    s.flush().unwrap();
    s.delete(b"c").unwrap();
    if !leave_something {
        s.delete(b"b").unwrap();
    }
    s.flush().unwrap();
    let want = if leave_something { pairs(&[("b", "b2")]) } else { vec![] };
    (s, want)
}

#[allow(dead_code)]
fn tables_in(dir: &std::path::Path) -> Vec<String> {
    names(dir).into_iter().filter(|n| n.ends_with(".sst")).collect()
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
fn many_tables_become_one() {
    let dir = TempDir::new("compact-one");
    let (mut s, want) = layered(dir.path(), true);
    s.compact().unwrap();
    check!("one table, same answers", (s.table_count(), tables_in(dir.path()).len(), scanned(&s, "a", "z")), (1, 1, want));
}

#[test]
fn nothing_dead_remains() {
    let dir = TempDir::new("compact-clean");
    let (mut s, _) = layered(dir.path(), true);
    s.compact().unwrap();
    let name = tables_in(dir.path()).remove(0);
    let kept = SsTable::open(&dir.join(&name), 4).unwrap().read_all().unwrap();
    check!(name.clone(), (name == "sst_000004.sst", kept), (true, vec![(b"b".to_vec(), Some(b"b2".to_vec()))]));
}

#[test]
fn all_deleted_means_no_table() {
    let dir = TempDir::new("compact-none");
    let (mut s, _) = layered(dir.path(), false);
    s.compact().unwrap();
    check!("no tables, no keys", (s.table_count(), tables_in(dir.path()), scanned(&s, "a", "z")), (0, vec![], vec![]));
}

#[test]
fn a_failed_removal_changes_no_answer() {
    let dir = TempDir::new("compact-fail");
    let (mut s, want) = layered(dir.path(), true);
    let result = s.compact_with(|_| Err(std::io::Error::other("cannot remove")));
    check!("the error comes back", matches!(result, Err(KvError::Io(_))), true);
    check!("the same store still answers", scanned(&s, "a", "z"), want);
}

#[test]
fn fewer_than_two_tables_is_left_alone() {
    let dir = TempDir::new("compact-few");
    let mut s = Store::open(dir.path(), NEVER).unwrap();
    s.compact().unwrap();
    s.put(b"a", b"1").unwrap();
    s.flush().unwrap();
    s.compact().unwrap();
    check!("one table keeps its name", (s.table_count(), tables_in(dir.path())), (1, vec!["sst_000001.sst".to_string()]));
}
