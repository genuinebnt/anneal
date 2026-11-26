use std::fs::File;
use std::io;
use std::path::Path;

/// An append-only file, or a stand-in for one.
#[allow(clippy::len_without_is_empty)]
pub trait LogDevice {
    /// The number of bytes in it now.
    fn len(&mut self) -> io::Result<u64>;
    /// Adds `bytes` at the end. When it fails, **any prefix** of `bytes` may have been added.
    fn append(&mut self, bytes: &[u8]) -> io::Result<()>;
    /// Cuts it down to `len` bytes.
    fn truncate(&mut self, len: u64) -> io::Result<()>;
    /// Makes what was added so far survive a power cut.
    fn sync(&mut self) -> io::Result<()>;
}

/// The real thing.
pub struct FileDevice {
    file: File,
}

impl FileDevice {
    /// Opens `path`, creating it if it is missing. What it already holds stays.
    pub fn open(path: &Path) -> io::Result<FileDevice> {
        todo!()
    }
}

impl LogDevice for FileDevice {
    fn len(&mut self) -> io::Result<u64> {
        todo!()
    }
    fn append(&mut self, bytes: &[u8]) -> io::Result<()> {
        todo!()
    }
    fn truncate(&mut self, len: u64) -> io::Result<()> {
        todo!()
    }
    fn sync(&mut self) -> io::Result<()> {
        todo!()
    }
}

#[derive(Debug, PartialEq, Eq, Clone, Copy)]
pub enum AppendError {
    /// This record was not added. The log is as it was before the call, and still usable.
    NotAdded(io::ErrorKind),
    /// This call left the log in a state it cannot vouch for. It takes no more records.
    Broke(io::ErrorKind),
    /// An earlier call returned `Broke`.
    Unusable,
}

pub struct Log<D: LogDevice> {
    device: D,
}

impl<D: LogDevice> Log<D> {
    pub fn new(device: D) -> io::Result<Log<D>> {
        todo!()
    }

    /// Adds one record as a 4-byte little-endian length followed by the record. `Ok` means it is on disk for good.
    pub fn append(&mut self, record: &[u8]) -> Result<(), AppendError> {
        todo!()
    }
}
