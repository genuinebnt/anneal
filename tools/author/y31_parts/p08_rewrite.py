from y17_parts.common import broken

TRAIT = r'''use std::collections::BTreeMap;
use std::io;

/// A directory, or a stand-in for one.
pub trait Disk {
    /// The file's bytes, or `None` when there is no such file.
    fn read(&mut self, name: &str) -> io::Result<Option<Vec<u8>>>;
    /// Adds bytes at the end, creating the file if needed. On an error, any prefix may have been added.
    fn append(&mut self, name: &str, bytes: &[u8]) -> io::Result<()>;
    /// Cuts the file down to `len` bytes.
    fn truncate(&mut self, name: &str, len: u64) -> io::Result<()>;
    /// Makes the file's bytes survive a power cut.
    fn sync(&mut self, name: &str) -> io::Result<()>;
    /// Renames `from` to `to` in one step, replacing `to`. The new name survives a power cut.
    fn rename(&mut self, from: &str, to: &str) -> io::Result<()>;
    /// Removes the file. A missing file is not an error.
    fn remove(&mut self, name: &str) -> io::Result<()>;
}

/// The log's file name.
pub const LOG: &str = "tally.log";
/// The only other name you may use.
pub const TEMP: &str = "tally.log.new";
'''

SOLUTION = TRAIT + r'''
/// Totals by name, kept in a log of changes.
pub struct Tally<D: Disk> {
    disk: D,
    totals: BTreeMap<String, i64>,
    /// While a rewrite runs: the lines logged since it began.
    tail: Option<Vec<u8>>,
}

fn invalid() -> io::Error {
    io::Error::from(io::ErrorKind::InvalidData)
}

impl<D: Disk> Tally<D> {
    pub fn open(mut disk: D) -> io::Result<Tally<D>> {
        let log = disk.read(LOG)?.unwrap_or_default();
        // a line that has no newline was never finished
        let whole = log.iter().rposition(|&b| b == b'\n').map_or(0, |i| i + 1);
        let text = std::str::from_utf8(&log[..whole]).map_err(|_| invalid())?;
        let mut totals = BTreeMap::new();
        // `lines` would also accept a `\r` before the `\n`, and `parse` a leading `+`
        for line in text.split_terminator('\n') {
            let (name, delta) = line.split_once(' ').ok_or_else(invalid)?;
            if delta.starts_with('+') {
                return Err(invalid());
            }
            let delta: i64 = delta.parse().map_err(|_| invalid())?;
            let total: &mut i64 = totals.entry(name.to_string()).or_default();
            *total = total.wrapping_add(delta);
        }
        if whole < log.len() {
            disk.truncate(LOG, whole as u64)?;
        }
        Ok(Tally {
            disk,
            totals,
            tail: None,
        })
    }

    pub fn get(&self, name: &str) -> i64 {
        self.totals.get(name).copied().unwrap_or(0)
    }

    pub fn add(&mut self, name: &str, delta: i64) -> io::Result<i64> {
        let line = format!("{name} {delta}\n").into_bytes();
        self.disk.append(LOG, &line)?;
        self.disk.sync(LOG)?;
        if let Some(tail) = &mut self.tail {
            tail.extend_from_slice(&line);
        }
        let total = self.totals.entry(name.to_string()).or_default();
        *total = total.wrapping_add(delta);
        Ok(*total)
    }

    pub fn start_rewrite(&mut self) -> Option<Vec<u8>> {
        if self.tail.is_some() {
            return None;
        }
        self.tail = Some(Vec::new());
        let lines = self
            .totals
            .iter()
            .map(|(name, total)| format!("{name} {total}\n"));
        Some(lines.collect::<String>().into_bytes())
    }

    pub fn finish_rewrite(&mut self, mut compact: Vec<u8>) -> io::Result<()> {
        let tail = self
            .tail
            .take()
            .ok_or_else(|| io::Error::from(io::ErrorKind::InvalidInput))?;
        compact.extend(tail);
        // an earlier attempt may have left a file there, and append would add to it
        self.disk.remove(TEMP)?;
        self.disk.append(TEMP, &compact)?;
        self.disk.sync(TEMP)?;
        self.disk.rename(TEMP, LOG)
    }

    pub fn cancel_rewrite(&mut self) {
        self.tail = None;
    }
}
'''

STARTER = TRAIT + r'''
/// Totals by name, kept in a log of changes.
pub struct Tally<D: Disk> {
    disk: D,
    // add what you need
}

impl<D: Disk> Tally<D> {
    /// Rebuilds the totals from the log.
    pub fn open(disk: D) -> io::Result<Tally<D>> {
        todo!()
    }

    pub fn get(&self, name: &str) -> i64 {
        todo!()
    }

    /// Adds `delta` and returns the new total, once the change is safe.
    pub fn add(&mut self, name: &str, delta: i64) -> io::Result<i64> {
        todo!()
    }

    /// Begins a rewrite and returns the compact log of the totals now. `None` if one is running.
    pub fn start_rewrite(&mut self) -> Option<Vec<u8>> {
        todo!()
    }

    /// Replaces the log with `compact` and everything added since the rewrite began.
    pub fn finish_rewrite(&mut self, compact: Vec<u8>) -> io::Result<()> {
        todo!()
    }

    pub fn cancel_rewrite(&mut self) {
        todo!()
    }
}
'''

HELPERS = r'''
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
'''

VISIBLE = [
    HELPERS,
    r'''
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
''',
]

HIDDEN = [
    HELPERS,
    r'''
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
''',
]

STATEMENT = r'''
A counting service logs every change as a line, such as `visits 1`, and rebuilds its totals from the log at start. The log grows without end, so now and then it is **rewritten** as one line per name. Writing that may take seconds, and the service must keep counting meanwhile.

The file is `LOG`. Each line is `name delta` ending in `\n`.

Names have no spaces or line ends. Deltas are `i64` as Rust writes them. Totals wrap on overflow.

- **`open(disk)`** rebuilds the totals. A missing log is empty. A last line without its `\n` was never finished: cut it off with `truncate`. A whole line that is not a name, one space and a number is an error of kind `InvalidData`, and the log is not changed.
- **`add(name, delta)`** appends the line and syncs. Only then does it change the total, and it returns the new one. (The tests never make a disk call fail during `add`.)
- **`get(name)`** is the total, 0 if never added.

**The rewrite** comes in two halves, so the compact form can be written elsewhere while `add` goes on.

- **`start_rewrite()`** returns the compact log of the totals now: one line `name total\n` for every name ever added to (totals of 0 included), in name order. It returns `None` while a rewrite is running.
- While it runs, `add` works as usual, on the old log.
- **`finish_rewrite(compact)`** replaces the log with `compact` (exactly what `start_rewrite` returned) followed by every change made **since the start, each once**. With no rewrite running it fails with `InvalidInput`.
- **`cancel_rewrite()`** forgets a running rewrite, if any.

**Crashes and failures.** The disk's promises are in the `Disk` trait: appended bytes survive a power cut only after `sync`, and a rename is atomic and lasting.

- The power may fail at any call during `finish_rewrite`. A restart must find every total as it was: nothing lost, nothing counted twice.
- If a call fails, return an error. The service keeps working on whichever log is in place, and a later rewrite must work.
- If it returns `Ok`, the new log is safe.
- However `finish_rewrite` ends, that rewrite is over.
- You may use one other name, `TEMP`. A crash may have left a file of that name behind, holding anything.
'''

PROBLEM = dict(
    slug="shrink-a-log-while-it-is-written", title="Shrink a log while it is still being written", level="hard", stage="disk",
    tags=["log compaction", "online rewrite", "atomic rename", "crash safety"],
    statement=STATEMENT, starter=STARTER, solution=SOLUTION, visible=VISIBLE, hidden=HIDDEN,
    teaches=[
        "A rewrite is a snapshot plus the tail written since; every change belongs to exactly one of them.",
        "Build the new file aside, sync it, and rename it over the old one.",
        "Leftovers from a failed attempt must not leak into the next one.",
    ],
    follow_up="1) The tail is kept in memory. What if writes arrive faster than the rewrite finishes? 2) Redis since version 7 keeps a base file and incremental files listed in a manifest. What does that remove from this design? 3) What changes if `add` may fail while a rewrite runs?",
    hints=[
        ("approach", "Between start and finish, every change must go to two places. Which two, and what does finish do with the second one?"),
        ("rust", "`Option<Vec<u8>>` holds the tail and also says whether a rewrite is running. `take()` it at the start of `finish_rewrite`, so every way out ends the rewrite."),
        ("edge", "`append` adds to whatever `TEMP` already holds. Sync before you rename: the rename is lasting, the unsynced bytes are not. And a partial line at the end of the log would glue onto the next one."),
    ],
    notes=("`start_rewrite` serialises the totals and begins collecting the lines added afterwards. `finish_rewrite` writes both to a fresh temporary file, syncs it and renames it over the log, so at every instant the log under its name is complete.",
           "O(names + tail) per rewrite", "O(names + tail)"),
    wrong={
        "tail_forgotten": broken(SOLUTION, ("        compact.extend(tail);\n", "        let _ = tail;\n")),
        "no_sync_before_rename": broken(SOLUTION, ("        self.disk.sync(TEMP)?;\n        self.disk.rename", "        self.disk.rename")),
        "leftover_temp_appended_to": broken(SOLUTION, ("        self.disk.remove(TEMP)?;\n", "")),
        "rewritten_from_totals_at_finish_plus_tail": broken(SOLUTION, (
            "        compact.extend(tail);\n", "        let now: String = self.totals.iter().map(|(n, t)| format!(\"{n} {t}\\n\")).collect();\n        compact = now.into_bytes();\n        compact.extend(tail);\n")),
        "rewrite_stays_open_after_a_failure": broken(SOLUTION, (
            "        let tail = self\n            .tail\n            .take()\n            .ok_or_else(|| io::Error::from(io::ErrorKind::InvalidInput))?;",
            "        let tail = self\n            .tail\n            .clone()\n            .ok_or_else(|| io::Error::from(io::ErrorKind::InvalidInput))?;"),
            ("        self.disk.rename(TEMP, LOG)\n    }", "        self.disk.rename(TEMP, LOG)?;\n        self.tail = None;\n        Ok(())\n    }")),
        "written_over_the_log_in_place": broken(SOLUTION, (
            "        self.disk.remove(TEMP)?;\n        self.disk.append(TEMP, &compact)?;\n        self.disk.sync(TEMP)?;\n        self.disk.rename(TEMP, LOG)",
            "        self.disk.truncate(LOG, 0)?;\n        self.disk.append(LOG, &compact)?;\n        self.disk.sync(LOG)")),
    },
)
