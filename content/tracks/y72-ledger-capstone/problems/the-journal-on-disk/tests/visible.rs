use solution::*;

use std::cell::RefCell;
use std::io;
use std::path::PathBuf;
use std::rc::Rc;

/// This checksum is written here too, so the tests do not trust the code under test for it.
#[allow(dead_code)]
fn crc(data: &[u8]) -> u32 {
    let mut c = !0u32;
    for &b in data {
        c ^= u32::from(b);
        for _ in 0..8 {
            c = if c & 1 == 1 { (c >> 1) ^ 0xEDB8_8320 } else { c >> 1 };
        }
    }
    !c
}

/// A frame built from the format in the statement, around any payload.
#[allow(dead_code)]
fn wrap(payload: &[u8]) -> Vec<u8> {
    let len = (payload.len() as u32).to_le_bytes();
    let mut out = len.to_vec();
    out.extend(crc(&len).to_le_bytes());
    out.extend(crc(payload).to_le_bytes());
    out.extend(payload);
    out
}

#[allow(dead_code)]
fn new_event(n: u64) -> NewEvent {
    NewEvent::current(CmdId::Client(n), 0, n, &Event::Deposited { amount: n as i64 })
}

/// A file in memory. Its next append can stop part way, and its next sync can fail.
#[allow(dead_code)]
#[derive(Default)]
struct Disk {
    bytes: Vec<u8>,
    short_write: Option<usize>,
    fail_sync: bool,
    calls: Vec<&'static str>,
}

#[allow(dead_code)]
#[derive(Clone, Default)]
struct Fake(Rc<RefCell<Disk>>);

impl Device for Fake {
    fn append(&mut self, bytes: &[u8]) -> io::Result<()> {
        let mut d = self.0.borrow_mut();
        d.calls.push("append");
        if let Some(keep) = d.short_write.take() {
            let keep = keep.min(bytes.len());
            d.bytes.extend_from_slice(&bytes[..keep]);
            return Err(io::Error::from(io::ErrorKind::StorageFull));
        }
        d.bytes.extend_from_slice(bytes);
        Ok(())
    }
    fn sync(&mut self) -> io::Result<()> {
        let mut d = self.0.borrow_mut();
        d.calls.push("sync");
        if std::mem::take(&mut d.fail_sync) {
            return Err(io::Error::other("sync"));
        }
        Ok(())
    }
}

/// A log of batches of these sizes, and where each frame ends.
#[allow(dead_code)]
fn log_of(sizes: &[usize]) -> (Vec<u8>, Vec<usize>, Vec<Stored>) {
    let disk = Fake::default();
    let mut store = FileStore::over(MemStore::new(), disk.clone());
    let mut ends = Vec::new();
    let mut n = 0;
    for (i, &size) in sizes.iter().enumerate() {
        let stream = if i % 2 == 0 { "ann" } else { "bo" };
        let batch = (0..size).map(|_| {
            n += 1;
            new_event(n)
        }).collect();
        let at = store.version(stream);
        store.append(stream, at, batch).unwrap();
        ends.push(disk.0.borrow().bytes.len());
    }
    let bytes = disk.0.borrow().bytes.clone();
    (bytes, ends, store.read_all(0).to_vec())
}

#[allow(dead_code)]
fn lines(r: &Recovered) -> Vec<Stored> {
    r.store.read_all(0).to_vec()
}

#[allow(dead_code)]
fn failure(bytes: &[u8]) -> Option<LoadError> {
    recover(bytes).err()
}

/// A path in a directory that only this test uses.
#[allow(dead_code)]
struct Scratch(PathBuf);

#[allow(dead_code)]
impl Scratch {
    fn new(name: &str) -> Scratch {
        let dir = std::env::temp_dir().join(format!("anneal-ledger-{}-{name}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        Scratch(dir)
    }
    fn path(&self) -> PathBuf {
        self.0.join("journal")
    }
}

impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

#[test]
fn a_frame_has_the_stated_layout() {
    let (bytes, _, stored) = log_of(&[2]);
    check!("frame", frame(&stored), bytes.clone());
    let payload = &bytes[HEADER..];
    check!("layout", wrap(payload), bytes.clone());
    check!("payload is the batch as JSON", serde_json::from_slice::<Vec<Stored>>(payload).unwrap(), stored);
}

#[test]
fn append_writes_once_and_then_syncs() {
    let disk = Fake::default();
    let mut store = FileStore::over(MemStore::new(), disk.clone());
    store.append("ann", 0, vec![new_event(1), new_event(2)]).unwrap();
    check!("calls", disk.0.borrow().calls.clone(), vec!["append", "sync"]);
    check!("lines", store.read_all(0).len(), 2);
}

#[test]
fn what_was_written_comes_back() {
    let (bytes, _, stored) = log_of(&[1, 3, 2]);
    let r = recover(&bytes).unwrap();
    check!("lines", lines(&r), stored);
    check!("keep, tail", (r.keep, r.tail), (bytes.len(), Tail::Clean));
}

#[test]
fn a_torn_write_loses_its_whole_batch() {
    let (bytes, ends, stored) = log_of(&[1, 3]);
    let r = recover(&bytes[..bytes.len() - 1]).unwrap();
    check!("only the first batch", lines(&r), stored[..1].to_vec());
    check!("keep, tail", (r.keep, r.tail), (ends[0], Tail::Torn { at: ends[0] }));
}

#[test]
fn damage_in_the_middle_is_refused() {
    let (mut bytes, _, _) = log_of(&[1, 1, 1]);
    bytes[HEADER + 3] ^= 1;
    check!("first batch damaged", failure(&bytes), Some(LoadError::Damaged { at: 0 }));
}

#[test]
fn after_a_failed_write_the_store_takes_no_more() {
    let disk = Fake::default();
    let mut store = FileStore::over(MemStore::new(), disk.clone());
    store.append("ann", 0, vec![new_event(1)]).unwrap();
    disk.0.borrow_mut().short_write = Some(5);
    check!("io", matches!(store.append("ann", 1, vec![new_event(2)]), Err(StoreError::Io(_))), true);
    disk.0.borrow_mut().calls.clear();
    check!("poisoned", matches!(store.append("ann", 1, vec![new_event(3)]), Err(StoreError::Poisoned)), true);
    check!("no calls", disk.0.borrow().calls.len(), 0);
    check!("reads still work", store.read_all(0).len(), 1);
}

#[test]
fn a_real_file() {
    let dir = Scratch::new("visible");
    let (mut store, tail) = FileStore::open(&dir.path()).unwrap();
    check!("new", tail, Tail::Clean);
    store.append("ann", 0, vec![new_event(1)]).unwrap();
    drop(store);
    let (store, tail) = FileStore::open(&dir.path()).unwrap();
    check!("reopened", (tail, store.read_all(0).len()), (Tail::Clean, 1));
}
