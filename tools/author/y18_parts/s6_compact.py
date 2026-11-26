from y17_parts.common import broken, TEMP_DIR
from y18_parts.code import STORE_HEAD, STORE_OPEN_RECOVER, STORE_OPS, TABLE_ID, COMPACT, MERGE, impl_store
from y18_parts.s4_store import BELOW, HARNESS
from y18_parts.s5_reopen import SOLUTION as STEP5

USES = r'''use std::cmp::Reverse;
use std::collections::{BTreeMap, BinaryHeap};
use std::fs::{self, File, OpenOptions};
use std::io::{self, Read, Seek, SeekFrom, Write};
use std::ops::Bound;
use std::path::{Path, PathBuf};
use thiserror::Error;
'''

SOLUTION = USES + BELOW + STORE_HEAD + impl_store(STORE_OPEN_RECOVER, STORE_OPS, COMPACT) + TABLE_ID + MERGE

STARTER = STEP5 + r'''
// ---------- step 6: your work starts here. You may also add `use` lines at the top. ----------

impl Store {
    /// Merges all tables into one and removes the old files.
    pub fn compact(&mut self) -> Result<()> {
        todo!()
    }

    /// `compact`, with the removal of a file passed in.
    pub fn compact_with(
        &mut self,
        remove: impl FnMut(&Path) -> std::io::Result<()>,
    ) -> Result<()> {
        todo!()
    }
}
'''

SETUP = r'''
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
'''

VISIBLE = HARNESS + SETUP + TEMP_DIR + r'''
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
'''

HIDDEN = HARNESS + SETUP + TEMP_DIR + r'''
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
'''

PROBLEM = dict(
    slug="compact-safely", title="Capstone 6: compaction that survives failure", level="hard", stage="the-store",
    tags=["compaction", "k-way merge", "delete markers", "crash safety", "ordering", "BinaryHeap"],
    statement=r'''
**Capstone, step 6 of 7.** Tables pile up. Reads get slower, and old values and delete markers take space. The starter holds the reference code of steps 1 to 5.

**Spoiler warning.** The starter gives away work from these parts of the series. Solve them first if you have not.

- Part 5, *An append that fails half-way*
- Part 7, *Save a file that a crash cannot tear*: also its restart rules (file names, their order and leftovers), in step 5's `open`
- Part 8, *A bundle you read from the back*
- Part 10, *One block is enough*
- Part 12, *Fix: a saved filter that forgets its keys*
- Part 14, *Unsaved changes over a read-only catalogue*

**`compact()`** replaces all tables with at most one. It is `compact_with(|path| fs::remove_file(path))`.

**`compact_with(remove)`** removes old files only through `remove`.

- With fewer than two tables it does nothing.
- The new table holds the newest version of every live key, and **no delete markers**. It takes the next id. If no key is live, no table is written.
- The memtable and the log are not touched.
- If a table cannot be read, or the new one cannot be written, nothing is removed.
- If `remove` fails, return its error at once.

**The rule that matters.** At every moment, a crash may stop compaction. So after **any** call to `remove`, whether it worked or failed:

1. reopening the directory gives the same answers as before compaction;
2. if `compact_with` has returned an error, **this** store also still gives the same answers, `table_count()` equals the number of table files, and a later `compact` works.

A deleted key must never come back. The new table does not say that it was deleted.
''',
    starter=STARTER, solution=SOLUTION, visible=[VISIBLE], hidden=[HIDDEN], use="use solution::*;", crates=["thiserror"],
    hints=[
        ("approach", "Take a key that is deleted by a newer table and held by an older one. After the merge, the new table says nothing about it. Which old files may still be there at each moment, and what do they say together?"),
        ("rust", "A `BinaryHeap` of `Reverse((key, run, value))`, with runs numbered newest first, yields keys in order and the newest version first. Dropping a `Vec::drain` iterator early removes the rest of the drained range too."),
        ("edge", "When a removal fails, the list in memory must name exactly the files that are still there, including the new one."),
    ],
    notes=("The merged table is published first, so every live key is safe in it. The old files are then removed oldest first: whatever remains of them is a newest part of the history, in which every delete marker still sits above the values it hides. A table leaves the in-memory list only after its file is gone.", "O(records x log tables)", "O(all records) in this version"),
    follow_up="1) This holds every record in memory. How would you merge and write block by block? 2) The argument assumes that removals reach the disk in the order they were made. What would you write down to avoid depending on that? 3) Compacting everything rewrites the whole store each time. How do real stores limit that, and what happens to delete markers then?",
    teaches=["The order of deletions decides what every crash point recovers to.", "Keep in-memory state in step with the disk at each fallible step, not only at the end.", "`Vec::drain` removes its whole range even when you stop early."],
    wrong={
        "removes_newest_first": broken(SOLUTION, ("            remove(self.tables[0].path())?;\n            self.tables.remove(0);\n", "            let at = old - 1 - _i;\n            remove(self.tables[at].path())?;\n            self.tables.remove(at);\n"), ("        for _ in 0..old {", "        for _i in 0..old {")),
        "drain_forgets_every_table_on_error": broken(SOLUTION, ("        let old = self.tables.len();\n        self.tables.extend(new_table);\n", ""), ("        for _ in 0..old {\n            // forget a table only after its file is gone, so that this list always matches the directory\n            remove(self.tables[0].path())?;\n            self.tables.remove(0);\n        }\n", "        for table in self.tables.drain(..) {\n            remove(table.path())?;\n        }\n        self.tables.extend(new_table);\n")),
        "new_table_listed_only_at_the_end": broken(SOLUTION, ("        let old = self.tables.len();\n        self.tables.extend(new_table);\n", "        let old = self.tables.len();\n"), ("            self.tables.remove(0);\n        }\n        Ok(())", "            self.tables.remove(0);\n        }\n        self.tables.extend(new_table);\n        Ok(())")),
        "keeps_removing_after_a_failure": broken(SOLUTION, ("        for _ in 0..old {\n            // forget a table only after its file is gone, so that this list always matches the directory\n            remove(self.tables[0].path())?;\n            self.tables.remove(0);\n        }\n        Ok(())", "        let mut kept = 0;\n        let mut failure = None;\n        for _ in 0..old {\n            match remove(self.tables[kept].path()) {\n                Ok(()) => {\n                    self.tables.remove(kept);\n                }\n                Err(error) => {\n                    failure = Some(error);\n                    kept += 1;\n                }\n            }\n        }\n        failure.map_or(Ok(()), |e| Err(e.into()))")),
        "merged_table_keeps_markers": broken(SOLUTION, ("        if value.is_some() {\n            out.push((key.clone(), value));\n        }\n", "        out.push((key.clone(), value));\n")),
        "the_older_version_wins": broken(SOLUTION, ("        for table in self.tables.iter().rev() {\n            runs.push", "        for table in &self.tables {\n            runs.push")),
        "removes_before_writing": broken(SOLUTION, ("        let new_table = if merged.is_empty() {", "        let old = self.tables.len();\n        for _ in 0..old {\n            remove(self.tables[0].path())?;\n            self.tables.remove(0);\n        }\n        let new_table = if merged.is_empty() {"), ("        let old = self.tables.len();\n        self.tables.extend(new_table);\n        // oldest first: the merged table has no delete markers, so a marker must outlive every older value it hides\n        for _ in 0..old {\n            // forget a table only after its file is gone, so that this list always matches the directory\n            remove(self.tables[0].path())?;\n            self.tables.remove(0);\n        }\n", "        self.tables.extend(new_table);\n")),
    },
)
