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
fn an_empty_or_missing_log() {
    for disk in [Mem::default(), Mem::with(&[(LOG, "")])] {
        let mut t = open(&disk);
        check!("zero", totals(&t), vec![0, 0, 0]);
        check!("an empty compact log", t.start_rewrite(), Some(Vec::new()));
        t.add("a", 1).unwrap();
        t.finish_rewrite(Vec::new()).unwrap();
        check!("the tail alone", disk.text(LOG), Some("a 1\n".to_string()));
    }
}

#[test]
fn a_whole_line_that_makes_no_sense_is_an_error() {
    for bad in ["a x\n", "a\n", "a 1\nnonsense\nb 2\n", "a 99999999999999999999\n"] {
        let disk = Mem::with(&[(LOG, bad)]);
        let kind = Tally::open(disk.clone()).map(|_| ()).map_err(|e| e.kind());
        check!(format!("{bad:?}"), kind, Err(io::ErrorKind::InvalidData));
        check!(format!("{bad:?}: left alone"), disk.text(LOG), Some(bad.to_string()));
    }
}

#[test]
fn add_syncs_before_it_returns() {
    let disk = Mem::default();
    let mut t = open(&disk);
    t.add("a", 3).unwrap();
    t.add("b", 4).unwrap();
    check!("after a power cut with nothing extra", totals(&open(&disk.cut(0))), vec![3, 4, 0]);
}

#[test]
fn finish_without_a_start() {
    let disk = Mem::with(&[(LOG, "a 1\n")]);
    let mut t = open(&disk);
    check!("refused", t.finish_rewrite(b"a 100\n".to_vec()).map_err(|e| e.kind()), Err(io::ErrorKind::InvalidInput));
    check!("untouched", disk.text(LOG), Some("a 1\n".to_string()));
    let compact = t.start_rewrite().unwrap();
    t.cancel_rewrite();
    check!("refused after cancel", t.finish_rewrite(compact).is_err(), true);
    t.cancel_rewrite();
}

#[test]
fn nothing_from_a_cancelled_rewrite_comes_back() {
    let disk = Mem::default();
    let mut t = open(&disk);
    t.start_rewrite();
    t.add("a", 1).unwrap();
    t.cancel_rewrite();
    let compact = t.start_rewrite().unwrap();
    t.finish_rewrite(compact).unwrap();
    check!("once", totals(&open(&disk)), vec![1, 0, 0]);
}

#[test]
fn a_second_rewrite_after_the_first() {
    let disk = Mem::default();
    let mut t = open(&disk);
    for round in 1..=3 {
        t.add("a", 1).unwrap();
        let compact = t.start_rewrite().unwrap();
        t.add("a", 1).unwrap();
        t.finish_rewrite(compact).unwrap();
        check!(format!("round {round}"), totals(&open(&disk)), vec![2 * round, 0, 0]);
    }
}

#[test]
fn every_failing_call_and_every_power_cut() {
    for failing in 0..8 {
        for power_off in [false, true] {
            for stale in [None, Some("a 500\n"), Some("junk")] {
                let disk = Mem::default();
                if let Some(text) = stale {
                    disk.put(TEMP, text.as_bytes());
                }
                let mut t = open(&disk);
                t.add("a", 1).unwrap();
                t.add("b", 2).unwrap();
                let compact = t.start_rewrite().unwrap();
                t.add("a", 10).unwrap();
                disk.fail_in(failing, power_off);
                let result = t.finish_rewrite(compact);
                let case = format!("call {failing} of finish fails (power off: {power_off}, leftover: {stale:?})");
                if power_off {
                    for extra in [0, 1, 6, usize::MAX] {
                        let kept = disk.cut(extra);
                        check!(format!("{case}, {extra} unsynced bytes kept: nothing lost, nothing twice"), totals(&open(&kept)), vec![11, 2, 0]);
                    }
                } else {
                    disk.heal();
                    if result.is_ok() {
                        check!(format!("{case}: Ok means the new log is safe"), totals(&open(&disk.cut(0))), vec![11, 2, 0]);
                    }
                    check!(format!("{case}: totals"), totals(&t), vec![11, 2, 0]);
                    t.add("c", 3).unwrap();
                    check!(format!("{case}: then an add"), totals(&open(&disk.cut(0))), vec![11, 2, 3]);
                    let compact = t.start_rewrite().unwrap_or_else(|| panic!("{case}: no new rewrite could start"));
                    t.finish_rewrite(compact).unwrap_or_else(|e| panic!("{case}: the new rewrite failed: {e}"));
                    check!(format!("{case}: after another rewrite"), totals(&open(&disk.cut(0))), vec![11, 2, 3]);
                }
            }
        }
    }
}

#[test]
fn random_histories_with_rewrites_match_a_model() {
    let mut rng = anneal_prelude::Rng::new(0x310b);
    for run in 0..300 {
        let disk = Mem::default();
        let mut t = open(&disk);
        let mut model = vec![0i64; 3];
        let mut compact = None;
        let mut history = Vec::new();
        for _ in 0..60 {
            match rng.below(8) {
                0 => {
                    history.push("start".to_string());
                    let got = t.start_rewrite();
                    check!(format!("run {run}: {history:?}"), got.is_some(), compact.is_none());
                    compact = compact.or(got);
                }
                1 => {
                    history.push("finish".to_string());
                    let ready = compact.take();
                    let had = ready.is_some();
                    let result = t.finish_rewrite(ready.unwrap_or_default());
                    check!(format!("run {run}: {history:?}"), result.is_ok(), had);
                }
                2 => {
                    history.push("cancel".to_string());
                    t.cancel_rewrite();
                    compact = None;
                }
                3 => {
                    history.push("power cut and restart".to_string());
                    let kept = disk.cut(0).0.lock().unwrap().files.clone();
                    disk.0.lock().unwrap().files = kept;
                    t = open(&disk);
                    compact = None;
                }
                _ => {
                    let i = rng.below(3);
                    let delta = rng.int(-5, 5);
                    history.push(format!("add {i} {delta}"));
                    model[i] += delta;
                    let name = ["a", "b", "c"][i];
                    check!(format!("run {run}: {history:?}"), t.add(name, delta).ok(), Some(model[i]));
                }
            }
            check!(format!("run {run}: totals after {history:?}"), totals(&t), model.clone());
        }
        check!(format!("run {run}: after a power cut, {history:?}"), totals(&open(&disk.cut(0))), model.clone());
    }
}
