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
fn a_failure_at_every_removal() {
    for leave in [true, false] {
        for ok in 0..3 {
            let dir = TempDir::new("compact-each");
            let (mut s, want) = layered(dir.path(), leave);
            let (mut done, mut calls) = (0, 0);
            let result = s.compact_with(|path| {
                calls += 1;
                if done == ok {
                    return Err(std::io::Error::other("injected"));
                }
                done += 1;
                std::fs::remove_file(path)
            });
            let story = format!("something live: {leave}; removal {} fails", ok + 1);
            check!(format!("{story}: the error comes back"), matches!(result, Err(KvError::Io(_))), true);
            check!(format!("{story}: nothing is removed after a failure"), calls, ok + 1);
            check!(format!("{story}: open store, scan"), scanned(&s, "a", "z"), want.clone());
            for key in ["a", "b", "c"] {
                let expected = want.iter().find(|p| p.0 == key).map(|p| p.1.clone());
                check!(format!("{story}: open store, get {key}"), got(&s, key), Some(expected));
            }
            check!(format!("{story}: table_count matches the files"), s.table_count(), tables_in(dir.path()).len());
            // as if the process died here
            let reopened = Store::open(dir.path(), NEVER).unwrap();
            check!(format!("{story}: after reopening"), scanned(&reopened, "a", "z"), want.clone());
            // and the store that saw the error carries on
            s.put(b"z0", b"later").unwrap();
            s.flush().unwrap();
            s.compact().unwrap();
            let mut after = want;
            after.push(("z0".to_string(), "later".to_string()));
            check!(format!("{story}: a second compaction"), (scanned(&s, "a", "zz"), s.table_count(), tables_in(dir.path()).len()), (after.clone(), 1, 1));
            check!(format!("{story}: and reopening again"), scanned(&Store::open(dir.path(), NEVER).unwrap(), "a", "zz"), after);
        }
    }
}

#[test]
fn the_new_table_is_complete_before_anything_is_removed() {
    let dir = TempDir::new("compact-first");
    let (mut s, _) = layered(dir.path(), true);
    let new = dir.join("sst_000004.sst");
    let mut seen = Vec::new();
    s.compact_with(|path| {
        seen.push(SsTable::open(&new, 4).is_ok());
        std::fs::remove_file(path)
    })
    .unwrap();
    check!("at each removal, sst_000004 can be opened", seen, vec![true; 3]);
}

#[test]
fn if_the_new_table_cannot_be_written_nothing_is_removed() {
    let dir = TempDir::new("compact-nowrite");
    let (mut s, want) = layered(dir.path(), true);
    std::fs::create_dir(dir.join("sst_000004.tmp")).unwrap();
    let mut calls = 0;
    let result = s.compact_with(|_| {
        calls += 1;
        Ok(())
    });
    check!("an error and no removals", (matches!(result, Err(KvError::Io(_))), calls, s.table_count()), (true, 0, 3));
    check!("same answers", scanned(&s, "a", "z"), want);
}

#[test]
fn an_unreadable_table_stops_compaction() {
    let dir = TempDir::new("compact-noread");
    let (mut s, _) = layered(dir.path(), true);
    ruin_data(&dir.join("sst_000002.sst"));
    let mut calls = 0;
    let result = s.compact_with(|_| {
        calls += 1;
        Ok(())
    });
    check!("an error and no removals", (result.is_err(), calls, tables_in(dir.path()).len()), (true, 0, 3));
}

#[test]
fn a_crash_after_any_removal_changes_no_answer() {
    let dir = TempDir::new("compact-crash");
    let mut s = Store::open(dir.path(), NEVER).unwrap();
    // each of the later tables deletes a key that only the table before it holds
    s.put(b"a", b"a1").unwrap();
    s.put(b"keep", b"k1").unwrap();
    s.flush().unwrap();
    s.delete(b"a").unwrap();
    s.put(b"b", b"b2").unwrap();
    s.flush().unwrap();
    s.delete(b"b").unwrap();
    s.put(b"c", b"c3").unwrap();
    s.flush().unwrap();
    let want = pairs(&[("c", "c3"), ("keep", "k1")]);
    let mut removed = Vec::new();
    s.compact_with(|path| {
        std::fs::remove_file(path)?;
        removed.push(path.file_name().unwrap().to_string_lossy().into_owned());
        // what a crash here would leave, opened from a copy so that the store at work is not disturbed
        let copy = TempDir::new("compact-crash-copy");
        for name in names(dir.path()) {
            std::fs::copy(dir.join(&name), copy.join(&name))?;
        }
        let reopened = Store::open(copy.path(), NEVER).expect("reopen");
        check!(format!("after removing {removed:?}: reopening"), scanned(&reopened, "a", "z"), want.clone());
        Ok(())
    })
    .unwrap();
    removed.sort();
    check!("every old table is removed", removed, ["sst_000001.sst", "sst_000002.sst", "sst_000003.sst"].map(String::from).to_vec());
    check!("one table, same answers", (s.table_count(), scanned(&s, "a", "z")), (1, want));
}

#[test]
fn memory_and_log_are_not_touched() {
    let dir = TempDir::new("compact-mem");
    let (mut s, _) = layered(dir.path(), true);
    s.put(b"a", b"in memory").unwrap();
    s.delete(b"b").unwrap();
    let log = std::fs::read(dir.join("wal.log")).unwrap();
    let bytes = s.memtable_bytes();
    s.compact().unwrap();
    check!("log and count unchanged", (std::fs::read(dir.join("wal.log")).unwrap(), s.memtable_bytes()), (log, bytes));
    check!("memory still wins", (got(&s, "a"), got(&s, "b"), got(&s, "c")), (value("in memory"), GONE, GONE));
    let s = Store::open(dir.path(), NEVER).unwrap();
    check!("also after reopening", (got(&s, "a"), got(&s, "b")), (value("in memory"), GONE));
}

#[test]
fn large_tables_compact() {
    let dir = TempDir::new("compact-large");
    let mut s = Store::open(dir.path(), NEVER).unwrap();
    for round in 0..4u32 {
        for i in 0..1500u32 {
            if (i + round) % 7 == 0 {
                s.delete(&i.to_be_bytes()).unwrap();
            } else if i % (round + 1) == 0 {
                s.put(&i.to_be_bytes(), &round.to_be_bytes()).unwrap();
            }
        }
        s.flush().unwrap();
    }
    let before = s.scan(&[], &[255; 5]).unwrap();
    s.compact().unwrap();
    check!("same contents, one table", (s.scan(&[], &[255; 5]).unwrap() == before, s.table_count()), (true, 1));
}

#[test]
fn random_histories_with_compaction_failures_and_reopening() {
    let mut rng = anneal_prelude::Rng::new(180601);
    for case in 0..18 {
        let dir = TempDir::new("compact-random");
        let mut s = Store::open(dir.path(), [80, 160, NEVER][case % 3]).unwrap();
        let mut model = std::collections::BTreeMap::new();
        let mut story = format!("case {case}:");
        for step in 0..60 {
            let len = rng.below(3);
            let key = rng.string(len, "ab");
            match rng.below(14) {
                0..=3 => {
                    s.put(key.as_bytes(), step.to_string().as_bytes()).unwrap();
                    model.insert(key.clone(), step.to_string());
                    story.push_str(&format!(" put {key:?},"));
                }
                4..=6 => {
                    s.delete(key.as_bytes()).unwrap();
                    model.remove(&key);
                    story.push_str(&format!(" delete {key:?},"));
                }
                7 | 8 => {
                    s.flush().unwrap();
                    story.push_str(" flush,");
                }
                9 => {
                    s.compact().unwrap();
                    story.push_str(" compact,");
                }
                10 => {
                    let ok = rng.below(4);
                    let mut done = 0;
                    let _ = s.compact_with(|path| {
                        if done == ok {
                            return Err(std::io::Error::other("injected"));
                        }
                        done += 1;
                        std::fs::remove_file(path)
                    });
                    story.push_str(&format!(" compact failing after {ok},"));
                }
                11 => {
                    drop(s);
                    s = Store::open(dir.path(), 120).expect("reopen");
                    story.push_str(" reopen,");
                }
                _ => check!(format!("{story} get {key:?}"), got(&s, &key), Some(model.get(&key).cloned())),
            }
        }
        let want: Vec<(String, String)> = model.into_iter().collect();
        check!(format!("{story} final scan"), scanned(&s, "", "zzzz"), want.clone());
        check!(format!("{story} final scan after reopening"), scanned(&Store::open(dir.path(), NEVER).unwrap(), "", "zzzz"), want);
    }
}
