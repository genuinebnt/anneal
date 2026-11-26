use solution::*;

use std::collections::{BTreeMap, BTreeSet};
use std::io;

type Files = BTreeMap<String, Vec<u8>>;

#[derive(Clone, Debug)]
enum Op {
    Create(String),
    Write(String, Vec<u8>),
    SyncFile(String),
    Rename(String, String),
    Remove(String),
    SyncDir,
}

/// Every directory a power cut can leave: synced bytes plus any prefix of the rest, and the name changes since the last
/// directory sync arriving in order, any number of them.
fn after_power_cut(ops: &[Op]) -> BTreeSet<Files> {
    let mut files: Vec<(Vec<u8>, usize)> = Vec::new();
    let mut live: BTreeMap<String, usize> = BTreeMap::new();
    let mut history = vec![live.clone()];
    for op in ops {
        let before = live.clone();
        match op {
            Op::Create(n) => {
                files.push((Vec::new(), 0));
                live.insert(n.clone(), files.len() - 1);
            }
            Op::Write(n, b) => files[live[n]].0.extend_from_slice(b),
            Op::SyncFile(n) => {
                let f = &mut files[live[n]];
                f.1 = f.0.len();
            }
            Op::Rename(a, b) => {
                let f = live.remove(a).unwrap();
                live.insert(b.clone(), f);
            }
            Op::Remove(n) => {
                live.remove(n);
            }
            Op::SyncDir => history = vec![live.clone()],
        }
        if before != live {
            history.push(live.clone());
        }
    }
    let mut out = BTreeSet::new();
    for dir in history {
        let mut states = vec![Files::new()];
        for (name, f) in dir {
            let (data, safe) = &files[f];
            states = states
                .iter()
                .flat_map(|s| {
                    (*safe..=data.len()).map(|k| {
                        let mut s = s.clone();
                        s.insert(name.clone(), data[..k].to_vec());
                        s
                    })
                })
                .collect();
        }
        out.extend(states);
    }
    out
}

/// A disk in memory that records what is done to it and can be told to fail.
#[derive(Default)]
struct Sim {
    files: Files,
    ops: Vec<Op>,
    calls: usize,
    /// This call (counting from 0) fails and does nothing. A failing write keeps half its bytes.
    fail_at: Option<usize>,
}

#[allow(dead_code)]
impl Sim {
    /// A disk that already holds `files`, all of them safe.
    fn holding(files: &Files) -> Sim {
        let mut ops = Vec::new();
        for (name, bytes) in files {
            ops.push(Op::Create(name.clone()));
            ops.push(Op::Write(name.clone(), bytes.clone()));
            ops.push(Op::SyncFile(name.clone()));
        }
        ops.push(Op::SyncDir);
        Sim { files: files.clone(), ops, calls: 0, fail_at: None }
    }
    fn step(&mut self) -> io::Result<()> {
        let n = self.calls;
        self.calls += 1;
        if self.fail_at == Some(n) {
            return Err(io::Error::from(io::ErrorKind::Other));
        }
        Ok(())
    }
    fn exists(&self, name: &str) -> io::Result<()> {
        if self.files.contains_key(name) { Ok(()) } else { Err(io::Error::from(io::ErrorKind::NotFound)) }
    }
}

impl Disk for Sim {
    fn create(&mut self, name: &str) -> io::Result<()> {
        self.step()?;
        self.files.insert(name.to_string(), Vec::new());
        self.ops.push(Op::Create(name.to_string()));
        Ok(())
    }
    fn write(&mut self, name: &str, bytes: &[u8]) -> io::Result<()> {
        let failed = self.step();
        self.exists(name)?;
        let kept = if failed.is_err() { &bytes[..bytes.len() / 2] } else { bytes };
        self.files.get_mut(name).unwrap().extend_from_slice(kept);
        self.ops.push(Op::Write(name.to_string(), kept.to_vec()));
        failed
    }
    fn sync_file(&mut self, name: &str) -> io::Result<()> {
        self.step()?;
        self.exists(name)?;
        self.ops.push(Op::SyncFile(name.to_string()));
        Ok(())
    }
    fn rename(&mut self, from: &str, to: &str) -> io::Result<()> {
        self.step()?;
        self.exists(from)?;
        let bytes = self.files.remove(from).unwrap();
        self.files.insert(to.to_string(), bytes);
        self.ops.push(Op::Rename(from.to_string(), to.to_string()));
        Ok(())
    }
    fn remove(&mut self, name: &str) -> io::Result<()> {
        self.step()?;
        self.exists(name)?;
        self.files.remove(name);
        self.ops.push(Op::Remove(name.to_string()));
        Ok(())
    }
    fn sync_dir(&mut self) -> io::Result<()> {
        self.step()?;
        self.ops.push(Op::SyncDir);
        Ok(())
    }
    fn list(&mut self) -> io::Result<Vec<String>> {
        self.step()?;
        // not in order, as a real directory
        Ok(self.files.keys().rev().cloned().collect())
    }
    fn read(&mut self, name: &str) -> io::Result<Vec<u8>> {
        self.step()?;
        self.exists(name)?;
        Ok(self.files[name].clone())
    }
}

#[allow(dead_code)]
fn files(list: &[(&str, &[u8])]) -> Files {
    list.iter().map(|(n, b)| (n.to_string(), b.to_vec())).collect()
}

/// Cuts the power at every moment of `ops` from `from` on. Every disk left behind must recover to one of `allowed`, and
/// what was recovered must survive another power cut.
#[allow(dead_code)]
fn every_cut(story: &str, ops: &[Op], from: usize, allowed: &[Option<(u64, Vec<u8>)>]) {
    for cut in from..=ops.len() {
        for left in after_power_cut(&ops[..cut]) {
            let mut disk = Sim::holding(&left);
            let got = recover(&mut disk).map_err(|e| e.kind());
            let ok = matches!(&got, Ok(found) if allowed.contains(found));
            check!(format!("{story}; power cut after {:?}; disk then {left:?}; recover gave {got:?}", &ops[..cut]), ok, true);
            for again in after_power_cut(&disk.ops) {
                let twice = recover(&mut Sim::holding(&again)).map_err(|e| e.kind());
                check!(format!("{story}; disk {left:?}, recovered, then a second cut leaving {again:?}"), twice, got.clone());
            }
        }
    }
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
fn a_power_cut_during_the_first_save() {
    let mut disk = Sim::default();
    save(&mut disk, b"abc").unwrap();
    every_cut("the first save", &disk.ops, 0, &[None, Some((1, b"abc".to_vec()))]);
}

#[test]
fn one_failing_call_at_every_place() {
    for fail in 0..14 {
        let mut disk = Sim::default();
        save(&mut disk, b"v1").unwrap();
        let before = disk.ops.len();
        disk.calls = 0;
        disk.fail_at = Some(fail);
        let saved = save(&mut disk, b"v2");
        let calls = disk.calls;
        disk.fail_at = None;
        let old = Some((1, b"v1".to_vec()));
        let new = Some((2, b"v2".to_vec()));
        let allowed = if saved.is_ok() { vec![new.clone()] } else { vec![old, new] };
        let story = format!("call {fail} of the second save fails (it made {calls} calls and returned {saved:?})");
        every_cut(&story, &disk.ops, if saved.is_ok() { disk.ops.len() } else { before }, &allowed);
        // the program restarts without a power cut: whatever it is given must then survive one
        let mut restarted = Sim { files: disk.files.clone(), ops: disk.ops.clone(), calls: 0, fail_at: None };
        let given = recover(&mut restarted).unwrap();
        check!(format!("{story}; restart gives {given:?}"), allowed.contains(&given), true);
        for left in after_power_cut(&restarted.ops) {
            check!(format!("{story}; restart gave {given:?}, then a power cut left {left:?}"), recover(&mut Sim::holding(&left)).unwrap(), given.clone());
        }
        // the same disk keeps working
        let third = save(&mut disk, b"v3").unwrap();
        check!(format!("{story}; then v3"), recover(&mut disk).unwrap(), Some((third, b"v3".to_vec())));
        for left in after_power_cut(&disk.ops) {
            check!(format!("{story}; then v3; cut leaving {left:?}"), recover(&mut Sim::holding(&left)).unwrap(), Some((third, b"v3".to_vec())));
        }
    }
}

#[test]
fn a_failure_while_tidying_is_not_a_failed_save() {
    let mut disk = Sim::holding(&files(&[("state-4", b"d")]));
    // find the call that removes the old version, and make it fail
    let mut probe = Sim::holding(&files(&[("state-4", b"d")]));
    save(&mut probe, b"e").unwrap();
    for fail in 0..probe.calls {
        let mut trial = Sim::holding(&files(&[("state-4", b"d")]));
        trial.fail_at = Some(fail);
        let got = save(&mut trial, b"e");
        if trial.files.contains_key("state-4") && trial.files.contains_key("state-5") {
            let safe = after_power_cut(&trial.ops).iter().all(|f| f.get("state-5") == Some(&b"e".to_vec()));
            if safe {
                check!(format!("call {fail} failed after the new version was safe"), got.map_err(|e| e.kind()), Ok(5));
            }
        }
    }
    check!("without failures", save(&mut disk, b"e").unwrap(), 5);
}

#[test]
fn ten_comes_after_nine() {
    let mut disk = Sim::holding(&files(&[("state-9", b"nine"), ("state-10", b"ten")]));
    check!("state-9 and state-10", recover(&mut disk).unwrap(), Some((10, b"ten".to_vec())));
    check!("the next number", save(&mut disk, b"x").unwrap(), 11);
    check!("old versions go", disk.files.keys().cloned().collect::<Vec<_>>(), vec!["state-11".to_string()]);
}

#[test]
fn leftovers_are_removed_and_other_files_are_not() {
    let others: &[(&str, &[u8])] = &[("notes.tmp", b"1"), ("state-07", b"2"), ("state-+8", b"3"), ("state-9.bak", b"4"), ("state-", b"5"), ("old-state-3", b"6"), ("state-x.tmp", b"7")];
    let mut all = files(others);
    all.insert("state-2".into(), b"real".to_vec());
    all.insert("state-3.tmp".into(), b"half".to_vec());
    all.insert("state-12.tmp".into(), b"".to_vec());
    let mut disk = Sim::holding(&all);
    check!("one version among look-alikes", recover(&mut disk).unwrap(), Some((2, b"real".to_vec())));
    let mut want = files(others);
    want.insert("state-2".into(), b"real".to_vec());
    check!("after recover", disk.files.clone(), want);
    check!("next", save(&mut disk, b"n").unwrap(), 3);
    check!("others survive a save", others.iter().all(|(n, b)| disk.files.get(*n) == Some(&b.to_vec())), true);
}

#[test]
fn recover_errors_are_reported() {
    for fail in 0..4 {
        let mut disk = Sim::holding(&files(&[("state-1", b"a"), ("state-2.tmp", b"b")]));
        disk.fail_at = Some(fail);
        let got = recover(&mut disk);
        let fine = match got {
            Err(_) => true,
            Ok(found) => found == Some((1, b"a".to_vec())) && disk.calls <= fail,
        };
        check!(format!("call {fail} of recover fails: an error, not a wrong answer"), fine, true);
    }
}

#[test]
fn random_runs_with_failures_and_cuts() {
    let mut rng = anneal_prelude::Rng::new(170301);
    for case in 0..60 {
        let mut disk = Sim::default();
        // what a restart may find: the last acknowledged state, or anything tried since
        let mut allowed: Vec<Option<(u64, Vec<u8>)>> = vec![None];
        let mut story = format!("case {case}:");
        for round in 0..6 {
            let len = rng.below(3);
            let bytes: Vec<u8> = rng.vec(len, 0, 9);
            disk.calls = 0;
            disk.fail_at = if rng.bool() { Some(rng.below(10)) } else { None };
            let before = disk.ops.len();
            let highest = disk.files.keys().filter_map(|n| n.strip_prefix("state-")?.parse::<u64>().ok()).max().unwrap_or(0);
            let saved = save(&mut disk, &bytes);
            story.push_str(&format!(" save {round} {bytes:?} fail {:?} -> {saved:?};", disk.fail_at));
            disk.fail_at = None;
            let new = Some((highest + 1, bytes));
            let mut during = allowed.clone();
            during.push(new.clone());
            if let Ok(n) = saved {
                check!(story.clone(), n, highest + 1);
                allowed = vec![new];
                every_cut(&story, &disk.ops, disk.ops.len(), &allowed);
            } else {
                every_cut(&story, &disk.ops, before, &during);
                allowed = during;
            }
        }
    }
}

#[test]
fn the_real_disk_does_what_it_says() {
    let dir = TempDir::new("save-real");
    let mut disk = DirDisk::open(dir.join("a/b")).unwrap();
    disk.create("f").unwrap();
    disk.write("f", b"ab").unwrap();
    disk.write("f", b"cd").unwrap();
    disk.sync_file("f").unwrap();
    check!("two writes add up", disk.read("f").unwrap(), b"abcd".to_vec());
    disk.create("f").unwrap();
    check!("create empties", disk.read("f").unwrap(), Vec::<u8>::new());
    disk.create("g").unwrap();
    disk.write("g", b"g").unwrap();
    disk.rename("g", "f").unwrap();
    disk.sync_dir().unwrap();
    std::fs::create_dir(dir.join("a/b/sub")).unwrap();
    let mut names = disk.list().unwrap();
    names.sort();
    check!("files only", names, vec!["f".to_string()]);
    check!("writing a missing file is an error", disk.write("nope", b"x").is_err(), true);
    check!("syncing a missing file is an error", disk.sync_file("nope").is_err(), true);
    disk.remove("f").unwrap();
    check!("removed", disk.read("f").map_err(|e| e.kind()), Err(io::ErrorKind::NotFound));
}
