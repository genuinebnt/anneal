//! Disks that keep their pages in memory, so that the layers above can be measured without the file system in the way.

use std::io;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Mutex;

use super::disk_manager::DiskIo;
use crate::common::config::{PageData, PageId, BUSTUB_PAGE_SIZE};

/// A disk of fixed size: pages `0..capacity`, kept in one buffer. Writing a page id `>= capacity` panics ("ran out of disk space").
pub struct DiskManagerMemory {
    memory: Mutex<Vec<u8>>,
    page_capacity: usize,
    num_writes: AtomicUsize,
}

impl DiskManagerMemory {
    /// A zeroed disk of `capacity` pages.
    pub fn new(capacity: usize) -> DiskManagerMemory {
        // @begin 1a-04
        DiskManagerMemory { memory: Mutex::new(vec![0; capacity * BUSTUB_PAGE_SIZE]), page_capacity: capacity, num_writes: AtomicUsize::new(0) }
        //~ todo!("1a-04: a zeroed buffer of capacity pages")
        // @end
    }

    pub fn get_num_writes(&self) -> usize {
        self.num_writes.load(Ordering::Relaxed)
    }
}

impl DiskIo for DiskManagerMemory {
    fn read_page(&self, page_id: PageId, buf: &mut PageData) -> io::Result<()> {
        // @begin 1a-04
        let at = self.range(page_id, "read");
        buf.copy_from_slice(&self.memory.lock().unwrap()[at]);
        Ok(())
        //~ todo!("1a-04: copy the page's bytes out of the buffer")
        // @end
    }

    fn write_page(&self, page_id: PageId, data: &PageData) -> io::Result<()> {
        // @begin 1a-04
        let at = self.range(page_id, "write");
        self.memory.lock().unwrap()[at].copy_from_slice(data);
        self.num_writes.fetch_add(1, Ordering::Relaxed);
        Ok(())
        //~ todo!("1a-04: copy data into the page's bytes and count the write")
        // @end
    }

    /// Nothing to reclaim in memory.
    fn delete_page(&self, _page_id: PageId) {}
}

impl DiskManagerMemory {
    /// The byte range of a page; panics for an id outside `0..capacity`.
    fn range(&self, page_id: PageId, what: &str) -> std::ops::Range<usize> {
        // @begin 1a-04
        assert!(page_id.is_valid() && (page_id.0 as usize) < self.page_capacity, "{what} of page {} on a disk of {} pages: ran out of disk space", page_id.0, self.page_capacity);
        let start = page_id.0 as usize * BUSTUB_PAGE_SIZE;
        start..start + BUSTUB_PAGE_SIZE
        //~ todo!("1a-04: assert the id is in 0..capacity (a message that says the disk ran out of space), then the page's byte range")
        // @end
    }
}

/// A disk with no size limit: pages spring into existence, zeroed, when they are first written. Reading a page that was never written gives zeros.
/// Deleting is a no-op.
pub struct DiskManagerUnlimitedMemory {
    pages: Mutex<Vec<Option<Box<PageData>>>>,
    num_writes: AtomicUsize,
}

impl DiskManagerUnlimitedMemory {
    pub fn new() -> DiskManagerUnlimitedMemory {
        DiskManagerUnlimitedMemory { pages: Mutex::new(Vec::new()), num_writes: AtomicUsize::new(0) }
    }

    pub fn get_num_writes(&self) -> usize {
        self.num_writes.load(Ordering::Relaxed)
    }

    /// Bytes of page data held: one page for each page id that has been written.
    pub fn get_memory_usage(&self) -> usize {
        // @begin 1a-04
        self.pages.lock().unwrap().iter().filter(|p| p.is_some()).count() * BUSTUB_PAGE_SIZE
        //~ todo!("1a-04: count the pages that exist; each takes BUSTUB_PAGE_SIZE bytes")
        // @end
    }
}

impl Default for DiskManagerUnlimitedMemory {
    fn default() -> Self {
        Self::new()
    }
}

impl DiskIo for DiskManagerUnlimitedMemory {
    fn read_page(&self, page_id: PageId, buf: &mut PageData) -> io::Result<()> {
        assert!(page_id.is_valid(), "read of invalid page id {}", page_id.0);
        // @begin 1a-04
        match self.pages.lock().unwrap().get(page_id.0 as usize) {
            Some(Some(page)) => buf.copy_from_slice(&**page),
            _ => buf.fill(0),
        }
        Ok(())
        //~ todo!("1a-04: copy the page out if it exists, otherwise zero the buffer")
        // @end
    }

    fn write_page(&self, page_id: PageId, data: &PageData) -> io::Result<()> {
        assert!(page_id.is_valid(), "write of invalid page id {}", page_id.0);
        // @begin 1a-04
        let mut pages = self.pages.lock().unwrap();
        let at = page_id.0 as usize;
        if at >= pages.len() {
            pages.resize_with(at + 1, || None);
        }
        pages[at].get_or_insert_with(|| Box::new([0u8; BUSTUB_PAGE_SIZE])).copy_from_slice(data);
        self.num_writes.fetch_add(1, Ordering::Relaxed);
        Ok(())
        //~ todo!("1a-04: grow the list of pages if needed, create the page if it doesn't exist, copy data in, count the write")
        // @end
    }

    fn delete_page(&self, _page_id: PageId) {}
}
