from y17_parts.common import broken, TEMP_DIR
from y18_parts.code import TYPES, CODEC, LOG_FILE_TRAIT, LOG_FILE_IMPL, WAL

USES = r'''use std::fs::{File, OpenOptions};
use std::io::{self, Read, Seek, SeekFrom, Write};
use std::path::Path;
use thiserror::Error;
'''

BELOW = TYPES + CODEC + LOG_FILE_TRAIT + LOG_FILE_IMPL

SOLUTION = USES + BELOW + WAL

STARTER = USES + BELOW + r'''
// ---------- step 2: your work starts here ----------

/// The write-ahead log.
pub struct Wal<F: LogFile = File> {
    file: F,
}

impl Wal<File> {
    /// Opens the log at `path`, creating it if it is missing, and returns the records it holds.
    pub fn open(path: &Path) -> Result<(Wal<File>, Vec<Entry>)> {
        todo!()
    }
}

impl<F: LogFile> Wal<F> {
    pub fn with_file(file: F) -> Result<(Wal<F>, Vec<Entry>)> {
        todo!()
    }

    pub fn append(&mut self, key: &[u8], value: Option<&[u8]>) -> Result<()> {
        todo!()
    }

    pub fn sync(&mut self) -> Result<()> {
        todo!()
    }

    /// Empties the log.
    pub fn reset(&mut self) -> Result<()> {
        todo!()
    }

    /// The number of bytes in the log.
    pub fn len(&self) -> u64 {
        todo!()
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
}
'''

HARNESS = r'''
use std::cell::RefCell;
use std::io;
use std::rc::Rc;

#[allow(dead_code)]
#[derive(Default)]
struct State {
    data: Vec<u8>,
    short_append: Option<usize>,
    fail_truncate: bool,
    fail_sync: bool,
    fail_read: bool,
    calls: Vec<String>,
}

#[allow(dead_code)]
#[derive(Clone, Default)]
struct Fake(Rc<RefCell<State>>);

impl LogFile for Fake {
    fn read_all(&mut self) -> io::Result<Vec<u8>> {
        let s = self.0.borrow();
        if s.fail_read {
            return Err(io::Error::from(io::ErrorKind::PermissionDenied));
        }
        Ok(s.data.clone())
    }
    fn append(&mut self, bytes: &[u8]) -> io::Result<()> {
        let mut s = self.0.borrow_mut();
        s.calls.push("append".into());
        if let Some(keep) = s.short_append.take() {
            let keep = keep.min(bytes.len());
            s.data.extend_from_slice(&bytes[..keep]);
            return Err(io::Error::from(io::ErrorKind::StorageFull));
        }
        s.data.extend_from_slice(bytes);
        Ok(())
    }
    fn truncate(&mut self, len: u64) -> io::Result<()> {
        let mut s = self.0.borrow_mut();
        s.calls.push(format!("truncate {len}"));
        if std::mem::take(&mut s.fail_truncate) {
            return Err(io::Error::from(io::ErrorKind::PermissionDenied));
        }
        s.data.truncate(len as usize);
        Ok(())
    }
    fn sync(&mut self) -> io::Result<()> {
        let mut s = self.0.borrow_mut();
        s.calls.push("sync".into());
        if std::mem::take(&mut s.fail_sync) {
            return Err(io::Error::from(io::ErrorKind::Other));
        }
        Ok(())
    }
}

#[allow(dead_code)]
fn put(k: &str, v: &str) -> Entry {
    (k.as_bytes().to_vec(), Some(v.as_bytes().to_vec()))
}

#[allow(dead_code)]
fn log_of(entries: &[Entry]) -> Vec<u8> {
    entries.iter().flat_map(|(k, v)| encode_frame(k, v.as_deref())).collect()
}

#[allow(dead_code)]
fn kind<T>(r: Result<T>) -> String {
    match r {
        Ok(_) => "ok".into(),
        Err(KvError::Io(e)) => format!("io {:?}", e.kind()),
        Err(KvError::Corrupt(_)) => "corrupt".into(),
        Err(KvError::Unusable) => "unusable".into(),
    }
}

#[allow(dead_code)]
fn entries_at(path: &std::path::Path) -> Vec<Entry> {
    Wal::open(path).expect("open").1
}
'''

VISIBLE = HARNESS + TEMP_DIR + r'''
#[test]
fn what_is_appended_is_there_after_reopening() {
    let dir = TempDir::new("wal-visible");
    let path = dir.join("wal.log");
    let (mut wal, before) = Wal::open(&path).unwrap();
    wal.append(b"a", Some(b"1")).unwrap();
    wal.append(b"a", None).unwrap();
    drop(wal);
    check!("a new log, then two appends and a reopen", (before, entries_at(&path)), (vec![], vec![put("a", "1"), (b"a".to_vec(), None)]));
}

#[test]
fn the_file_holds_exactly_the_frames() {
    let dir = TempDir::new("wal-bytes");
    let path = dir.join("wal.log");
    let (mut wal, _) = Wal::open(&path).unwrap();
    wal.append(b"k", Some(b"v")).unwrap();
    let want = log_of(&[put("k", "v")]);
    check!("bytes and len", (std::fs::read(&path).unwrap(), wal.len()), (want.clone(), want.len() as u64));
}

#[test]
fn opening_cuts_off_a_torn_tail() {
    let dir = TempDir::new("wal-torn");
    let path = dir.join("wal.log");
    let mut bytes = log_of(&[put("a", "1")]);
    let whole = bytes.len() as u64;
    bytes.extend(&encode_frame(b"b", Some(b"2"))[..9]);
    std::fs::write(&path, bytes).unwrap();
    let (mut wal, got) = Wal::open(&path).unwrap();
    check!("the file is cut on open", (got, std::fs::metadata(&path).unwrap().len()), (vec![put("a", "1")], whole));
    wal.append(b"c", Some(b"3")).unwrap();
    check!("later appends are readable", entries_at(&path), vec![put("a", "1"), put("c", "3")]);
}

#[test]
fn reset_empties_the_file() {
    let dir = TempDir::new("wal-reset");
    let path = dir.join("wal.log");
    let (mut wal, _) = Wal::open(&path).unwrap();
    wal.append(b"old", Some(b"1")).unwrap();
    wal.reset().unwrap();
    let emptied = (wal.len(), std::fs::metadata(&path).unwrap().len());
    wal.append(b"new", Some(b"2")).unwrap();
    check!("reset, then append", (emptied, entries_at(&path)), ((0, 0), vec![put("new", "2")]));
}

#[test]
fn a_failed_append_leaves_no_trace() {
    let fake = Fake::default();
    let (mut wal, _) = Wal::with_file(fake.clone()).unwrap();
    wal.append(b"a", Some(b"1")).unwrap();
    fake.0.borrow_mut().short_append = Some(5);
    check!("the append fails with the device's error", kind(wal.append(b"b", Some(b"2"))), "io StorageFull".to_string());
    wal.append(b"c", Some(b"3")).unwrap();
    check!("what a reader finds", fake.0.borrow().data.clone(), log_of(&[put("a", "1"), put("c", "3")]));
}
'''

HIDDEN = HARNESS + TEMP_DIR + r'''
#[test]
fn a_zero_filled_tail_opens() {
    for zeros in [1usize, 7, 8, 9, 100, 4096] {
        let dir = TempDir::new("wal-zeros");
        let path = dir.join("wal.log");
        let mut bytes = log_of(&[put("a", "1")]);
        let whole = bytes.len() as u64;
        bytes.extend(vec![0; zeros]);
        std::fs::write(&path, bytes).unwrap();
        let opened = Wal::open(&path).map(|(w, e)| (w.len(), e)).ok();
        check!(format!("{zeros} zeros after one frame"), opened, Some((whole, vec![put("a", "1")])));
        check!(format!("{zeros} zeros: cut from the file"), std::fs::metadata(&path).unwrap().len(), whole);
    }
}

#[test]
fn every_cut_of_a_real_file() {
    let dir = TempDir::new("wal-cuts");
    let path = dir.join("wal.log");
    let entries = vec![put("one", "1"), (b"one".to_vec(), None), put("two", "22")];
    let whole = log_of(&entries);
    for cut in 0..=whole.len() {
        std::fs::write(&path, &whole[..cut]).unwrap();
        let (mut wal, got) = Wal::open(&path).unwrap();
        let n = (0..=entries.len()).rev().find(|&i| log_of(&entries[..i]).len() <= cut).unwrap();
        check!(format!("cut at {cut}"), got, entries[..n].to_vec());
        wal.append(b"z", Some(b"z")).unwrap();
        let mut want = entries[..n].to_vec();
        want.push(put("z", "z"));
        check!(format!("cut at {cut}, then an append"), entries_at(&path), want.clone());
        check!(format!("cut at {cut}: no stray bytes"), std::fs::read(&path).unwrap(), log_of(&want));
    }
}

#[test]
fn corruption_is_reported_and_the_file_is_left_alone() {
    let dir = TempDir::new("wal-corrupt");
    let path = dir.join("wal.log");
    let payload = [9u8; 9];
    let mut bytes = log_of(&[put("a", "1")]);
    bytes.extend((payload.len() as u32).to_le_bytes());
    bytes.extend(crc32(&payload).to_le_bytes());
    bytes.extend(payload);
    bytes.extend([1, 2, 3]);
    std::fs::write(&path, &bytes).unwrap();
    check!("a checked frame of nonsense", kind(Wal::open(&path)), "corrupt".to_string());
    check!("nothing was cut", std::fs::read(&path).unwrap(), bytes);
}

#[test]
fn a_clean_log_is_not_touched() {
    let fake = Fake::default();
    fake.0.borrow_mut().data = log_of(&[put("a", "1")]);
    let (wal, got) = Wal::with_file(fake.clone()).unwrap();
    let want = log_of(&[put("a", "1")]);
    check!("one whole frame", (got, wal.len(), fake.0.borrow().data.clone()), (vec![put("a", "1")], want.len() as u64, want));
}

#[test]
fn a_read_error_is_not_an_empty_log() {
    let fake = Fake::default();
    fake.0.borrow_mut().data = log_of(&[put("a", "1")]);
    fake.0.borrow_mut().fail_read = true;
    check!("read fails", kind(Wal::with_file(fake.clone())), "io PermissionDenied".to_string());
    check!("data untouched", fake.0.borrow().data.clone(), log_of(&[put("a", "1")]));
    let dir = TempDir::new("wal-isdir");
    check!("a directory in place of the log", Wal::open(dir.path()).is_err(), true);
}

#[test]
fn if_the_bad_bytes_cannot_be_removed_the_log_stops() {
    let fake = Fake::default();
    let (mut wal, _) = Wal::with_file(fake.clone()).unwrap();
    fake.0.borrow_mut().short_append = Some(3);
    fake.0.borrow_mut().fail_truncate = true;
    check!("the first failure is the append's", kind(wal.append(b"a", Some(b"1"))), "io StorageFull".to_string());
    let calls = fake.0.borrow().calls.len();
    let after = (kind(wal.append(b"b", None)), kind(wal.sync()), kind(wal.reset()));
    check!("everything after it", after, ("unusable".to_string(), "unusable".to_string(), "unusable".to_string()));
    check!("without touching the file", fake.0.borrow().calls.len(), calls);
}

#[test]
fn a_failed_sync_or_reset_stops_the_log() {
    let fake = Fake::default();
    let (mut wal, _) = Wal::with_file(fake.clone()).unwrap();
    wal.append(b"a", Some(b"1")).unwrap();
    fake.0.borrow_mut().fail_sync = true;
    check!("sync fails", (kind(wal.sync()), kind(wal.sync()), kind(wal.append(b"b", None))), ("io Other".to_string(), "unusable".to_string(), "unusable".to_string()));
    let fake = Fake::default();
    let (mut wal, _) = Wal::with_file(fake.clone()).unwrap();
    wal.append(b"a", Some(b"1")).unwrap();
    fake.0.borrow_mut().fail_truncate = true;
    check!("reset fails", (kind(wal.reset()), kind(wal.append(b"b", None))), ("io PermissionDenied".to_string(), "unusable".to_string()));
}

#[test]
fn only_sync_syncs() {
    let fake = Fake::default();
    let (mut wal, _) = Wal::with_file(fake.clone()).unwrap();
    // what opening does is not at issue here
    fake.0.borrow_mut().calls.clear();
    wal.append(b"a", None).unwrap();
    wal.append(b"b", None).unwrap();
    wal.sync().unwrap();
    check!("two appends and one sync", fake.0.borrow().calls.clone(), ["append", "append", "sync"].map(String::from).to_vec());
}

#[test]
fn random_use_with_failures_and_reopening() {
    let mut rng = anneal_prelude::Rng::new(180201);
    for case in 0..200 {
        let fake = Fake::default();
        let (mut wal, _) = Wal::with_file(fake.clone()).unwrap();
        let mut model: Vec<Entry> = Vec::new();
        let mut story = format!("case {case}:");
        for step in 0..20 {
            match rng.below(8) {
                0 => {
                    wal.reset().unwrap();
                    model.clear();
                    story.push_str(" reset,");
                }
                1 => {
                    // the process dies part-way through writing one more frame
                    let frame = encode_frame(b"torn", Some(b"torn"));
                    let keep = rng.below(frame.len());
                    fake.0.borrow_mut().data.extend_from_slice(&frame[..keep]);
                    let (new, got) = Wal::with_file(fake.clone()).unwrap();
                    wal = new;
                    story.push_str(&format!(" crash keeping {keep},"));
                    check!(story.clone(), got, model.clone());
                }
                2 => {
                    let keep = rng.below(25);
                    fake.0.borrow_mut().short_append = Some(keep);
                    check!(format!("{story} short append {keep}"), wal.append(b"lost", Some(b"lost")).is_err(), true);
                    story.push_str(&format!(" short {keep},"));
                }
                _ => {
                    let k = vec![rng.below(4) as u8];
                    let v = if rng.bool() { Some(vec![step as u8]) } else { None };
                    wal.append(&k, v.as_deref()).unwrap();
                    model.push((k, v));
                    story.push_str(" append,");
                }
            }
            check!(format!("{story} the file"), fake.0.borrow().data.clone(), log_of(&model));
            check!(format!("{story} len"), wal.len(), log_of(&model).len() as u64);
        }
    }
}
'''

PROBLEM = dict(
    slug="log-on-disk", title="Capstone 2: the log on disk", level="medium", stage="on-disk",
    tags=["write-ahead log", "File", "recovery", "truncate", "failure handling"],
    statement=r'''
**Capstone, step 2 of 7.** The store writes every change to a log file before it applies it. The starter holds the reference code of step 1, and the `LogFile` trait with its implementation for a real file.

**Spoiler warning.** The starter gives away work from these parts of the series. Solve them first if you have not.

- Part 4, *A key-value store with a write-ahead log that survives torn writes*: the torn-end rule, in the step 1 code
- Part 5, *An append that fails half-way*: how to solve its real-file device

Write `Wal`, the log. It works on any `LogFile`, so tests can script failures.

**`open(path)`** creates the file if it is missing. It returns the log and the records the file holds. `with_file` does the same for a given file.

- If the end is torn, in any of the ways `replay` accepts, **cut it off the file**. Later appends must be readable.
- A `Corrupt` error from `replay`, or a failed read, is returned, and the file is not changed.

**`append(key, value)`** adds one frame with one call to the file's `append`. It does not sync.

- If the call fails, put the file back as it was, trying once, and return the error. The log stays usable.
- If that also fails, return the **append's** error. The log is now unusable.

**`sync()`** syncs the file. **`reset()`** empties it. If either call fails, return the error; the log is then unusable.

Once unusable, `append`, `sync` and `reset` return `KvError::Unusable` without touching the file. **`len()`** is the number of bytes of whole frames in the log.
''',
    starter=STARTER, solution=SOLUTION, visible=[VISIBLE], hidden=[HIDDEN], use="use solution::*;", crates=["thiserror"],
    hints=[
        ("approach", "Two things must be remembered between calls: how long the good part of the file is, and whether the log can still be trusted."),
        ("rust", "The given implementation expects a file opened with `OpenOptions` for reading and appending, created if missing. `Wal<F: LogFile = File>` lets `Wal` alone mean the real one."),
        ("edge", "Cutting the tail must happen when the log is opened, not at the next append. After a `Corrupt` error the evidence must stay on disk."),
    ],
    notes=("Opening replays the file and truncates it to the valid length, so new frames never sit behind damaged bytes. A failed append is rolled back to the remembered length. Whenever the file's state is no longer known, the log refuses further work.", "append O(record); open O(log)", "O(record) per append"),
    follow_up="1) `append` returns before the data is safe from a power cut. Who in the store should decide when to call `sync`, and what would one `sync` per write cost? 2) What else must happen once when the log file is first created, to be sure it exists after a power cut? 3) Two processes open the same log. What goes wrong, and how would you stop it?",
    teaches=["Recovery repairs the file as well as reading it, or later writes are lost behind the damage.", "Roll back a partial append before anything else is written.", "A trait around the file makes disk failures testable."],
    wrong={
        "reads_but_does_not_truncate": broken(SOLUTION, ("        if valid < bytes.len() {\n            file.truncate(valid as u64)?;\n        }\n", "")),
        "keeps_a_partial_append": broken(SOLUTION, ("            if self.file.truncate(self.len).is_err() {\n                self.broken = true;\n            }\n", "")),
        "ignores_a_failed_rollback": broken(SOLUTION, ("            if self.file.truncate(self.len).is_err() {\n                self.broken = true;\n            }\n", "            let _ = self.file.truncate(self.len);\n")),
        "sync_may_be_retried": broken(SOLUTION, ("        self.file.sync().map_err(|error| {\n            self.broken = true;\n            error.into()\n        })", "        Ok(self.file.sync()?)")),
        "an_unreadable_log_is_empty": broken(SOLUTION, ("        let bytes = file.read_all()?;", "        let bytes = file.read_all().unwrap_or_default();")),
        "truncates_whatever_replay_dislikes": broken(SOLUTION, ("        let (entries, valid) = replay(&bytes)?;", "        let (entries, valid) = match replay(&bytes) {\n            Ok(found) => found,\n            Err(error) => {\n                file.truncate(0)?;\n                return Err(error);\n            }\n        };")),
        "length_forgotten_after_reopen": broken(SOLUTION, ("            len: valid as u64,\n", "            len: 0,\n")),
    },
)
