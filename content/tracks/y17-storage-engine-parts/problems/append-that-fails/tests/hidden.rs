use solution::*;

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
