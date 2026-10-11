//! Where the log lives: an append-only byte stream that survives a crash once `append` has returned.

use std::fs;
use std::io;

use crate::storage::disk::disk_manager::DiskManager;

pub trait LogIo: Send + Sync {
    /// Appends the bytes at the end of the log and makes them durable before returning.
    fn append(&self, data: &[u8]) -> io::Result<()>;
    /// Every byte of the log, from the start.
    fn read_all(&self) -> io::Result<Vec<u8>>;
    /// Cuts the log to `len` bytes (to drop a torn tail before appending after it).
    fn truncate(&self, len: u64) -> io::Result<()>;
}

impl LogIo for DiskManager {
    fn append(&self, data: &[u8]) -> io::Result<()> {
        self.write_log(data)
    }

    fn read_all(&self) -> io::Result<Vec<u8>> {
        match fs::read(self.log_file_name()) {
            Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(Vec::new()),
            other => other,
        }
    }

    fn truncate(&self, len: u64) -> io::Result<()> {
        fs::OpenOptions::new().write(true).open(self.log_file_name())?.set_len(len)
    }
}
