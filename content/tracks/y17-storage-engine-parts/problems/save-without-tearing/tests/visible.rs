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
fn versions_count_up() {
    let mut disk = Sim::default();
    check!("an empty disk", recover(&mut disk).unwrap(), None);
    let numbers = (save(&mut disk, b"one").unwrap(), save(&mut disk, b"two").unwrap());
    check!("two saves", numbers, (1, 2));
    check!("recover", recover(&mut disk).unwrap(), Some((2, b"two".to_vec())));
    check!("only the newest is kept", disk.files.clone(), files(&[("state-2", b"two")]));
}

#[test]
fn a_power_cut_during_the_second_save() {
    let mut disk = Sim::default();
    save(&mut disk, b"old").unwrap();
    let before = disk.ops.len();
    save(&mut disk, b"new").unwrap();
    every_cut("old saved, saving new", &disk.ops, before, &[Some((1, b"old".to_vec())), Some((2, b"new".to_vec()))]);
}

#[test]
fn after_ok_the_new_version_is_safe() {
    let mut disk = Sim::default();
    save(&mut disk, b"old").unwrap();
    save(&mut disk, b"new").unwrap();
    for left in after_power_cut(&disk.ops) {
        check!(format!("{left:?}"), recover(&mut Sim::holding(&left)).unwrap(), Some((2, b"new".to_vec())));
    }
}

#[test]
fn an_unfinished_save_is_not_a_version() {
    let mut disk = Sim::holding(&files(&[("state-1", b"good"), ("state-2.tmp", b"ha")]));
    check!("state-1 and a leftover", recover(&mut disk).unwrap(), Some((1, b"good".to_vec())));
    check!("the leftover is removed", disk.files.clone(), files(&[("state-1", b"good")]));
}

#[test]
fn a_real_directory() {
    let dir = TempDir::new("save-visible");
    let mut disk = DirDisk::open(dir.join("settings")).unwrap();
    save(&mut disk, b"a").unwrap();
    save(&mut disk, b"b").unwrap();
    let mut disk = DirDisk::open(dir.join("settings")).unwrap();
    check!("saved twice, reopened", recover(&mut disk).unwrap(), Some((2, b"b".to_vec())));
}
