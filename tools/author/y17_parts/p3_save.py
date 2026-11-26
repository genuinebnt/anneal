from y17_parts.common import broken, TEMP_DIR

TRAIT = r'''use std::io;
use std::path::PathBuf;

/// The files of one directory.
pub trait Disk {
    /// A new, empty file under this name, replacing any that had it.
    fn create(&mut self, name: &str) -> io::Result<()>;
    /// Adds bytes at the end of a file that exists. On error, any prefix of them may have been added.
    fn write(&mut self, name: &str, bytes: &[u8]) -> io::Result<()>;
    /// Makes the file's bytes safe from a power cut (not its name).
    fn sync_file(&mut self, name: &str) -> io::Result<()>;
    /// Gives `from`'s file the name `to`, replacing any file that had it.
    fn rename(&mut self, from: &str, to: &str) -> io::Result<()>;
    fn remove(&mut self, name: &str) -> io::Result<()>;
    /// Makes the directory's names, as they are now, safe from a power cut (not any bytes).
    fn sync_dir(&mut self) -> io::Result<()>;
    /// The names of the files in the directory, in any order.
    fn list(&mut self) -> io::Result<Vec<String>>;
    fn read(&mut self, name: &str) -> io::Result<Vec<u8>>;
}

/// A real directory.
pub struct DirDisk {
    dir: PathBuf,
}
'''

STARTER = TRAIT + r'''
impl DirDisk {
    /// Uses `dir`, creating it and its parents if they are missing.
    pub fn open(dir: PathBuf) -> io::Result<DirDisk> {
        todo!()
    }
}

impl Disk for DirDisk {
    fn create(&mut self, name: &str) -> io::Result<()> {
        todo!()
    }
    fn write(&mut self, name: &str, bytes: &[u8]) -> io::Result<()> {
        todo!()
    }
    fn sync_file(&mut self, name: &str) -> io::Result<()> {
        todo!()
    }
    fn rename(&mut self, from: &str, to: &str) -> io::Result<()> {
        todo!()
    }
    fn remove(&mut self, name: &str) -> io::Result<()> {
        todo!()
    }
    fn sync_dir(&mut self) -> io::Result<()> {
        todo!()
    }
    fn list(&mut self) -> io::Result<Vec<String>> {
        todo!()
    }
    fn read(&mut self, name: &str) -> io::Result<Vec<u8>> {
        todo!()
    }
}

/// Stores a new version of the state and returns its number.
pub fn save<D: Disk>(disk: &mut D, bytes: &[u8]) -> io::Result<u64> {
    todo!()
}

/// What a program that has just started should use: the newest version and its number.
pub fn recover<D: Disk>(disk: &mut D) -> io::Result<Option<(u64, Vec<u8>)>> {
    todo!()
}
'''

SOLUTION = TRAIT.replace("use std::io;\nuse std::path::PathBuf;", "use std::fs::{self, File, OpenOptions};\nuse std::io::{self, Write};\nuse std::path::PathBuf;") + r'''
impl DirDisk {
    /// Uses `dir`, creating it and its parents if they are missing.
    pub fn open(dir: PathBuf) -> io::Result<DirDisk> {
        fs::create_dir_all(&dir)?;
        Ok(DirDisk { dir })
    }
}

impl Disk for DirDisk {
    fn create(&mut self, name: &str) -> io::Result<()> {
        File::create(self.dir.join(name))?;
        Ok(())
    }
    fn write(&mut self, name: &str, bytes: &[u8]) -> io::Result<()> {
        let mut file = OpenOptions::new().append(true).open(self.dir.join(name))?;
        file.write_all(bytes)
    }
    fn sync_file(&mut self, name: &str) -> io::Result<()> {
        File::open(self.dir.join(name))?.sync_all()
    }
    fn rename(&mut self, from: &str, to: &str) -> io::Result<()> {
        fs::rename(self.dir.join(from), self.dir.join(to))
    }
    fn remove(&mut self, name: &str) -> io::Result<()> {
        fs::remove_file(self.dir.join(name))
    }
    fn sync_dir(&mut self) -> io::Result<()> {
        // on Unix a directory can be opened and synced like a file
        File::open(&self.dir)?.sync_all()
    }
    fn list(&mut self) -> io::Result<Vec<String>> {
        let mut names = Vec::new();
        for entry in fs::read_dir(&self.dir)? {
            let entry = entry?;
            if entry.file_type()?.is_file() {
                if let Ok(name) = entry.file_name().into_string() {
                    names.push(name);
                }
            }
        }
        Ok(names)
    }
    fn read(&mut self, name: &str) -> io::Result<Vec<u8>> {
        fs::read(self.dir.join(name))
    }
}

/// The number in `state-<number>`, if the whole name has that form.
fn generation(name: &str) -> Option<u64> {
    let digits = name.strip_prefix("state-")?;
    if digits.starts_with('0') || !digits.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    digits.parse().ok()
}

fn newest<D: Disk>(disk: &mut D) -> io::Result<Option<u64>> {
    Ok(disk.list()?.iter().filter_map(|n| generation(n)).max())
}

/// Stores a new version of the state and returns its number.
pub fn save<D: Disk>(disk: &mut D, bytes: &[u8]) -> io::Result<u64> {
    let next = newest(disk)?.unwrap_or(0) + 1;
    let name = format!("state-{next}");
    let tmp = format!("{name}.tmp");
    disk.create(&tmp)?;
    disk.write(&tmp, bytes)?;
    disk.sync_file(&tmp)?; // the bytes must be safe before the name can be
    disk.rename(&tmp, &name)?;
    disk.sync_dir()?; // only now is the new version sure to be found
    // the save has succeeded: tidying up may fail without taking that back
    if let Ok(names) = disk.list() {
        for old in names {
            if generation(&old).is_some_and(|g| g < next) {
                let _ = disk.remove(&old);
            }
        }
    }
    Ok(next)
}

/// What a program that has just started should use: the newest version and its number.
pub fn recover<D: Disk>(disk: &mut D) -> io::Result<Option<(u64, Vec<u8>)>> {
    for name in disk.list()? {
        if name.strip_suffix(".tmp").and_then(generation).is_some() {
            disk.remove(&name)?;
        }
    }
    let Some(found) = newest(disk)? else {
        return Ok(None);
    };
    // the name may so far exist only in memory; make it safe before anyone acts on it
    disk.sync_dir()?;
    let bytes = disk.read(&format!("state-{found}"))?;
    Ok(Some((found, bytes)))
}
'''

HARNESS = r'''
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
'''

VISIBLE = HARNESS + TEMP_DIR + r'''
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
'''

HIDDEN = HARNESS + TEMP_DIR + r'''
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
'''

PROBLEM = dict(
    slug="save-without-tearing", title="Save a file that a crash cannot tear", level="hard", stage="files-that-survive",
    tags=["atomic replace", "rename", "fsync", "directory sync", "crash recovery", "std::fs"],
    statement=r'''
A program keeps its whole state in one blob and saves it often. It overwrites the file in place. Last week the machine lost power and the program started with an empty state.

Write the two functions it needs, against a directory given by the trait `Disk`. The tests cut the power after every call, and check every disk that could be left behind. What a power cut can leave is the model of *What is on the disk after a power cut*: bytes are safe after `sync_file`, names after `sync_dir`, name changes arrive in order, and unsynced bytes may be cut anywhere.

**Files.** Version `n` is stored under the name `state-n`, with `n` written as a plain decimal number from 1 up. While it is being made, it may only be called `state-n.tmp`. The directory may hold other files. A name that is not exactly of these two forms (`state-07` is not) is somebody else's: leave it alone.

**`save(disk, bytes)`** stores `bytes` as the version after the highest `state-n` present, and returns its number.

- If it returns `Ok(n)`, no later power cut may lose version `n`.
- If the power fails during it, or any call fails, a restart finds the earlier state or the new one, **whole**.
- Once the new version is safe, older versions are removed.
- From the moment it is safe, the save has succeeded: a later failure, as while removing, does **not** make it fail.
- Any failure before that is returned as an error.

**`recover(disk)`** returns the highest version and its bytes, or `None`. It removes leftover `state-n.tmp` files. What it returns must be safe: a later power cut cannot make a restart return something older. If any call it makes fails, it returns that error.

**`DirDisk`** is the real directory. Implement it with the standard library. `open` creates the directory and its parents. Writing to or syncing a file that does not exist is an error. `list` returns files only, not folders.

The tests of this problem contain a working version of that power-cut model. Solve that problem first if you have not.
''',
    starter=STARTER, solution=SOLUTION, visible=[VISIBLE], hidden=[HIDDEN], use="use solution::*;",
    hints=[
        ("approach", "For each pair of neighbouring steps, ask what the disk can look like if the power fails between them, using the four rules. Then ask the same about a restart that happens without a power cut, after a save that failed late."),
        ("rust", "`File::create` empties or creates. `OpenOptions::new().append(true)` writes at the end of an existing file. `File::open(dir)?.sync_all()` syncs a directory on Unix. `entry.file_type()?.is_file()` and `file_name().into_string()` help with `read_dir`."),
        ("edge", "`state-10` is newer than `state-9`, though it sorts first as text. After a failed directory sync, a new name is visible to the running program but may still vanish: what should the next `recover` do before it hands that version out?"),
    ],
    notes=("The bytes are synced under a temporary name, so the real name can never point at half a file. The directory sync is the moment the save becomes true, and only after it may older versions go. `recover` syncs the directory too, because the name it reads may so far exist only in memory.", "O(files + bytes)", "O(bytes)"),
    follow_up="1) This keeps a whole copy per save. At what size would you rather append changes to a log, and what would you still need this function for? 2) What can two processes calling `save` on one directory do to each other? 3) On macOS, `sync_all` uses a different system call from a plain `fsync`. Why?",
    teaches=["Sync the bytes, then rename, then sync the directory: each step covers what the others do not.", "Delete the old version only after the new one cannot be lost.", "After the point of no return, a clean-up failure is not a failure of the operation."],
    wrong={
        "renames_before_the_bytes_are_safe": broken(SOLUTION, ("    disk.sync_file(&tmp)?; // the bytes must be safe before the name can be\n", "")),
        "forgets_the_directory": broken(SOLUTION, ("    disk.sync_dir()?; // only now is the new version sure to be found\n", "")),
        "removes_old_before_the_new_name_exists": broken(SOLUTION, ("    disk.rename(&tmp, &name)?;\n", "    if next > 1 {\n        disk.remove(&format!(\"state-{}\", next - 1)).ok();\n        disk.sync_dir()?;\n    }\n    disk.rename(&tmp, &name)?;\n")),
        "writes_the_final_name_directly": broken(SOLUTION, ('let tmp = format!("{name}.tmp");', "let tmp = name.clone();"), ("    disk.rename(&tmp, &name)?;\n", "")),
        "newest_by_text_order": broken(SOLUTION, ("    Ok(disk.list()?.iter().filter_map(|n| generation(n)).max())", "    let mut names = disk.list()?;\n    names.sort();\n    Ok(names.iter().rev().find_map(|n| generation(n)))")),
        "cleanup_failure_fails_the_save": broken(SOLUTION, ("                let _ = disk.remove(&old);", "                disk.remove(&old)?;")),
        "recover_trusts_unsynced_names": broken(SOLUTION, ("    // the name may so far exist only in memory; make it safe before anyone acts on it\n    disk.sync_dir()?;\n", "")),
    },
)
