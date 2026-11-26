from y17_parts.common import broken

TYPES = r'''use std::collections::{BTreeMap, BTreeSet};

/// One thing a program did to the files of a single directory.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Op {
    /// Makes a new, empty file under this name. If the name is taken, it now means the new file.
    Create(String),
    /// Adds bytes at the end of the file with this name.
    Write(String, Vec<u8>),
    /// `fsync` on the file with this name.
    SyncFile(String),
    /// Gives the first name's file the second name, replacing whatever had it.
    Rename(String, String),
    Remove(String),
    /// `fsync` on the directory.
    SyncDir,
}

/// What a directory holds: each name and its file's bytes.
pub type Files = BTreeMap<String, Vec<u8>>;
'''

STARTER = TYPES + r'''
/// Every directory that can be found after the power fails, when `ops` is what had been done by then.
pub fn after_power_cut(ops: &[Op]) -> BTreeSet<Files> {
    todo!()
}
'''

SOLUTION = TYPES + r'''
/// A file's bytes, apart from any name it has.
#[derive(Default, Clone)]
struct Inode {
    data: Vec<u8>,
    /// How many bytes at the front a power cut cannot take.
    durable: usize,
}

#[derive(Clone)]
enum DirChange {
    Link(String, usize),
    Move(String, String),
    Unlink(String),
}

fn apply(dir: &mut BTreeMap<String, usize>, change: &DirChange) {
    match change {
        DirChange::Link(name, inode) => {
            dir.insert(name.clone(), *inode);
        }
        DirChange::Move(from, to) => {
            if let Some(inode) = dir.remove(from) {
                dir.insert(to.clone(), inode);
            }
        }
        DirChange::Unlink(name) => {
            dir.remove(name);
        }
    }
}

/// Every directory that can be found after the power fails, when `ops` is what had been done by then.
pub fn after_power_cut(ops: &[Op]) -> BTreeSet<Files> {
    let mut inodes: Vec<Inode> = Vec::new();
    // the directory as the running program sees it
    let mut live: BTreeMap<String, usize> = BTreeMap::new();
    // the directory as the disk is sure to have it, and the changes it may or may not have yet
    let mut stored: BTreeMap<String, usize> = BTreeMap::new();
    let mut pending: Vec<DirChange> = Vec::new();
    for op in ops {
        let change = match op {
            Op::Create(name) => {
                inodes.push(Inode::default());
                Some(DirChange::Link(name.clone(), inodes.len() - 1))
            }
            Op::Write(name, bytes) => {
                inodes[live[name]].data.extend_from_slice(bytes);
                None
            }
            Op::SyncFile(name) => {
                let inode = &mut inodes[live[name]];
                inode.durable = inode.data.len();
                None
            }
            Op::Rename(from, to) => Some(DirChange::Move(from.clone(), to.clone())),
            Op::Remove(name) => Some(DirChange::Unlink(name.clone())),
            Op::SyncDir => {
                stored = live.clone();
                pending.clear();
                None
            }
        };
        if let Some(change) = change {
            apply(&mut live, &change);
            pending.push(change);
        }
    }
    let mut out = BTreeSet::new();
    let mut dir = stored;
    for step in 0..=pending.len() {
        if step > 0 {
            apply(&mut dir, &pending[step - 1]);
        }
        // every file keeps its synced bytes and any part of what followed, each on its own
        let mut states = vec![Files::new()];
        for (name, &index) in &dir {
            let inode = &inodes[index];
            let mut next = Vec::new();
            for state in &states {
                for kept in inode.durable..=inode.data.len() {
                    let mut state = state.clone();
                    state.insert(name.clone(), inode.data[..kept].to_vec());
                    next.push(state);
                }
            }
            states = next;
        }
        out.extend(states);
    }
    out
}
'''

HARNESS = r'''
use std::collections::BTreeSet;

#[allow(dead_code)]
fn create(n: &str) -> Op {
    Op::Create(n.into())
}
#[allow(dead_code)]
fn write(n: &str, b: &[u8]) -> Op {
    Op::Write(n.into(), b.to_vec())
}
#[allow(dead_code)]
fn sync(n: &str) -> Op {
    Op::SyncFile(n.into())
}
#[allow(dead_code)]
fn rename(a: &str, b: &str) -> Op {
    Op::Rename(a.into(), b.into())
}
#[allow(dead_code)]
fn remove(n: &str) -> Op {
    Op::Remove(n.into())
}
#[allow(dead_code)]
fn dir(files: &[(&str, &[u8])]) -> Files {
    files.iter().map(|(n, b)| (n.to_string(), b.to_vec())).collect()
}
#[allow(dead_code)]
fn set(dirs: &[Files]) -> BTreeSet<Files> {
    dirs.iter().cloned().collect()
}
'''

VISIBLE = HARNESS + r'''
#[test]
fn nothing_done_nothing_there() {
    check!("[]", after_power_cut(&[]), set(&[dir(&[])]));
}

#[test]
fn unsynced_bytes_may_be_cut_anywhere() {
    let ops = [create("a"), Op::SyncDir, write("a", b"hi")];
    check!(format!("{ops:?}"), after_power_cut(&ops), set(&[dir(&[("a", b"")]), dir(&[("a", b"h")]), dir(&[("a", b"hi")])]));
}

#[test]
fn syncing_a_file_does_not_save_its_name() {
    let ops = [create("a"), write("a", b"x"), sync("a")];
    check!(format!("{ops:?}"), after_power_cut(&ops), set(&[dir(&[]), dir(&[("a", b"x")])]));
}

#[test]
fn syncing_the_directory_does_not_save_bytes() {
    let ops = [create("a"), write("a", b"x"), Op::SyncDir];
    check!(format!("{ops:?}"), after_power_cut(&ops), set(&[dir(&[("a", b"")]), dir(&[("a", b"x")])]));
}

#[test]
fn a_rename_can_reach_the_disk_before_the_bytes_do() {
    let ops = [create("t"), write("t", b"v2"), rename("t", "a")];
    check!(format!("{ops:?}"), after_power_cut(&ops), set(&[
        dir(&[]),
        dir(&[("t", b"")]), dir(&[("t", b"v")]), dir(&[("t", b"v2")]),
        dir(&[("a", b"")]), dir(&[("a", b"v")]), dir(&[("a", b"v2")]),
    ]));
}
'''

HIDDEN = HARNESS + r'''
#[test]
fn directory_changes_arrive_in_order() {
    // "b" can only be there if "a" is
    let ops = [create("a"), create("b")];
    check!(format!("{ops:?}"), after_power_cut(&ops), set(&[dir(&[]), dir(&[("a", b"")]), dir(&[("a", b""), ("b", b"")])]));
}

#[test]
fn each_file_is_cut_on_its_own() {
    let ops = [create("a"), create("b"), Op::SyncDir, write("a", b"1"), write("b", b"2")];
    check!(format!("{ops:?}"), after_power_cut(&ops), set(&[
        dir(&[("a", b""), ("b", b"")]), dir(&[("a", b"1"), ("b", b"")]),
        dir(&[("a", b""), ("b", b"2")]), dir(&[("a", b"1"), ("b", b"2")]),
    ]));
}

#[test]
fn bytes_synced_earlier_stay() {
    let ops = [create("a"), Op::SyncDir, write("a", b"ab"), sync("a"), write("a", b"c"), write("a", b"d")];
    check!(format!("{ops:?}"), after_power_cut(&ops), set(&[dir(&[("a", b"ab")]), dir(&[("a", b"abc")]), dir(&[("a", b"abcd")])]));
}

#[test]
fn syncing_one_file_leaves_the_other() {
    let ops = [create("a"), create("b"), Op::SyncDir, write("a", b"1"), write("b", b"2"), sync("a")];
    check!(format!("{ops:?}"), after_power_cut(&ops), set(&[dir(&[("a", b"1"), ("b", b"")]), dir(&[("a", b"1"), ("b", b"2")])]));
}

#[test]
fn an_unsynced_remove_may_come_back() {
    let ops = [create("a"), write("a", b"k"), sync("a"), Op::SyncDir, remove("a")];
    check!(format!("{ops:?}"), after_power_cut(&ops), set(&[dir(&[]), dir(&[("a", b"k")])]));
    let mut done = ops.to_vec();
    done.push(Op::SyncDir);
    check!("and after a directory sync", after_power_cut(&done), set(&[dir(&[])]));
}

#[test]
fn create_over_a_name_is_a_new_file() {
    let ops = [create("a"), write("a", b"old"), sync("a"), Op::SyncDir, create("a"), write("a", b"n")];
    check!(format!("{ops:?}"), after_power_cut(&ops), set(&[dir(&[("a", b"old")]), dir(&[("a", b"")]), dir(&[("a", b"n")])]));
}

#[test]
fn a_sync_follows_the_file_through_a_rename() {
    let ops = [create("t"), write("t", b"z"), Op::SyncDir, rename("t", "a"), sync("a")];
    check!(format!("{ops:?}"), after_power_cut(&ops), set(&[dir(&[("t", b"z")]), dir(&[("a", b"z")])]));
}

#[test]
fn writes_after_a_rename_go_to_the_same_file() {
    let ops = [create("a"), Op::SyncDir, rename("a", "b"), write("b", b"q")];
    check!(format!("{ops:?}"), after_power_cut(&ops), set(&[
        dir(&[("a", b"")]), dir(&[("a", b"q")]), dir(&[("b", b"")]), dir(&[("b", b"q")]),
    ]));
}

#[test]
fn the_careful_replacement_is_old_or_new() {
    let ops = [create("s"), write("s", b"old"), sync("s"), Op::SyncDir, create("t"), write("t", b"new"), sync("t"), rename("t", "s")];
    for state in after_power_cut(&ops) {
        let s = state.get("s").cloned();
        check!(format!("a state: {state:?}"), s == Some(b"old".to_vec()) || s == Some(b"new".to_vec()), true);
    }
}

/// A second way to the same answer: for one choice of how many directory changes arrived, replay the whole history and
/// follow each file by number.
fn by_replay(ops: &[Op], arrived: usize) -> Option<Vec<(String, Vec<u8>, usize)>> {
    let last_sync = ops.iter().rposition(|o| *o == Op::SyncDir).map_or(0, |i| i + 1);
    let mut live: std::collections::BTreeMap<String, usize> = Default::default();
    let mut disk = live.clone();
    let mut files: Vec<(Vec<u8>, usize)> = Vec::new();
    let mut left = arrived;
    for (i, op) in ops.iter().enumerate() {
        let changes_names = matches!(op, Op::Create(_) | Op::Rename(..) | Op::Remove(_));
        let reaches_disk = changes_names && (i < last_sync || left > 0);
        if changes_names && i >= last_sync && left > 0 {
            left -= 1;
        }
        let targets: Vec<&mut std::collections::BTreeMap<String, usize>> = if reaches_disk { vec![&mut live, &mut disk] } else { vec![&mut live] };
        match op {
            Op::Create(n) => {
                files.push((Vec::new(), 0));
                for d in targets {
                    d.insert(n.clone(), files.len() - 1);
                }
            }
            Op::Rename(a, b) => {
                for d in targets {
                    if let Some(f) = d.remove(a) {
                        d.insert(b.clone(), f);
                    }
                }
            }
            Op::Remove(n) => {
                for d in targets {
                    d.remove(n);
                }
            }
            Op::Write(n, bytes) => files[live[n]].0.extend_from_slice(bytes),
            Op::SyncFile(n) => {
                let f = &mut files[live[n]];
                f.1 = f.0.len();
            }
            Op::SyncDir => {}
        }
    }
    (left == 0).then(|| disk.into_iter().map(|(n, f)| (n, files[f].0.clone(), files[f].1)).collect())
}

#[test]
fn random_histories() {
    let mut rng = anneal_prelude::Rng::new(170201);
    for _ in 0..1500 {
        let mut ops = Vec::new();
        let mut names: BTreeSet<String> = BTreeSet::new();
        let steps = 1 + rng.below(8);
        for _ in 0..steps {
            let any = ["a", "b", "c"][rng.below(3)].to_string();
            let existing: Vec<String> = names.iter().cloned().collect();
            let pick = if existing.is_empty() { None } else { Some(existing[rng.below(existing.len())].clone()) };
            match (rng.below(7), pick) {
                (0, _) | (_, None) => {
                    names.insert(any.clone());
                    ops.push(Op::Create(any));
                }
                (1 | 2, Some(n)) => {
                    let len = 1 + rng.below(2);
                    ops.push(Op::Write(n, rng.vec(len, 0, 3)));
                }
                (3, Some(n)) => ops.push(Op::SyncFile(n)),
                (4, Some(n)) => {
                    names.remove(&n);
                    names.insert(any.clone());
                    ops.push(Op::Rename(n, any));
                }
                (5, Some(n)) => {
                    names.remove(&n);
                    ops.push(Op::Remove(n));
                }
                _ => ops.push(Op::SyncDir),
            }
        }
        let mut want: BTreeSet<Files> = BTreeSet::new();
        for arrived in 0..=ops.len() {
            let Some(files) = by_replay(&ops, arrived) else { continue };
            let mut states = vec![Files::new()];
            for (name, data, synced) in files {
                states = states
                    .iter()
                    .flat_map(|s| {
                        (synced..=data.len()).map(|k| {
                            let mut s = s.clone();
                            s.insert(name.clone(), data[..k].to_vec());
                            s
                        })
                    })
                    .collect();
            }
            want.extend(states);
        }
        check!(format!("{ops:?}"), after_power_cut(&ops), want);
    }
}
'''

PROBLEM = dict(
    slug="after-the-power-cut", title="What is on the disk after a power cut", level="medium", stage="files-that-survive",
    tags=["durability", "fsync", "crash states", "simulation", "file systems"],
    statement=r'''
A program writes its settings carefully, or so its author thinks. After a power cut the settings file is there and empty. To test such code you need to know every disk the power cut could leave behind.

Write `after_power_cut(ops)`. `ops` is everything a program did, in order, to the files of one directory before the power failed. Return the set of all directories that can be found afterwards.

**Names and files are separate things.** A file is its bytes. The directory says which name means which file. `Rename` moves a name and leaves the bytes alone. `Write` and `SyncFile` act on whatever file the name means **at that moment**. `Create` always makes a new file, even if the name was taken.

**The rules of this model** (a simplification of real file systems, and one you can rely on here):

1. `SyncFile` makes the bytes that file holds at that moment safe. It says nothing about any name.
2. `SyncDir` makes the directory's names safe as they are at that moment. It says nothing about bytes.
3. `Create`, `Rename` and `Remove` since the last `SyncDir` reach the disk **in order**: after the cut, the first `k` of them are there, for some `k` from none to all. A rename happens completely or not at all.
4. Each file that has a name after the cut holds its safe bytes, followed by **any prefix** of the bytes written after them. Files are cut independently of each other, and independently of `k`.

Every `Write`, `SyncFile`, `Rename` and `Remove` in the input names a file that exists at that moment, as the running program sees it. A rename replaces the file that had the new name. Renaming a file to its own name changes nothing.

The histories in the tests are small (at most a few thousand possible disks).
''',
    starter=STARTER, solution=SOLUTION, visible=[VISIBLE], hidden=[HIDDEN], use="use solution::*;",
    hints=[
        ("approach", "Walk through `ops` once, keeping what the running program sees and, apart from it, what the disk is certain to have. At the end, build the possible disks from the second plus the things that are still in doubt."),
        ("rust", "Give each file a number and keep the files in a `Vec`; a directory is then a `BTreeMap<String, usize>`. To combine independent choices, start with one empty result and, for each file, replace the list by every pairing of the list with that file's choices."),
        ("edge", "A write after a rename goes to the same file, so it shows under whichever name the cut leaves. A removed file can come back with bytes that were never synced."),
    ],
    notes=("Files are numbered, so that a name change never copies or loses bytes. Names have a stored version and a list of changes still in doubt; bytes have a safe length. The possible disks are every prefix of the doubtful changes, combined with every length of every file.", "O(states x size of a state)", "O(states x size of a state)"),
    follow_up="1) By these rules, write the shortest list of operations that replaces a file's contents so that every possible disk holds the old or the new bytes. 2) Real file systems can reorder some of these, and can leave zeros where unsynced data should be. Which of your rules would you loosen first? 3) Why does `fsync` on a new file not make the file findable?",
    teaches=["A file's bytes and its name are saved separately: one sync does not do the other's work.", "A rename can be on disk while the renamed file's bytes are not.", "A crash test is only as good as its model of what a crash can leave."],
    wrong={
        "file_sync_saves_the_name": broken(SOLUTION, ("                inode.durable = inode.data.len();\n                None", "                inode.durable = inode.data.len();\n                stored = live.clone();\n                pending.clear();\n                None")),
        "directory_sync_saves_bytes": broken(SOLUTION, ("                stored = live.clone();\n", "                stored = live.clone();\n                for inode in &mut inodes {\n                    inode.durable = inode.data.len();\n                }\n")),
        "unsynced_bytes_all_or_nothing": broken(SOLUTION, ("for kept in inode.durable..=inode.data.len() {", "for kept in [inode.durable, inode.data.len()] {")),
        "names_are_always_safe": broken(SOLUTION, ("    let mut dir = stored;\n    for step in 0..=pending.len() {\n        if step > 0 {", "    let _ = stored;\n    let mut dir = live.clone();\n    for step in 0..=0 {\n        if step > 0 {")),
        "unsynced_bytes_always_lost": broken(SOLUTION, ("for kept in inode.durable..=inode.data.len() {", "for kept in inode.durable..=inode.durable {")),
    },
)
