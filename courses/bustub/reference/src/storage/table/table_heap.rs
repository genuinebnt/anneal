//! A table on disk: a linked list of table pages in the buffer pool. A tuple is added to the last page, and when that page is full a new page is
//! allocated and linked after it. A tuple is found by its [`Rid`] (page, slot); because slots never move, a rid stays valid.

// @begin 3c-01
use std::sync::Mutex;
//~ // TODO(3c-01): your imports go here.
// @end

use super::table_iterator::TableIterator;
use super::tuple::{Tuple, TupleMeta};
use crate::buffer::buffer_pool_manager::BufferPoolManager;
use crate::common::config::PageId;
use crate::common::exception::{Exception, ExceptionType, Result};
use crate::common::rid::Rid;
use crate::storage::page::table_page::TablePage;

pub struct TableHeap<'a> {
    pub(crate) bpm: &'a BufferPoolManager,
    // @begin 3c-01
    first_page_id: PageId,
    /// The id of the last page of the table; the lock also serialises inserts (one at a time, or two inserters could both extend the table).
    last_page_id: Mutex<PageId>,
    //~ // TODO(3c-01): the fields are yours: where the table starts, where it ends, and what makes inserts take turns.
    // @end
}

impl<'a> TableHeap<'a> {
    /// Creates an empty table: allocates the first page and formats it as a table page.
    pub fn new(bpm: &'a BufferPoolManager) -> TableHeap<'a> {
        // @begin 3c-01
        let first_page_id = bpm.new_page();
        TablePage::new(&mut bpm.write_page(first_page_id)[..]).init();
        TableHeap { bpm, first_page_id, last_page_id: Mutex::new(first_page_id) }
        //~ todo!("3c-01: allocate a page for the first table page and init it; it is also the last page; remember the pool")
        // @end
    }

    /// The id of the first page of the table.
    pub fn get_first_page_id(&self) -> PageId {
        // @begin 3c-01
        self.first_page_id
        //~ todo!("3c-01: the id of the first page")
        // @end
    }

    /// Adds the tuple at the end of the table and returns where it went. If the last page has no room, a new page is allocated and linked
    /// after it (and becomes the last page). A tuple that does not fit even an empty page is an error ("tuple is too large, cannot
    /// insert"). Inserts are serialised by the heap's lock.
    pub fn insert_tuple(&self, meta: &TupleMeta, tuple: &Tuple) -> Result<Rid> {
        // @begin 3c-01
        let mut last_page_id = self.last_page_id.lock().unwrap();
        let mut page_guard = self.bpm.write_page(*last_page_id);
        loop {
            let mut page = TablePage::new(&mut page_guard[..]);
            if page.get_next_tuple_offset(meta, tuple).is_some() {
                break;
            }
            // an empty page that cannot take it: no page ever will
            if page.get_num_tuples() == 0 {
                return Err(Exception::new(ExceptionType::Invalid, "tuple is too large, cannot insert"));
            }
            let next_page_id = self.bpm.new_page();
            page.set_next_page_id(Some(next_page_id));
            let mut next_guard = self.bpm.write_page(next_page_id);
            TablePage::new(&mut next_guard[..]).init();
            *last_page_id = next_page_id;
            page_guard = next_guard; // the old page's guard is dropped here
        }
        let slot = TablePage::new(&mut page_guard[..]).insert_tuple(meta, tuple).expect("the tuple fits: it was checked above");
        Ok(Rid::new(*last_page_id, slot as u32))
        //~ todo!("3c-01: hold the heap's lock; write-latch the last page; while the tuple does not fit: (an empty page means too large: Err) allocate and init a new page, link it after this one, make it the last; then insert into the page and return its rid")
        // @end
    }

    /// Replaces the metadata of the tuple at `rid` (for instance to mark it deleted).
    pub fn update_tuple_meta(&self, meta: &TupleMeta, rid: Rid) -> Result<()> {
        // @begin 3c-01
        let mut guard = self.bpm.write_page(rid.page_id());
        TablePage::new(&mut guard[..]).update_tuple_meta(meta, rid)
        //~ todo!("3c-01: write-latch the rid's page and update the slot's metadata")
        // @end
    }

    /// The tuple at `rid` and its metadata (read together under one latch). The tuple carries `rid`.
    pub fn get_tuple(&self, rid: Rid) -> Result<(TupleMeta, Tuple)> {
        // @begin 3c-01
        let guard = self.bpm.read_page(rid.page_id());
        TablePage::new(&guard[..]).get_tuple(rid)
        //~ todo!("3c-01: read-latch the rid's page and read the slot")
        // @end
    }

    /// Just the metadata of the tuple at `rid`.
    pub fn get_tuple_meta(&self, rid: Rid) -> Result<TupleMeta> {
        // @begin 3c-01
        let guard = self.bpm.read_page(rid.page_id());
        TablePage::new(&guard[..]).get_tuple_meta(rid)
        //~ todo!("3c-01: read-latch the rid's page and read the slot's metadata")
        // @end
    }

    /// Overwrites the tuple at `rid` and its metadata with a tuple of the same length, if `check` (when present) approves the old tuple. Returns
    /// whether it did. The check runs under the page's write latch, so nothing can change between the check and the write.
    pub fn update_tuple_in_place(&self, meta: &TupleMeta, tuple: &Tuple, rid: Rid, check: Option<&dyn Fn(&TupleMeta, &Tuple, Rid) -> bool>) -> Result<bool> {
        // @begin 3c-01
        let mut guard = self.bpm.write_page(rid.page_id());
        let mut page = TablePage::new(&mut guard[..]);
        let (old_meta, old_tuple) = page.get_tuple(rid)?;
        if check.map_or(true, |check| check(&old_meta, &old_tuple, rid)) {
            page.update_tuple_in_place_unsafe(meta, tuple, rid)?;
            return Ok(true);
        }
        Ok(false)
        //~ todo!("3c-01: write-latch the page; read the old tuple; if there is no check or the check approves, overwrite in place and say true; otherwise false")
        // @end
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
        // @begin 3c-02
        let last_page_id = *self.last_page_id.lock().unwrap();
        let num_tuples = TablePage::new(&self.bpm.read_page(last_page_id)[..]).get_num_tuples();
        TableIterator::new(self, Rid::new(self.first_page_id, 0), Rid::new(last_page_id, num_tuples))
        //~ todo!("3c-02: remember the last page and how many tuples it has now; start at (first page, slot 0) and stop at (last page, that count)")
        // @end
    }

    /// An iterator that sees tuples inserted while it runs: it goes on until the table's end.
    pub fn make_eager_iterator(&self) -> TableIterator<'_> {
        // @begin 3c-02
        TableIterator::new(self, Rid::new(self.first_page_id, 0), Rid::default())
        //~ todo!("3c-02: start at (first page, slot 0) with no stopping point (an invalid rid)")
        // @end
    }
}
