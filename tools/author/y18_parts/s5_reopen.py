from y17_parts.common import broken, TEMP_DIR
from y18_parts.code import STORE_HEAD, STORE_OPEN_RECOVER, STORE_OPS, TABLE_ID, impl_store
from y18_parts.s4_store import USES, BELOW, HARNESS, SOLUTION as STEP4

SOLUTION = USES + BELOW + STORE_HEAD + impl_store(STORE_OPEN_RECOVER, STORE_OPS) + TABLE_ID

STARTER = STEP4 + r'''
// ---------- step 5: replace the body of `Store::open` above so that it also works on a directory used before ----------
'''
# the step-4 open ignores what is on disk; say so where the learner has to work
STARTER = STARTER.replace("    /// Opens a store in `dir`, which is new or empty.\n", "    /// Step 5: this works only for a new directory. Make it open any directory the store has used.\n")

VISIBLE = HARNESS + TEMP_DIR + r'''
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
'''

HIDDEN = HARNESS + TEMP_DIR + r'''
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
'''

PROBLEM = dict(
    slug="reopen-after-a-crash", title="Capstone 5: open after any crash", level="hard", stage="the-store",
    tags=["crash recovery", "read_dir", "startup", "write-ahead log", "file naming"],
    statement=r'''
**Capstone, step 5 of 7.** The process can die at any moment. The starter holds the reference code of steps 1 to 4, in which `Store::open` only works on a new directory. Make it open any directory the store has used.

**Spoiler warning.** The starter gives away work from these parts of the series. Solve them first if you have not.

- Part 5, *An append that fails half-way*
- Part 7, *Save a file that a crash cannot tear*
- Part 8, *A bundle you read from the back*
- Part 10, *One block is enough*
- Part 12, *Fix: a saved filter that forgets its keys*
- Part 14, *Unsaved changes over a read-only catalogue*: the way to keep deletes over older data

**What a crash can leave:**

- tables under their final names, which are complete;
- a half-written table under its temporary name;
- a log with a torn or zero-filled end;
- a log that still holds writes which a table already has, if the crash came during a flush.

**`open(dir, flush_threshold)`** creates the directory if needed. After it, `get` and `scan` answer as just before the crash, apart from writes whose frame did not reach the log whole.

- A table is a file whose name is exactly what the store would write: `sst_<id>.sst`. A temporary table is the same with `.tmp`. **Remove** temporary tables. Leave every other file alone.
- A table or log that is `Corrupt` makes `open` fail. Do not skip it.
- A new table's id is one more than the highest id present, and never reuses one.
- `memtable_bytes()` counts the writes replayed from the log, by the rule of step 4.

Ids may grow past six digits.
''',
    starter=STARTER, solution=SOLUTION, visible=[VISIBLE], hidden=[HIDDEN], use="use solution::*;", crates=["thiserror"],
    hints=[
        ("approach", "Go through the four things a crash can leave, and say for each what `open` does about it. Most of the work was done by earlier steps."),
        ("rust", "`fs::read_dir` yields entries in no particular order. `strip_prefix` and `strip_suffix` pick a name apart. Formatting the parsed number again shows whether the name was in the store's own form."),
        ("edge", "Sorted as text, `sst_1000000.sst` comes before `sst_999999.sst`. `parse::<u64>()` accepts a leading `+`."),
    ],
    notes=("Open removes unfinished tables, loads the finished ones in numeric order and replays the log into the memtable. Replaying writes that a table already holds changes nothing, because the memtable gives the same answers. Names are accepted only in the exact form the store writes.", "O(files + table bytes + log)", "O(indexes + log)"),
    follow_up="1) Why is replaying already-flushed writes harmless here? Name an operation for which it would not be. 2) Two processes could open the same directory. What would each destroy, and what lock would stop it? 3) With thousands of tables, opening is slow. What could the store write down to avoid listing and checking them all?",
    teaches=["Recovery is the other half of every write path: list what each crash can leave and handle each.", "Compare file ids as numbers.", "Delete only files you are sure are yours."],
    wrong={
        "sorts_by_name": broken(SOLUTION, ("        found.sort_by_key(|f| f.0);\n", "        found.sort_by_key(|f| f.1.clone());\n")),
        "next_id_from_the_count": broken(SOLUTION, ("        let next_id = tables.last().map_or(1, |t| t.id() + 1);\n", "        let next_id = tables.len() as u64 + 1;\n")),
        "removes_every_tmp_file": broken(SOLUTION, ('            if table_id(name, ".tmp").is_some() {', '            if name.ends_with(".tmp") {')),
        "skips_damaged_tables": broken(SOLUTION, ("            tables.push(SsTable::open(path, *id)?);", "            if let Ok(table) = SsTable::open(path, *id) {\n                tables.push(table);\n            }")),
        "replay_is_not_counted": broken(SOLUTION, ("        for (key, value) in entries {\n            store.remember(key, value);\n        }\n", "        for (key, value) in entries {\n            store.memtable.insert(key, value);\n        }\n")),
        "any_number_form_is_a_table": broken(SOLUTION, ('    (name == format!("sst_{id:06}{ending}")).then_some(id)', '    let _ = name;\n    Some(id)')),
        "the_log_is_applied_only_as_puts": broken(SOLUTION, ("        for (key, value) in entries {\n            store.remember(key, value);\n        }\n", "        for (key, value) in entries {\n            if value.is_some() {\n                store.remember(key, value);\n            }\n        }\n")),
    },
)
