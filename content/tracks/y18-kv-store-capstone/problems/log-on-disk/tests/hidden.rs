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
