use std::io::{self, Read, Seek, SeekFrom};

#[derive(Debug, PartialEq, Eq, Clone, Copy)]
pub enum BundleError {
    /// The reader failed.
    Io(io::ErrorKind),
    /// The bytes are not a well-formed bundle.
    Corrupt,
}

impl From<io::Error> for BundleError {
    fn from(error: io::Error) -> BundleError {
        BundleError::Io(error.kind())
    }
}

pub struct Bundle<R: Read + Seek> {
    reader: R,
}

impl<R: Read + Seek> Bundle<R> {
    pub fn open(reader: R) -> Result<Bundle<R>, BundleError> {
        todo!()
    }

    /// The names, in the order the directory lists them.
    pub fn names(&self) -> Vec<&str> {
        todo!()
    }

    /// The bytes stored under `name`, or `None` if there is no such entry.
    pub fn get(&mut self, name: &str) -> Result<Option<Vec<u8>>, BundleError> {
        todo!()
    }
}
