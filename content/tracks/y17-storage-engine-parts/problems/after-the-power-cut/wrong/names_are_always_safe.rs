use std::collections::{BTreeMap, BTreeSet};

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
    let _ = stored;
    let mut dir = live.clone();
    for step in 0..=0 {
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
