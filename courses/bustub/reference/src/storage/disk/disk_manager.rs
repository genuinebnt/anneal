//! The disk manager reads and writes pages to and from a file, and a log to a second file. Pages are named by `PageId`; where a page lives in the
//! file is the disk manager's own business.

use std::io;
use std::path::Path;

use crate::common::config::{PageData, PageId};

// @begin 1a-01
use std::collections::HashMap;
use std::fs::{File, OpenOptions};
use std::io::Write;
use std::os::unix::fs::FileExt;
use std::path::PathBuf;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Mutex;

use crate::common::config::{BUSTUB_PAGE_SIZE, DEFAULT_DB_IO_SIZE};

/// The byte where slot `slot` starts. Slots lie end to end: slot 0 at byte 0, slot 1 one page later, ...
fn slot_offset(slot: usize) -> u64 {
    slot as u64 * BUSTUB_PAGE_SIZE as u64
}

/// The size of a db file with room for `capacity` pages. BusTub keeps one spare page at the end.
fn file_size_for(capacity: usize) -> u64 {
    slot_offset(capacity + 1)
}

/// Reads into `buf` from byte `offset` until `buf` is full or the file ends; returns how many bytes it got. A single
/// `read_at` may return fewer bytes than asked for even when more are there, so this keeps asking.
fn read_full_at(file: &File, buf: &mut [u8], offset: u64) -> io::Result<usize> {
    let mut filled = 0;
    while filled < buf.len() {
        match file.read_at(&mut buf[filled..], offset + filled as u64) {
            Ok(0) => break,
            Ok(n) => filled += n,
            Err(e) if e.kind() == io::ErrorKind::Interrupted => continue,
            Err(e) => return Err(e),
        }
    }
    Ok(filled)
}

/// Everything the db file's latch protects: the file and the bookkeeping that says where pages are.
struct DbIo {
    file: File,
    /// page id -> the slot holding it.
    pages: HashMap<PageId, usize>,
    /// Slots whose pages were deleted, newest last.
    free_slots: Vec<usize>,
    /// How many slots have ever been handed out; the next fresh slot is this number.
    num_slots: usize,
    /// How many pages the file has room for: its length is `file_size_for(page_capacity)`.
    page_capacity: usize,
}

impl DbIo {
    /// A slot for a new page: a freed one if there is one, otherwise a fresh slot at the end, growing the file
    /// (doubling `page_capacity`) when the fresh slot would not fit.
    fn allocate_slot(&mut self) -> io::Result<usize> {
        // @begin 1a-02
        if let Some(slot) = self.free_slots.pop() {
            return Ok(slot);
        }
        // @end
        let slot = self.num_slots;
        self.num_slots += 1;
        // @begin 1a-02
        if slot >= self.page_capacity {
            self.page_capacity *= 2;
            self.file.set_len(file_size_for(self.page_capacity))?;
        }
        // @end
        Ok(slot)
    }
}
//~ // TODO(1a-01): your private types and helper functions go here. Nothing in this file is prescribed except the public
//~ // methods below; the tests call only those.
// @end

/// Reads and writes pages to a database file, and a log to a log file. All methods take `&self`: the disk manager is
/// shared between threads, so whatever mutable state you keep must be protected (a `Mutex` is the usual way).
pub struct DiskManager {
    // @begin 1a-01
    db_file_name: PathBuf,
    log_file_name: PathBuf,
    db_io: Mutex<DbIo>,
    log_io: Mutex<File>,
    num_flushes: AtomicUsize,
    num_writes: AtomicUsize,
    num_deletes: AtomicUsize,
    //~ // TODO(1a-01): the fields are yours to design. You will need at least: where each page lives, the open files, and
    //~ // the counters of 1a-03.
    // @end
}

impl DiskManager {
    /// Opens the database file `db_file`, creating it if it doesn't exist (an existing file keeps its contents), and the
    /// log file next to it: `db_file` with its extension replaced by `.log`, opened for appending and also created.
    /// Fails with the `io::Error` if either can't be opened; it must not panic.
    pub fn new(db_file: impl AsRef<Path>) -> io::Result<DiskManager> {
        // @begin 1a-01
        let db_file_name = db_file.as_ref().to_path_buf();
        let log_file_name = db_file_name.with_extension("log");
        let log_io = OpenOptions::new().read(true).append(true).create(true).open(&log_file_name)?;
        let db = OpenOptions::new().read(true).write(true).create(true).truncate(false).open(&db_file_name)?;
        db.set_len(file_size_for(DEFAULT_DB_IO_SIZE))?;
        Ok(DiskManager {
            db_file_name,
            log_file_name,
            db_io: Mutex::new(DbIo { file: db, pages: HashMap::new(), free_slots: Vec::new(), num_slots: 0, page_capacity: DEFAULT_DB_IO_SIZE }),
            log_io: Mutex::new(log_io),
            num_flushes: AtomicUsize::new(0),
            num_writes: AtomicUsize::new(0),
            num_deletes: AtomicUsize::new(0),
        })
        //~ todo!("1a-01: open (creating) the db file and the log file, and build the DiskManager")
        // @end
    }

    /// Syncs both files to disk.
    pub fn shut_down(&self) -> io::Result<()> {
        // @begin 1a-05
        self.db_io.lock().unwrap().file.sync_all()?;
        self.log_io.lock().unwrap().sync_all()
        //~ todo!("1a-05: make what was written durable: sync both files")
        // @end
    }

    /// The path of the db file, as given to `new`.
    pub fn db_file_name(&self) -> &Path {
        // @begin 1a-01
        &self.db_file_name
        //~ todo!("1a-01: the path new was given")
        // @end
    }

    /// The path of the log file.
    pub fn log_file_name(&self) -> &Path {
        // @begin 1a-01
        &self.log_file_name
        //~ todo!("1a-01: the db path with its extension replaced by `log`")
        // @end
    }

    /// The db file's size in bytes right now.
    pub fn get_db_file_size(&self) -> u64 {
        // @begin 1a-01
        std::fs::metadata(&self.db_file_name).expect("the db file exists").len()
        //~ todo!("1a-01: the length of the db file, from its metadata")
        // @end
    }

    /// Writes a page. Whatever was on that page before is replaced. (Page ids are non-negative.)
    pub fn write_page(&self, page_id: PageId, data: &PageData) -> io::Result<()> {
        assert!(page_id.is_valid(), "write of invalid page id {}", page_id.0);
        // @begin 1a-01
        let mut io = self.db_io.lock().unwrap();
        let slot = match io.pages.get(&page_id) {
            Some(&slot) => slot,
            None => {
                let slot = io.allocate_slot()?;
                io.pages.insert(page_id, slot);
                slot
            }
        };
        io.file.write_all_at(data, slot_offset(slot))?;
        // @begin 1a-03
        self.num_writes.fetch_add(1, Ordering::Relaxed);
        // @end
        Ok(())
        //~ todo!("1a-01: store the page so that read_page can find it again, whatever order pages arrive in")
        // @end
    }

    /// Reads a page into `buf`. A page that was never written reads as zeros. After `delete_page`, what a read returns is
    /// not specified: do not rely on it (a later `write_page` of that id defines it again).
    pub fn read_page(&self, page_id: PageId, buf: &mut PageData) -> io::Result<()> {
        assert!(page_id.is_valid(), "read of invalid page id {}", page_id.0);
        // @begin 1a-01
        let io = self.db_io.lock().unwrap();
        match io.pages.get(&page_id) {
            Some(&slot) => {
                let n = read_full_at(&io.file, buf, slot_offset(slot))?;
                buf[n..].fill(0);
                Ok(())
            }
            None => {
                buf.fill(0);
                Ok(())
            }
        }
        //~ todo!("1a-01: the page's bytes if it was written, otherwise a zeroed buffer")
        // @end
    }

    /// Forgets a page and makes its space available again. Deleting a page that doesn't exist does nothing.
    pub fn delete_page(&self, page_id: PageId) {
        // @begin 1a-02
        let mut io = self.db_io.lock().unwrap();
        let Some(slot) = io.pages.remove(&page_id) else {
            return;
        };
        io.free_slots.push(slot);
        // @begin 1a-03
        self.num_deletes.fetch_add(1, Ordering::Relaxed);
        // @end
        //~ todo!("1a-02: forget the page and let a later page use its space; do nothing if it isn't there")
        // @end
    }

    /// Appends `data` to the log file and counts a flush. An empty `data` does nothing at all.
    pub fn write_log(&self, data: &[u8]) -> io::Result<()> {
        // @begin 1a-03
        if data.is_empty() {
            return Ok(());
        }
        let mut log = self.log_io.lock().unwrap();
        log.write_all(data)?;
        self.num_flushes.fetch_add(1, Ordering::Relaxed);
        Ok(())
        //~ todo!("1a-03: append data to the log file and count a flush; an empty write does nothing")
        // @end
    }

    /// Reads up to `buf.len()` bytes of the log starting at byte `offset`. Returns `false` if `offset` is at or past
    /// the end of the log (then `buf` is untouched); otherwise `true`, with whatever the log doesn't have zero-filled.
    pub fn read_log(&self, buf: &mut [u8], offset: u64) -> io::Result<bool> {
        // @begin 1a-03
        let log = self.log_io.lock().unwrap();
        if offset >= log.metadata()?.len() {
            return Ok(false);
        }
        let n = read_full_at(&log, buf, offset)?;
        buf[n..].fill(0);
        Ok(true)
        //~ todo!("1a-03: false past the end of the log; otherwise the bytes from `offset`, zero-filled where the log ends")
        // @end
    }

    /// How many non-empty log writes there have been.
    pub fn get_num_flushes(&self) -> usize {
        // @begin 1a-03
        self.num_flushes.load(Ordering::Relaxed)
        //~ todo!("1a-03: the number of non-empty log writes")
        // @end
    }

    /// How many page writes there have been (a rewrite counts).
    pub fn get_num_writes(&self) -> usize {
        // @begin 1a-03
        self.num_writes.load(Ordering::Relaxed)
        //~ todo!("1a-03: the number of write_page calls")
        // @end
    }

    /// How many deletes of pages that existed there have been.
    pub fn get_num_deletes(&self) -> usize {
        // @begin 1a-03
        self.num_deletes.load(Ordering::Relaxed)
        //~ todo!("1a-03: the number of delete_page calls that found a page")
        // @end
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
    // @begin 1a-04
    fn read_page(&self, page_id: PageId, buf: &mut PageData) -> io::Result<()> {
        DiskManager::read_page(self, page_id, buf)
    }

    fn write_page(&self, page_id: PageId, data: &PageData) -> io::Result<()> {
        DiskManager::write_page(self, page_id, data)
    }

    fn delete_page(&self, page_id: PageId) {
        DiskManager::delete_page(self, page_id)
    }
    //~ fn read_page(&self, page_id: PageId, buf: &mut PageData) -> io::Result<()> {
    //~     todo!("1a-04: DiskManager is a disk too")
    //~ }
    //~
    //~ fn write_page(&self, page_id: PageId, data: &PageData) -> io::Result<()> {
    //~     todo!("1a-04: DiskManager is a disk too")
    //~ }
    //~
    //~ fn delete_page(&self, page_id: PageId) {
    //~     todo!("1a-04: DiskManager is a disk too")
    //~ }
    // @end
}

/// Copies page `from` to page `to` through any disk.
pub fn copy_page(disk: &dyn DiskIo, from: PageId, to: PageId) -> io::Result<()> {
    // @begin 1a-04
    let mut buf = [0u8; crate::common::config::BUSTUB_PAGE_SIZE];
    disk.read_page(from, &mut buf)?;
    disk.write_page(to, &buf)
    //~ todo!("1a-04: copy a page from one page id to another through whatever disk was passed in")
    // @end
}
