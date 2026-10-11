//! The disk manager reads and writes pages to and from a file, and a log to a second file. Pages are named by `PageId`; where a page lives in the
//! file is the disk manager's own business.

use std::io;
use std::path::Path;

use crate::common::config::{PageData, PageId};

// TODO(1a-01): your private types and helper functions go here. Nothing in this file is prescribed except the public
// methods below; the tests call only those.

/// Reads and writes pages to a database file, and a log to a log file. All methods take `&self`: the disk manager is
/// shared between threads, so whatever mutable state you keep must be protected (a `Mutex` is the usual way).
pub struct DiskManager {
    // TODO(1a-01): the fields are yours to design. You will need at least: where each page lives, the open files, and
    // the counters of 1a-03.
}

impl DiskManager {
    /// Opens the database file `db_file`, creating it if it doesn't exist (an existing file keeps its contents), and the
    /// log file next to it: `db_file` with its extension replaced by `.log`, opened for appending and also created.
    /// Fails with the `io::Error` if either can't be opened; it must not panic.
    pub fn new(db_file: impl AsRef<Path>) -> io::Result<DiskManager> {
        todo!("1a-01: open (creating) the db file and the log file, and build the DiskManager")
    }

    /// Syncs both files to disk.
    pub fn shut_down(&self) -> io::Result<()> {
        todo!("1a-05: make what was written durable: sync both files")
    }

    /// The path of the db file, as given to `new`.
    pub fn db_file_name(&self) -> &Path {
        todo!("1a-01: the path new was given")
    }

    /// The path of the log file.
    pub fn log_file_name(&self) -> &Path {
        todo!("1a-01: the db path with its extension replaced by `log`")
    }

    /// The db file's size in bytes right now.
    pub fn get_db_file_size(&self) -> u64 {
        todo!("1a-01: the length of the db file, from its metadata")
    }

    /// Writes a page. Whatever was on that page before is replaced. (Page ids are non-negative.)
    pub fn write_page(&self, page_id: PageId, data: &PageData) -> io::Result<()> {
        assert!(page_id.is_valid(), "write of invalid page id {}", page_id.0);
        todo!("1a-01: store the page so that read_page can find it again, whatever order pages arrive in")
    }

    /// Reads a page into `buf`. A page that was never written reads as zeros. After `delete_page`, what a read returns is
    /// not specified: do not rely on it (a later `write_page` of that id defines it again).
    pub fn read_page(&self, page_id: PageId, buf: &mut PageData) -> io::Result<()> {
        assert!(page_id.is_valid(), "read of invalid page id {}", page_id.0);
        todo!("1a-01: the page's bytes if it was written, otherwise a zeroed buffer")
    }

    /// Forgets a page and makes its space available again. Deleting a page that doesn't exist does nothing.
    pub fn delete_page(&self, page_id: PageId) {
        todo!("1a-02: forget the page and let a later page use its space; do nothing if it isn't there")
    }

    /// Appends `data` to the log file and counts a flush. An empty `data` does nothing at all.
    pub fn write_log(&self, data: &[u8]) -> io::Result<()> {
        todo!("1a-03: append data to the log file and count a flush; an empty write does nothing")
    }

    /// Reads up to `buf.len()` bytes of the log starting at byte `offset`. Returns `false` if `offset` is at or past
    /// the end of the log (then `buf` is untouched); otherwise `true`, with whatever the log doesn't have zero-filled.
    pub fn read_log(&self, buf: &mut [u8], offset: u64) -> io::Result<bool> {
        todo!("1a-03: false past the end of the log; otherwise the bytes from `offset`, zero-filled where the log ends")
    }

    /// How many non-empty log writes there have been.
    pub fn get_num_flushes(&self) -> usize {
        todo!("1a-03: the number of non-empty log writes")
    }

    /// How many page writes there have been (a rewrite counts).
    pub fn get_num_writes(&self) -> usize {
        todo!("1a-03: the number of write_page calls")
    }

    /// How many deletes of pages that existed there have been.
    pub fn get_num_deletes(&self) -> usize {
        todo!("1a-03: the number of delete_page calls that found a page")
    }
}

/// What the rest of the system needs from a disk: read, write and delete pages. [`DiskManager`] is the real one;
/// [`DiskManagerMemory`](super::disk_manager_memory::DiskManagerMemory) and `DiskManagerUnlimitedMemory` keep pages in memory.
pub trait DiskIo: Send + Sync {
    fn read_page(&self, page_id: PageId, buf: &mut PageData) -> io::Result<()>;
    fn write_page(&self, page_id: PageId, data: &PageData) -> io::Result<()>;
    fn delete_page(&self, page_id: PageId);
}

impl DiskIo for DiskManager {
    fn read_page(&self, page_id: PageId, buf: &mut PageData) -> io::Result<()> {
        todo!("1a-04: DiskManager is a disk too")
    }
    
    fn write_page(&self, page_id: PageId, data: &PageData) -> io::Result<()> {
        todo!("1a-04: DiskManager is a disk too")
    }
    
    fn delete_page(&self, page_id: PageId) {
        todo!("1a-04: DiskManager is a disk too")
    }
}

/// Copies page `from` to page `to` through any disk.
pub fn copy_page(disk: &dyn DiskIo, from: PageId, to: PageId) -> io::Result<()> {
    todo!("1a-04: copy a page from one page id to another through whatever disk was passed in")
}
