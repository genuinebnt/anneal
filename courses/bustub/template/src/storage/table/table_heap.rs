//! A table on disk: a linked list of table pages in the buffer pool. A tuple is added to the last page, and when that page is full a new page is
//! allocated and linked after it. A tuple is found by its [`Rid`] (page, slot); because slots never move, a rid stays valid.

// TODO(3c-01): your imports go here.

use super::table_iterator::TableIterator;
use super::tuple::{Tuple, TupleMeta};
use crate::buffer::buffer_pool_manager::BufferPoolManager;
use crate::common::config::PageId;
use crate::common::exception::{Exception, ExceptionType, Result};
use crate::common::rid::Rid;
use crate::storage::page::table_page::TablePage;

pub struct TableHeap<'a> {
    pub(crate) bpm: &'a BufferPoolManager,
    // TODO(3c-01): the fields are yours: where the table starts, where it ends, and what makes inserts take turns.
}

impl<'a> TableHeap<'a> {
    /// Creates an empty table: allocates the first page and formats it as a table page.
    pub fn new(bpm: &'a BufferPoolManager) -> TableHeap<'a> {
        todo!("3c-01: allocate a page for the first table page and init it; it is also the last page; remember the pool")
    }

    /// The id of the first page of the table.
    pub fn get_first_page_id(&self) -> PageId {
        todo!("3c-01: the id of the first page")
    }

    /// Adds the tuple at the end of the table and returns where it went. If the last page has no room, a new page is allocated and linked
    /// after it (and becomes the last page). A tuple that does not fit even an empty page is an error ("tuple is too large, cannot
    /// insert"). Inserts are serialised by the heap's lock.
    pub fn insert_tuple(&self, meta: &TupleMeta, tuple: &Tuple) -> Result<Rid> {
        todo!("3c-01: hold the heap's lock; write-latch the last page; while the tuple does not fit: (an empty page means too large: Err) allocate and init a new page, link it after this one, make it the last; then insert into the page and return its rid")
    }

    /// Replaces the metadata of the tuple at `rid` (for instance to mark it deleted).
    pub fn update_tuple_meta(&self, meta: &TupleMeta, rid: Rid) -> Result<()> {
        todo!("3c-01: write-latch the rid's page and update the slot's metadata")
    }

    /// The tuple at `rid` and its metadata (read together under one latch). The tuple carries `rid`.
    pub fn get_tuple(&self, rid: Rid) -> Result<(TupleMeta, Tuple)> {
        todo!("3c-01: read-latch the rid's page and read the slot")
    }

    /// Just the metadata of the tuple at `rid`.
    pub fn get_tuple_meta(&self, rid: Rid) -> Result<TupleMeta> {
        todo!("3c-01: read-latch the rid's page and read the slot's metadata")
    }

    /// Overwrites the tuple at `rid` and its metadata with a tuple of the same length, if `check` (when present) approves the old tuple. Returns
    /// whether it did. The check runs under the page's write latch, so nothing can change between the check and the write.
    pub fn update_tuple_in_place(&self, meta: &TupleMeta, tuple: &Tuple, rid: Rid, check: Option<&dyn Fn(&TupleMeta, &Tuple, Rid) -> bool>) -> Result<bool> {
        todo!("3c-01: write-latch the page; read the old tuple; if there is no check or the check approves, overwrite in place and say true; otherwise false")
    }

    /// Runs `f` on the table page of `rid` while holding its **read** latch, so a tuple and its version link are read together.
    pub fn with_page<R>(&self, rid: Rid, f: impl FnOnce(&TablePage<&[u8]>) -> R) -> R {
        let guard = self.bpm.read_page(rid.page_id());
        f(&TablePage::new(&guard[..]))
    }

    /// Runs `f` on the table page of `rid` while holding its **write** latch, so a tuple and its version link are changed together.
    pub fn with_page_mut<R>(&self, rid: Rid, f: impl FnOnce(&mut TablePage<&mut [u8]>) -> R) -> R {
        let mut guard = self.bpm.write_page(rid.page_id());
        f(&mut TablePage::new(&mut guard[..]))
    }

    /// An iterator over the table as it is **now**: it stops at the last tuple that exists when it is created, so a statement that inserts into the
    /// table it scans (the "Halloween problem") does not see its own output.
    pub fn make_iterator(&self) -> TableIterator<'_> {
        todo!("3c-02: remember the last page and how many tuples it has now; start at (first page, slot 0) and stop at (last page, that count)")
    }

    /// An iterator that sees tuples inserted while it runs: it goes on until the table's end.
    pub fn make_eager_iterator(&self) -> TableIterator<'_> {
        todo!("3c-02: start at (first page, slot 0) with no stopping point (an invalid rid)")
    }
}
