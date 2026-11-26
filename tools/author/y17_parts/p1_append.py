from y17_parts.common import broken, TEMP_DIR

STARTER = r'''use std::fs::File;
use std::io;
use std::path::Path;

/// An append-only file, or a stand-in for one.
#[allow(clippy::len_without_is_empty)]
pub trait LogDevice {
    /// The number of bytes in it now.
    fn len(&mut self) -> io::Result<u64>;
    /// Adds `bytes` at the end. When it fails, **any prefix** of `bytes` may have been added.
    fn append(&mut self, bytes: &[u8]) -> io::Result<()>;
    /// Cuts it down to `len` bytes.
    fn truncate(&mut self, len: u64) -> io::Result<()>;
    /// Makes what was added so far survive a power cut.
    fn sync(&mut self) -> io::Result<()>;
}

/// The real thing.
pub struct FileDevice {
    file: File,
}

impl FileDevice {
    /// Opens `path`, creating it if it is missing. What it already holds stays.
    pub fn open(path: &Path) -> io::Result<FileDevice> {
        todo!()
    }
}

impl LogDevice for FileDevice {
    fn len(&mut self) -> io::Result<u64> {
        todo!()
    }
    fn append(&mut self, bytes: &[u8]) -> io::Result<()> {
        todo!()
    }
    fn truncate(&mut self, len: u64) -> io::Result<()> {
        todo!()
    }
    fn sync(&mut self) -> io::Result<()> {
        todo!()
    }
}

#[derive(Debug, PartialEq, Eq, Clone, Copy)]
pub enum AppendError {
    /// This record was not added. The log is as it was before the call, and still usable.
    NotAdded(io::ErrorKind),
    /// This call left the log in a state it cannot vouch for. It takes no more records.
    Broke(io::ErrorKind),
    /// An earlier call returned `Broke`.
    Unusable,
}

pub struct Log<D: LogDevice> {
    device: D,
}

impl<D: LogDevice> Log<D> {
    pub fn new(device: D) -> io::Result<Log<D>> {
        todo!()
    }

    /// Adds one record as a 4-byte little-endian length followed by the record. `Ok` means it is on disk for good.
    pub fn append(&mut self, record: &[u8]) -> Result<(), AppendError> {
        todo!()
    }
}
'''

SOLUTION = r'''use std::fs::{File, OpenOptions};
use std::io::{self, Write};
use std::path::Path;

/// An append-only file, or a stand-in for one.
#[allow(clippy::len_without_is_empty)]
pub trait LogDevice {
    /// The number of bytes in it now.
    fn len(&mut self) -> io::Result<u64>;
    /// Adds `bytes` at the end. When it fails, **any prefix** of `bytes` may have been added.
    fn append(&mut self, bytes: &[u8]) -> io::Result<()>;
    /// Cuts it down to `len` bytes.
    fn truncate(&mut self, len: u64) -> io::Result<()>;
    /// Makes what was added so far survive a power cut.
    fn sync(&mut self) -> io::Result<()>;
}

/// The real thing.
pub struct FileDevice {
    file: File,
}

impl FileDevice {
    /// Opens `path`, creating it if it is missing. What it already holds stays.
    pub fn open(path: &Path) -> io::Result<FileDevice> {
        // append mode: every write goes to the current end, also after the file was cut shorter
        let file = OpenOptions::new().create(true).append(true).open(path)?;
        Ok(FileDevice { file })
    }
}

impl LogDevice for FileDevice {
    fn len(&mut self) -> io::Result<u64> {
        Ok(self.file.metadata()?.len())
    }
    fn append(&mut self, bytes: &[u8]) -> io::Result<()> {
        self.file.write_all(bytes)
    }
    fn truncate(&mut self, len: u64) -> io::Result<()> {
        self.file.set_len(len)
    }
    fn sync(&mut self) -> io::Result<()> {
        self.file.sync_data()
    }
}

#[derive(Debug, PartialEq, Eq, Clone, Copy)]
pub enum AppendError {
    /// This record was not added. The log is as it was before the call, and still usable.
    NotAdded(io::ErrorKind),
    /// This call left the log in a state it cannot vouch for. It takes no more records.
    Broke(io::ErrorKind),
    /// An earlier call returned `Broke`.
    Unusable,
}

pub struct Log<D: LogDevice> {
    device: D,
    /// The length of the part made of whole, acknowledged records.
    len: u64,
    broken: bool,
}

impl<D: LogDevice> Log<D> {
    pub fn new(mut device: D) -> io::Result<Log<D>> {
        let len = device.len()?;
        Ok(Log {
            device,
            len,
            broken: false,
        })
    }

    /// Adds one record as a 4-byte little-endian length followed by the record. `Ok` means it is on disk for good.
    pub fn append(&mut self, record: &[u8]) -> Result<(), AppendError> {
        if self.broken {
            return Err(AppendError::Unusable);
        }
        let mut frame = Vec::with_capacity(4 + record.len());
        frame.extend_from_slice(&(record.len() as u32).to_le_bytes());
        frame.extend_from_slice(record);
        if let Err(error) = self.device.append(&frame) {
            // part of the frame may be there; later records must not land behind it
            if self.device.truncate(self.len).is_err() {
                self.broken = true;
                return Err(AppendError::Broke(error.kind()));
            }
            return Err(AppendError::NotAdded(error.kind()));
        }
        if let Err(error) = self.device.sync() {
            // a second sync that succeeds would not show that these bytes were kept
            self.broken = true;
            return Err(AppendError::Broke(error.kind()));
        }
        self.len += frame.len() as u64;
        Ok(())
    }
}
'''

HARNESS = r'''
use std::cell::RefCell;
use std::io;
use std::rc::Rc;

/// What the next call of one kind does.
#[allow(dead_code)]
#[derive(Default)]
struct Plan {
    /// The next append keeps this many bytes of what it was given and fails.
    short_append: Option<usize>,
    fail_truncate: bool,
    fail_sync: bool,
}

#[allow(dead_code)]
#[derive(Default)]
struct Disk {
    data: Vec<u8>,
    plan: Plan,
    /// Every call that changes or syncs the device.
    calls: Vec<String>,
}

#[allow(dead_code)]
#[derive(Clone, Default)]
struct Fake(Rc<RefCell<Disk>>);

impl LogDevice for Fake {
    fn len(&mut self) -> io::Result<u64> {
        Ok(self.0.borrow().data.len() as u64)
    }
    fn append(&mut self, bytes: &[u8]) -> io::Result<()> {
        let mut d = self.0.borrow_mut();
        d.calls.push(format!("append {}", bytes.len()));
        if let Some(keep) = d.plan.short_append.take() {
            let keep = keep.min(bytes.len());
            d.data.extend_from_slice(&bytes[..keep]);
            return Err(io::Error::from(io::ErrorKind::StorageFull));
        }
        d.data.extend_from_slice(bytes);
        Ok(())
    }
    fn truncate(&mut self, len: u64) -> io::Result<()> {
        let mut d = self.0.borrow_mut();
        d.calls.push(format!("truncate {len}"));
        if std::mem::take(&mut d.plan.fail_truncate) {
            return Err(io::Error::from(io::ErrorKind::PermissionDenied));
        }
        assert!(len as usize <= d.data.len(), "truncate must not grow the log");
        d.data.truncate(len as usize);
        Ok(())
    }
    fn sync(&mut self) -> io::Result<()> {
        let mut d = self.0.borrow_mut();
        d.calls.push("sync".to_string());
        if std::mem::take(&mut d.plan.fail_sync) {
            return Err(io::Error::from(io::ErrorKind::Other));
        }
        Ok(())
    }
}

/// The whole records at the front of `data`, and whether anything is left over.
#[allow(dead_code)]
fn records(mut data: &[u8]) -> (Vec<Vec<u8>>, bool) {
    let mut out = Vec::new();
    while data.len() >= 4 {
        let n = u32::from_le_bytes(data[..4].try_into().unwrap()) as usize;
        if data.len() - 4 < n {
            break;
        }
        out.push(data[4..4 + n].to_vec());
        data = &data[4 + n..];
    }
    (out, !data.is_empty())
}

#[allow(dead_code)]
fn new_log() -> (Log<Fake>, Fake) {
    let fake = Fake::default();
    (Log::new(fake.clone()).unwrap(), fake)
}
'''

VISIBLE = HARNESS + TEMP_DIR + r'''
#[test]
fn records_are_framed_and_synced() {
    let (mut log, fake) = new_log();
    let results = (log.append(b"ab"), log.append(b""));
    let d = fake.0.borrow();
    check!("append ab, then an empty record", (results, d.data.clone()), ((Ok(()), Ok(())), vec![2, 0, 0, 0, b'a', b'b', 0, 0, 0, 0]));
    check!("one append and then a sync for each", d.calls.clone(), ["append 6", "sync", "append 4", "sync"].map(String::from).to_vec());
}

#[test]
fn a_half_written_record_is_removed() {
    let (mut log, fake) = new_log();
    log.append(b"first").unwrap();
    fake.0.borrow_mut().plan.short_append = Some(3);
    let failed = log.append(b"second");
    let after = log.append(b"third");
    let data = fake.0.borrow().data.clone();
    check!("the second append stops after 3 bytes", (failed, after), (Err(AppendError::NotAdded(io::ErrorKind::StorageFull)), Ok(())));
    check!("what a reader finds", records(&data), (vec![b"first".to_vec(), b"third".to_vec()], false));
}

#[test]
fn a_failed_sync_breaks_the_log() {
    let (mut log, fake) = new_log();
    fake.0.borrow_mut().plan.fail_sync = true;
    check!("sync fails", log.append(b"x"), Err(AppendError::Broke(io::ErrorKind::Other)));
    let before = fake.0.borrow().calls.len();
    check!("the next append", log.append(b"y"), Err(AppendError::Unusable));
    check!("no further calls on the device", fake.0.borrow().calls.len(), before);
}

#[test]
fn a_new_log_does_nothing_to_the_device() {
    let (_log, fake) = new_log();
    check!("Log::new", fake.0.borrow().calls.len(), 0);
}

#[test]
fn a_real_file_keeps_what_it_has() {
    let dir = TempDir::new("append-visible");
    let path = dir.join("log");
    std::fs::write(&path, b"old").unwrap();
    let mut device = FileDevice::open(&path).unwrap();
    device.append(b"new").unwrap();
    device.sync().unwrap();
    check!("open, append", (std::fs::read(&path).unwrap(), device.len().unwrap()), (b"oldnew".to_vec(), 6));
}
'''

HIDDEN = HARNESS + TEMP_DIR + r'''
#[test]
fn a_failed_truncate_breaks_the_log() {
    let (mut log, fake) = new_log();
    log.append(b"kept").unwrap();
    {
        let mut d = fake.0.borrow_mut();
        d.plan.short_append = Some(2);
        d.plan.fail_truncate = true;
    }
    check!("append fails and cannot be undone: the append's error kind", log.append(b"lost"), Err(AppendError::Broke(io::ErrorKind::StorageFull)));
    let before = (fake.0.borrow().calls.len(), fake.0.borrow().data.clone());
    for _ in 0..3 {
        check!("afterwards", log.append(b"more"), Err(AppendError::Unusable));
    }
    check!("the device was left alone", (fake.0.borrow().calls.len(), fake.0.borrow().data.clone()), before.clone());
    check!("the acknowledged record is still first", records(&before.1).0, vec![b"kept".to_vec()]);
}

#[test]
fn nothing_follows_a_failed_sync() {
    let (mut log, fake) = new_log();
    log.append(b"a").unwrap();
    fake.0.borrow_mut().plan.fail_sync = true;
    let got = log.append(b"b");
    check!("sync error", got, Err(AppendError::Broke(io::ErrorKind::Other)));
    check!("the calls", fake.0.borrow().calls.clone(), ["append 5", "sync", "append 5", "sync"].map(String::from).to_vec());
}

#[test]
fn several_failures_in_a_row() {
    let (mut log, fake) = new_log();
    for keep in [0, 1, 4, 5, 100] {
        fake.0.borrow_mut().plan.short_append = Some(keep);
        check!(format!("failure keeping {keep} bytes"), log.append(b"zz"), Err(AppendError::NotAdded(io::ErrorKind::StorageFull)));
        check!(format!("after keeping {keep}"), fake.0.borrow().data.clone(), Vec::<u8>::new());
    }
    log.append(b"ok").unwrap();
    check!("then one that works", records(&fake.0.borrow().data), (vec![b"ok".to_vec()], false));
}

#[test]
fn a_log_that_already_has_records() {
    let fake = Fake::default();
    fake.0.borrow_mut().data = vec![1, 0, 0, 0, 7];
    let mut log = Log::new(fake.clone()).unwrap();
    fake.0.borrow_mut().plan.short_append = Some(3);
    let _ = log.append(b"abc");
    check!("the old record survives a failed append", fake.0.borrow().data.clone(), vec![1, 0, 0, 0, 7]);
}

#[test]
fn random_failures_never_cost_an_acknowledged_record() {
    let mut rng = anneal_prelude::Rng::new(170101);
    for case in 0..600 {
        let (mut log, fake) = new_log();
        let mut acked: Vec<Vec<u8>> = Vec::new();
        let mut broken = false;
        let mut story = String::new();
        for _ in 0..12 {
            let len = rng.below(6);
            let record: Vec<u8> = rng.vec(len, 0, 255);
            let fault = rng.below(10);
            let keep = rng.below(len + 5);
            {
                let mut d = fake.0.borrow_mut();
                d.plan = Plan::default();
                match fault {
                    0 | 1 => d.plan.short_append = Some(keep),
                    2 => {
                        d.plan.short_append = Some(keep);
                        d.plan.fail_truncate = true;
                    }
                    3 => d.plan.fail_sync = true,
                    _ => {}
                }
            }
            story.push_str(&format!("len {len} fault {fault} keep {keep}; "));
            let got = log.append(&record);
            let want = if broken {
                Err(AppendError::Unusable)
            } else {
                match fault {
                    0 | 1 => Err(AppendError::NotAdded(io::ErrorKind::StorageFull)),
                    2 => Err(AppendError::Broke(io::ErrorKind::StorageFull)),
                    3 => Err(AppendError::Broke(io::ErrorKind::Other)),
                    _ => Ok(()),
                }
            };
            check!(format!("case {case}: {story}"), got, want);
            if got.is_ok() {
                acked.push(record);
            }
            broken |= matches!(got, Err(AppendError::Broke(_)));
            let (found, junk) = records(&fake.0.borrow().data);
            if broken {
                check!(format!("case {case}: acknowledged records come first. {story}"), found.starts_with(&acked), true);
            } else {
                check!(format!("case {case}: exactly the acknowledged records. {story}"), (found, junk), (acked.clone(), false));
            }
        }
    }
}

#[test]
fn a_real_file_appends_at_the_end_after_a_truncate() {
    let dir = TempDir::new("append-truncate");
    let path = dir.join("log");
    let mut device = FileDevice::open(&path).unwrap();
    check!("a missing file is created empty", device.len().unwrap(), 0);
    device.append(b"abcdef").unwrap();
    device.truncate(2).unwrap();
    device.append(b"XY").unwrap();
    check!("abcdef, cut to 2, then XY", std::fs::read(&path).unwrap(), b"abXY".to_vec());
    drop(device);
    let mut device = FileDevice::open(&path).unwrap();
    device.truncate(1).unwrap();
    device.append(b"Z").unwrap();
    check!("reopened, cut to 1, then Z", std::fs::read(&path).unwrap(), b"aZ".to_vec());
}

#[test]
fn a_log_on_a_real_file() {
    let dir = TempDir::new("append-log");
    let path = dir.join("log");
    for round in 0..3u8 {
        let mut log = Log::new(FileDevice::open(&path).unwrap()).unwrap();
        log.append(&[round]).unwrap();
    }
    check!("three openings, one record each", records(&std::fs::read(&path).unwrap()), (vec![vec![0], vec![1], vec![2]], false));
}

#[test]
fn a_file_that_cannot_be_opened_is_an_error() {
    let dir = TempDir::new("append-noopen");
    check!("a directory is not a log file", FileDevice::open(dir.path()).is_err(), true);
    check!("a missing parent", FileDevice::open(&dir.join("no/such/log")).is_err(), true);
}
'''

PROBLEM = dict(
    slug="append-that-fails", title="An append that fails half-way", level="medium", stage="files-that-survive",
    tags=["file io", "partial writes", "error handling", "durability"],
    statement=r'''
A service keeps a log of records in a file. One day the disk filled up in the middle of a write. The call returned an error, the program carried on, and later writes succeeded. After the next restart, every one of those later records was gone.

Make the log safe against a failing disk. The device is behind the trait `LogDevice`, so that failures can be scripted. A failed `append` may have added **any prefix** of the bytes.

**`FileDevice`** is the real device. `open` creates the file if it is missing and keeps what it holds. Every `append` goes to the current end of the file, also after a `truncate`. `sync` makes the data durable.

**`Log::new(device)`** takes a device whose current contents are whole records. It does not write, truncate or sync.

**`Log::append(record)`** writes a 4-byte little-endian length and then the record, with **one** call to the device's `append`, and syncs before it returns `Ok`.

- If the device's `append` fails, put the device back to what it held before this call, trying once. If that works, return `NotAdded` with the append error's kind. The log stays usable.
- If it cannot be put back, return `Broke` with the **append** error's kind.
- If `sync` fails, return `Broke` with the sync error's kind. After a failed sync the operating system may have thrown the unsaved bytes away, and a later sync can still report success. So make **no further call** on the device, not even to undo.
- After `Broke`, every call returns `Unusable` without touching the device.

What must always hold: each record that got `Ok` can be read back, in order, from the front of the device. While the log is usable, nothing else is on the device.
''',
    starter=STARTER, solution=SOLUTION, visible=[VISIBLE], hidden=[HIDDEN], use="use solution::*;",
    hints=[
        ("approach", "Think about what the device holds after each kind of failure. The log needs to remember two things between calls that the device cannot tell it."),
        ("rust", "`OpenOptions::new().create(true).append(true)` puts every write at the current end. `File::set_len` truncates, `File::sync_data` syncs, `metadata()?.len()` gives the length."),
        ("edge", "After a failed append, `device.len()` includes the partial bytes. The length to go back to must have been remembered before the call, and it only grows when a record is acknowledged."),
    ],
    notes=("The log tracks the length made of acknowledged records. A failed write is cut off before anything else can be written behind it. A failed sync is not retried, because success the second time proves nothing about the first bytes.", "O(record) per append", "O(record)"),
    follow_up="1) PostgreSQL changed how it treats a failed fsync in 2018. What did it do before, and why was that unsafe? 2) Appends here sync one by one. How would you let many writers share one sync without acknowledging any of them early? 3) The reader still has to cope with a torn last record after a power cut. Why can this code not prevent that?",
    teaches=["A failed write may have written part of the data. Remove it before anything is written after it.", "Remember the last good length yourself; the file's length is not it after a failure.", "A failed sync cannot be retried away: stop and let recovery sort it out."],
    wrong={
        "leaves_the_partial_record": broken(SOLUTION, ("            if self.device.truncate(self.len).is_err() {\n                self.broken = true;\n                return Err(AppendError::Broke(error.kind()));\n            }\n", "")),
        "carries_on_when_undo_fails": broken(SOLUTION, ("            if self.device.truncate(self.len).is_err() {\n                self.broken = true;\n                return Err(AppendError::Broke(error.kind()));\n            }\n", "            let _ = self.device.truncate(self.len);\n")),
        "retries_a_failed_sync": broken(SOLUTION, ("if let Err(error) = self.device.sync() {", "if let Err(error) = self.device.sync().or_else(|_| self.device.sync()) {")),
        "failed_sync_is_not_fatal": broken(SOLUTION, ("            // a second sync that succeeds would not show that these bytes were kept\n            self.broken = true;\n            return Err(AppendError::Broke(error.kind()));", "            let _ = self.device.truncate(self.len);\n            return Err(AppendError::NotAdded(error.kind()));")),
        "cuts_to_the_length_after_the_failure": broken(SOLUTION, ("if self.device.truncate(self.len).is_err() {", "let now = self.device.len().unwrap_or(0);\n            if self.device.truncate(now).is_err() {")),
        "file_cursor_instead_of_append_mode": broken(SOLUTION, (".create(true).append(true).open(path)?;\n        Ok", ".create(true).write(true).truncate(false).open(path)?;\n        let mut file = file;\n        io::Seek::seek(&mut file, io::SeekFrom::End(0))?;\n        Ok")),
    },
)
