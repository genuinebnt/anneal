use std::fs::{File, OpenOptions};
use std::io;
use std::os::unix::fs::FileExt;
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
    page_size: usize,
    pages: u64,
}

/// The length in bytes of `pages` pages, if a file can be that long.
fn length(pages: u64, page_size: usize) -> Result<u64, PageError> {
    pages
        .checked_mul(page_size as u64)
        .filter(|bytes| *bytes <= i64::MAX as u64)
        .ok_or(PageError::OutOfRange)
}

impl PageFile {
    pub fn create(path: &Path, page_size: usize, pages: u64) -> Result<PageFile, PageError> {
        if page_size == 0 {
            return Err(PageError::BadPageSize);
        }
        let file = OpenOptions::new()
            .read(true)
            .write(true)
            .create_new(true)
            .open(path)?;
        let bytes = length(pages, page_size)?;
        file.set_len(bytes)?;
        Ok(PageFile {
            file,
            page_size,
            pages,
        })
    }

    pub fn open(path: &Path, page_size: usize) -> Result<PageFile, PageError> {
        if page_size == 0 {
            return Err(PageError::BadPageSize);
        }
        let file = OpenOptions::new().read(true).write(true).open(path)?;
        let bytes = file.metadata()?.len();
        if bytes % page_size as u64 != 0 {
            return Err(PageError::NotWholePages);
        }
        Ok(PageFile {
            file,
            page_size,
            pages: bytes / page_size as u64,
        })
    }

    pub fn pages(&self) -> u64 {
        self.pages
    }

    /// Where page `n` starts, once the buffer and the number have been checked.
    fn place(&self, n: u64, len: usize) -> Result<u64, PageError> {
        if len != self.page_size {
            return Err(PageError::WrongLength);
        }
        if n >= self.pages {
            return Err(PageError::OutOfRange);
        }
        length(n, self.page_size)
    }

    pub fn read(&self, n: u64, page: &mut [u8]) -> Result<(), PageError> {
        let at = self.place(n, page.len())?;
        self.file.read_exact_at(page, at)?;
        Ok(())
    }

    pub fn write(&self, n: u64, page: &[u8]) -> Result<(), PageError> {
        let at = self.place(n, page.len())?;
        self.file.write_all_at(page, at)?;
        Ok(())
    }

    /// Adds `extra` pages of zeros and returns the number of the first.
    pub fn grow(&mut self, extra: u64) -> Result<u64, PageError> {
        let pages = self.pages.checked_add(extra).ok_or(PageError::OutOfRange)?;
        self.file.set_len(length(pages, self.page_size)?)?;
        let first = self.pages;
        self.pages = pages;
        Ok(first)
    }

    pub fn sync(&self) -> Result<(), PageError> {
        self.file.sync_data()?;
        Ok(())
    }
}
