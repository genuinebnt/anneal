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
        self.disk.append(TEMP, &compact)?;
        self.disk.sync(TEMP)?;
        self.disk.rename(TEMP, LOG)
    }

    pub fn cancel_rewrite(&mut self) {
        self.tail = None;
    }
}
