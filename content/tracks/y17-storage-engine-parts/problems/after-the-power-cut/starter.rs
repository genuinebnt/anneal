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

/// Every directory that can be found after the power fails, when `ops` is what had been done by then.
pub fn after_power_cut(ops: &[Op]) -> BTreeSet<Files> {
    todo!()
}
