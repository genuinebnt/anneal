use std::fs::{File, OpenOptions};
use std::io::{self, Write};
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
        // append mode: every write goes to the current end, also after the file was cut shorter
        let file = OpenOptions::new().create(true).append(true).open(path)?;
        Ok(FileDevice { file })
    }
}

impl LogDevice for FileDevice {
    fn len(&mut self) -> io::Result<u64> {
        Ok(self.file.metadata()?.len())
    }
    fn append(&mut self, bytes: &[u8]) -> io::Result<()> {
        self.file.write_all(bytes)
    }
    fn truncate(&mut self, len: u64) -> io::Result<()> {
        self.file.set_len(len)
    }
    fn sync(&mut self) -> io::Result<()> {
        self.file.sync_data()
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
    /// The length of the part made of whole, acknowledged records.
    len: u64,
    broken: bool,
}

impl<D: LogDevice> Log<D> {
    pub fn new(mut device: D) -> io::Result<Log<D>> {
        let len = device.len()?;
        Ok(Log {
            device,
            len,
            broken: false,
        })
    }

    /// Adds one record as a 4-byte little-endian length followed by the record. `Ok` means it is on disk for good.
    pub fn append(&mut self, record: &[u8]) -> Result<(), AppendError> {
        if self.broken {
            return Err(AppendError::Unusable);
        }
        let mut frame = Vec::with_capacity(4 + record.len());
        frame.extend_from_slice(&(record.len() as u32).to_le_bytes());
        frame.extend_from_slice(record);
        if let Err(error) = self.device.append(&frame) {
            // part of the frame may be there; later records must not land behind it
            if self.device.truncate(self.len).is_err() {
                self.broken = true;
                return Err(AppendError::Broke(error.kind()));
            }
            return Err(AppendError::NotAdded(error.kind()));
        }
        if let Err(error) = self.device.sync().or_else(|_| self.device.sync()) {
            // a second sync that succeeds would not show that these bytes were kept
            self.broken = true;
            return Err(AppendError::Broke(error.kind()));
        }
        self.len += frame.len() as u64;
        Ok(())
    }
}
