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
