use solution::*;

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
