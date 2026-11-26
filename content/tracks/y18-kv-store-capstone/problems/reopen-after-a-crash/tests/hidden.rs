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
fn numbers_not_text_decide_which_table_is_newer() {
    let dir = TempDir::new("reopen-million");
    SsTable::write(dir.path(), 999_999, &[(b"k".to_vec(), Some(b"older".to_vec()))]).unwrap();
    SsTable::write(dir.path(), 1_000_000, &[(b"k".to_vec(), Some(b"newer".to_vec()))]).unwrap();
    let mut s = Store::open(dir.path(), NEVER).unwrap();
    check!("tables 999999 and 1000000", got(&s, "k"), value("newer"));
    s.put(b"k", b"newest").unwrap();
    s.flush().unwrap();
    check!("the next id", dir.join("sst_1000001.sst").exists(), true);
    let s = Store::open(dir.path(), NEVER).unwrap();
    check!("after another reopen", (got(&s, "k"), s.table_count()), (value("newest"), 3));
}

#[test]
fn ids_with_gaps() {
    let dir = TempDir::new("reopen-gaps");
    SsTable::write(dir.path(), 3, &[(b"a".to_vec(), Some(b"3".to_vec()))]).unwrap();
    SsTable::write(dir.path(), 41, &[(b"a".to_vec(), Some(b"41".to_vec()))]).unwrap();
    let mut s = Store::open(dir.path(), NEVER).unwrap();
    s.put(b"b", b"x").unwrap();
    s.flush().unwrap();
    check!("after tables 3 and 41 comes 42", (dir.join("sst_000042.sst").exists(), got(&s, "a")), (true, value("41")));
}

#[test]
fn crash_after_the_table_before_the_log_was_emptied() {
    let dir = TempDir::new("reopen-both");
    let mut s = Store::open(dir.path(), NEVER).unwrap();
    s.put(b"a", b"1").unwrap();
    s.delete(b"b").unwrap();
    s.put(b"c", b"3").unwrap();
    let log = std::fs::read(dir.join("wal.log")).unwrap();
    s.flush().unwrap();
    drop(s);
    // put the log back as it was just before the flush emptied it
    std::fs::write(dir.join("wal.log"), log).unwrap();
    let mut s = Store::open(dir.path(), NEVER).unwrap();
    check!("both hold the writes", scanned(&s, "a", "z"), pairs(&[("a", "1"), ("c", "3")]));
    s.put(b"d", b"4").unwrap();
    s.flush().unwrap();
    let s = Store::open(dir.path(), NEVER).unwrap();
    check!("and later writes are fine", scanned(&s, "a", "z"), pairs(&[("a", "1"), ("c", "3"), ("d", "4")]));
}

#[test]
fn zeros_at_the_end_of_the_log() {
    let dir = TempDir::new("reopen-zeros");
    let mut s = Store::open(dir.path(), NEVER).unwrap();
    s.put(b"a", b"1").unwrap();
    drop(s);
    let mut log = std::fs::read(dir.join("wal.log")).unwrap();
    log.extend([0; 512]);
    std::fs::write(dir.join("wal.log"), log).unwrap();
    let mut s = Store::open(dir.path(), NEVER).expect("a zero-filled tail must not stop the store");
    check!("the record before the zeros", got(&s, "a"), value("1"));
    s.put(b"b", b"2").unwrap();
    let s = Store::open(dir.path(), NEVER).unwrap();
    check!("writes after recovery", (got(&s, "a"), got(&s, "b")), (value("1"), value("2")));
}

#[test]
fn other_peoples_files_are_left_alone() {
    let dir = TempDir::new("reopen-foreign");
    let foreign = ["notes.txt", "notes.tmp", "sst_abc.sst", "sst_.sst", "sst_1.sst", "sst_0000001.sst", "sst_+00001.sst", "sst_000001.sst.bak", "sst_1.tmp", "backup.sst", "wal.log.old"];
    for name in foreign {
        std::fs::write(dir.join(name), b"mine").unwrap();
    }
    std::fs::write(dir.join("sst_000007.tmp"), b"ours").unwrap();
    let s = Store::open(dir.path(), NEVER).expect("foreign files are ignored");
    let mut want: Vec<String> = foreign.iter().map(|s| s.to_string()).collect();
    want.push("wal.log".to_string());
    want.sort();
    check!("what is left", (s.table_count(), names(dir.path())), (0, want));
}

#[test]
fn the_replayed_log_counts_towards_the_threshold() {
    let dir = TempDir::new("reopen-count");
    let mut s = Store::open(dir.path(), NEVER).unwrap();
    s.put(b"a", b"111").unwrap();
    s.put(b"a", b"222").unwrap();
    drop(s);
    let mut s = Store::open(dir.path(), 60).unwrap();
    check!("two replayed writes of 20", s.memtable_bytes(), 40);
    s.put(b"b", b"333").unwrap();
    check!("the third reaches 60", (s.table_count(), s.memtable_bytes()), (1, 0));
}

#[test]
fn a_corrupt_log_is_an_error() {
    let dir = TempDir::new("reopen-badlog");
    let payload = [5u8, 5, 5];
    let mut log = (payload.len() as u32).to_le_bytes().to_vec();
    log.extend(crc32(&payload).to_le_bytes());
    log.extend(payload);
    std::fs::write(dir.join("wal.log"), &log).unwrap();
    check!("open", matches!(Store::open(dir.path(), NEVER), Err(KvError::Corrupt(_))), true);
    check!("the log is kept", std::fs::read(dir.join("wal.log")).unwrap(), log);
}

#[test]
fn a_missing_directory_is_created() {
    let dir = TempDir::new("reopen-mkdir");
    let path = dir.join("a/b/c");
    let mut s = Store::open(&path, NEVER).unwrap();
    s.put(b"k", b"v").unwrap();
    check!("reopened", got(&Store::open(&path, NEVER).unwrap(), "k"), value("v"));
}

#[test]
fn random_writes_crashes_and_torn_records() {
    let mut rng = anneal_prelude::Rng::new(180501);
    let (mut crashes, mut torn) = (0, 0);
    for case in 0..16 {
        let dir = TempDir::new("reopen-random");
        let threshold = [100, 200, 500, NEVER][case % 4];
        let mut s = Store::open(dir.path(), threshold).unwrap();
        let mut model = std::collections::BTreeMap::new();
        // what the newest write replaced, as long as its frame is still at the end of the log
        let mut last: Option<(String, Option<String>)> = None;
        let mut story = format!("case {case}, threshold {threshold}:");
        for step in 0..80 {
            let len = rng.below(3);
            let key = rng.string(len, "abc");
            match rng.below(12) {
                0..=5 => {
                    let before = model.get(&key).cloned();
                    if rng.below(3) == 0 {
                        s.delete(key.as_bytes()).unwrap();
                        model.remove(&key);
                        story.push_str(&format!(" delete {key:?},"));
                    } else {
                        s.put(key.as_bytes(), step.to_string().as_bytes()).unwrap();
                        model.insert(key.clone(), step.to_string());
                        story.push_str(&format!(" put {key:?}={step},"));
                    }
                    last = (!logged(dir.path()).is_empty()).then(|| (key.clone(), before));
                }
                6 => {
                    s.flush().unwrap();
                    last = None;
                    story.push_str(" flush,");
                }
                7 | 8 => {
                    drop(s);
                    if let Some((key, before)) = last.take() {
                        if rng.bool() {
                            let log = std::fs::read(dir.join("wal.log")).unwrap();
                            let cut = 1 + rng.below(8);
                            std::fs::write(dir.join("wal.log"), &log[..log.len() - cut]).unwrap();
                            match before {
                                Some(v) => model.insert(key, v),
                                None => model.remove(&key),
                            };
                            torn += 1;
                            story.push_str(&format!(" crash tearing {cut},"));
                        }
                    }
                    crashes += 1;
                    story.push_str(" reopen,");
                    s = Store::open(dir.path(), threshold).expect("reopen");
                }
                _ => check!(format!("{story} get {key:?}"), got(&s, &key), Some(model.get(&key).cloned())),
            }
        }
        drop(s);
        let s = Store::open(dir.path(), threshold).expect("final reopen");
        let want: Vec<(String, String)> = model.into_iter().collect();
        check!(format!("{story} final scan"), scanned(&s, "", "zzzz"), want);
    }
    check!(format!("the run really crashed ({crashes}) and tore ({torn})"), crashes > 120 && torn > 25, true);
}
