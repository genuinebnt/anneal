use solution::*;


use std::collections::BTreeMap;
use std::io;
use std::sync::{Arc, Mutex};

#[allow(dead_code)]
#[derive(Default, Clone)]
struct File {
    data: Vec<u8>,
    synced: usize,
}

#[allow(dead_code)]
#[derive(Default)]
struct State {
    files: BTreeMap<String, File>,
    calls: usize,
    /// This call (counting changing calls from 0) fails; a failing append keeps half.
    fail_at: Option<usize>,
    /// After the failing call, every later one fails too: the power is gone.
    power_off: bool,
}

#[allow(dead_code)]
#[derive(Clone, Default)]
struct Mem(Arc<Mutex<State>>);

#[allow(dead_code)]
impl Mem {
    fn with(files: &[(&str, &str)]) -> Mem {
        let disk = Mem::default();
        for (name, data) in files {
            disk.put(name, data.as_bytes());
        }
        disk
    }
    fn put(&self, name: &str, data: &[u8]) {
        let file = File { data: data.to_vec(), synced: data.len() };
        self.0.lock().unwrap().files.insert(name.to_string(), file);
    }
    fn text(&self, name: &str) -> Option<String> {
        self.0.lock().unwrap().files.get(name).map(|f| String::from_utf8_lossy(&f.data).into_owned())
    }
    fn names(&self) -> Vec<String> {
        self.0.lock().unwrap().files.keys().cloned().collect()
    }
    fn fail_in(&self, n: usize, power_off: bool) {
        let mut s = self.0.lock().unwrap();
        s.fail_at = Some(s.calls + n);
        s.power_off = power_off;
    }
    fn heal(&self) {
        let mut s = self.0.lock().unwrap();
        s.fail_at = None;
        s.power_off = false;
    }
    /// What a power cut leaves: each file's synced bytes and up to `extra` more.
    fn cut(&self, extra: usize) -> Mem {
        let s = self.0.lock().unwrap();
        let disk = Mem::default();
        for (name, f) in &s.files {
            let len = f.data.len().min(f.synced.saturating_add(extra));
            disk.put(name, &f.data[..len]);
        }
        disk
    }
    /// `Some(true)` for the failing call itself, `Some(false)` for calls after the power went.
    fn step(s: &mut State) -> Option<bool> {
        let n = s.calls;
        s.calls += 1;
        match s.fail_at {
            Some(at) if n == at => Some(true),
            Some(at) if s.power_off && n > at => Some(false),
            _ => None,
        }
    }
}

#[allow(dead_code)]
fn full() -> io::Error {
    io::Error::from(io::ErrorKind::StorageFull)
}

impl Disk for Mem {
    fn read(&mut self, name: &str) -> io::Result<Option<Vec<u8>>> {
        let s = self.0.lock().unwrap();
        Ok(s.files.get(name).map(|f| f.data.clone()))
    }
    fn append(&mut self, name: &str, bytes: &[u8]) -> io::Result<()> {
        let mut s = self.0.lock().unwrap();
        let failure = Mem::step(&mut s);
        let keep = match failure {
            None => bytes.len(),
            Some(true) => bytes.len() / 2,
            Some(false) => return Err(full()),
        };
        s.files.entry(name.into()).or_default().data.extend_from_slice(&bytes[..keep]);
        if failure.is_some() { Err(full()) } else { Ok(()) }
    }
    fn truncate(&mut self, name: &str, len: u64) -> io::Result<()> {
        let mut s = self.0.lock().unwrap();
        if Mem::step(&mut s).is_some() {
            return Err(full());
        }
        let f = s.files.get_mut(name).ok_or_else(|| io::Error::from(io::ErrorKind::NotFound))?;
        f.data.truncate(len as usize);
        f.synced = f.synced.min(f.data.len());
        Ok(())
    }
    fn sync(&mut self, name: &str) -> io::Result<()> {
        let mut s = self.0.lock().unwrap();
        if Mem::step(&mut s).is_some() {
            return Err(full());
        }
        let f = s.files.get_mut(name).ok_or_else(|| io::Error::from(io::ErrorKind::NotFound))?;
        f.synced = f.data.len();
        Ok(())
    }
    fn rename(&mut self, from: &str, to: &str) -> io::Result<()> {
        let mut s = self.0.lock().unwrap();
        if Mem::step(&mut s).is_some() {
            return Err(full());
        }
        let f = s.files.remove(from).ok_or_else(|| io::Error::from(io::ErrorKind::NotFound))?;
        s.files.insert(to.into(), f);
        Ok(())
    }
    fn remove(&mut self, name: &str) -> io::Result<()> {
        let mut s = self.0.lock().unwrap();
        if Mem::step(&mut s).is_some() {
            return Err(full());
        }
        s.files.remove(name);
        Ok(())
    }
}

#[allow(dead_code)]
fn open(disk: &Mem) -> Tally<Mem> {
    match Tally::open(disk.clone()) {
        Ok(t) => t,
        Err(e) => panic!("open failed: {e}, log {:?}", disk.text(LOG)),
    }
}
#[allow(dead_code)]
fn totals(t: &Tally<Mem>) -> Vec<i64> {
    ["a", "b", "c"].iter().map(|n| t.get(n)).collect()
}

#[test]
fn adding_and_reopening() {
    let disk = Mem::default();
    let mut t = open(&disk);
    check!("a", t.add("a", 5).unwrap(), 5);
    check!("a again", t.add("a", -2).unwrap(), 3);
    check!("b", t.add("b", 1).unwrap(), 1);
    check!("the log", disk.text(LOG), Some("a 5\na -2\nb 1\n".to_string()));
    check!("reopened", totals(&open(&disk)), vec![3, 1, 0]);
}

#[test]
fn an_unfinished_line_is_cut_off() {
    let disk = Mem::with(&[(LOG, "a 5\nb 12")]);
    let mut t = open(&disk);
    check!("totals", totals(&t), vec![5, 0, 0]);
    t.add("b", 1).unwrap();
    check!("the log stays well formed", disk.text(LOG), Some("a 5\nb 1\n".to_string()));
}

#[test]
fn the_compact_log() {
    let mut t = open(&Mem::default());
    for (name, d) in [("b", 2), ("a", 1), ("b", 3), ("c", 4), ("c", -4)] {
        t.add(name, d).unwrap();
    }
    check!("one line per name added to, in name order", t.start_rewrite(), Some(b"a 1\nb 5\nc 0\n".to_vec()));
    check!("one at a time", t.start_rewrite(), None);
}

#[test]
fn writes_during_a_rewrite_are_kept_once() {
    let disk = Mem::default();
    let mut t = open(&disk);
    for _ in 0..10 {
        t.add("a", 1).unwrap();
    }
    let compact = t.start_rewrite().unwrap();
    t.add("a", 1).unwrap();
    t.add("b", 7).unwrap();
    t.finish_rewrite(compact).unwrap();
    check!("the log", disk.text(LOG), Some("a 10\na 1\nb 7\n".to_string()));
    check!("no temporary file is left", disk.names(), vec![LOG.to_string()]);
    t.add("a", 1).unwrap();
    check!("reopened", totals(&open(&disk)), vec![12, 7, 0]);
}

#[test]
fn cancelling() {
    let disk = Mem::default();
    let mut t = open(&disk);
    t.add("a", 1).unwrap();
    t.start_rewrite();
    t.add("a", 1).unwrap();
    t.cancel_rewrite();
    check!("the log is untouched", disk.text(LOG), Some("a 1\na 1\n".to_string()));
    check!("another may start", t.start_rewrite(), Some(b"a 2\n".to_vec()));
}
