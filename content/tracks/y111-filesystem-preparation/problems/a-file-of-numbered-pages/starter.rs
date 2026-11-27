use std::fs::File;
use std::io;
use std::path::Path;

#[derive(Debug, PartialEq, Eq)]
pub enum PageError {
    /// The page size is 0.
    BadPageSize,
    /// The file's length is not a multiple of the page size.
    NotWholePages,
    /// A page number past the end, or a size that does not fit in 64 bits.
    OutOfRange,
    /// A buffer that is not exactly one page long.
    WrongLength,
    Io(io::ErrorKind),
}

impl From<io::Error> for PageError {
    fn from(e: io::Error) -> PageError {
        PageError::Io(e.kind())
    }
}

pub struct PageFile {
    file: File,
    // TODO: whatever else you need
}

impl PageFile {
    pub fn create(path: &Path, page_size: usize, pages: u64) -> Result<PageFile, PageError> {
        todo!()
    }

    pub fn open(path: &Path, page_size: usize) -> Result<PageFile, PageError> {
        todo!()
    }

    pub fn pages(&self) -> u64 {
        todo!()
    }

    pub fn read(&self, n: u64, page: &mut [u8]) -> Result<(), PageError> {
        todo!()
    }

    pub fn write(&self, n: u64, page: &[u8]) -> Result<(), PageError> {
        todo!()
    }

    /// Adds `extra` pages of zeros and returns the number of the first.
    pub fn grow(&mut self, extra: u64) -> Result<u64, PageError> {
        todo!()
    }

    pub fn sync(&self) -> Result<(), PageError> {
        todo!()
    }
}
