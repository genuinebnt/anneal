use std::collections::BTreeMap;
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
