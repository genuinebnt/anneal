//! A sequential scan of a table heap. It is a cursor of a [`Rid`] and walks slot by slot and page by page along the chain. It returns **every**
//! tuple, deleted ones included (with their metadata): skipping the deleted is the caller's job.

use super::table_heap::TableHeap;
use super::tuple::{Tuple, TupleMeta};
use crate::common::config::PageId;
use crate::common::exception::{Exception, ExceptionType, Result};
use crate::common::rid::Rid;
use crate::storage::page::table_page::TablePage;

pub struct TableIterator<'a> {
    // @begin 3c-02
    heap: &'a TableHeap<'a>,
    /// The tuple the iterator is at; a rid in an invalid page is the end.
    rid: Rid,
    /// Where to stop (exclusive); an invalid page means "at the end of the table, whenever that is".
    stop_at_rid: Rid,
    //~ _heap: std::marker::PhantomData<&'a TableHeap<'a>>,
    //~ // TODO(3c-02): the fields are yours: the heap, where the cursor is, and where it must stop.
    // @end
}

impl<'a> TableIterator<'a> {
    /// A cursor at `rid`. If `rid` does not name a tuple (a new, empty table) the iterator is already at the end.
    pub fn new(heap: &'a TableHeap<'a>, rid: Rid, stop_at_rid: Rid) -> TableIterator<'a> {
        // @begin 3c-02
        let mut it = TableIterator { heap, rid, stop_at_rid };
        if !rid.page_id().is_valid() {
            it.rid = Rid::new(PageId::INVALID, 0);
        } else {
            let guard = heap.bpm.read_page(rid.page_id());
            if rid.slot_num() >= TablePage::new(&guard[..]).get_num_tuples() {
                it.rid = Rid::new(PageId::INVALID, 0);
            }
        }
        it
        //~ todo!("3c-02: remember the heap and the two rids; if the start does not name an existing tuple the iterator starts at the end (an invalid rid)")
        // @end
    }

    /// The tuple the iterator is at, with its metadata. An error at the end.
    pub fn get_tuple(&self) -> Result<(TupleMeta, Tuple)> {
        // @begin 3c-02
        if self.is_end() {
            return Err(Exception::new(ExceptionType::Invalid, "the iterator is at the end"));
        }
        self.heap.get_tuple(self.rid)
        //~ todo!("3c-02: an error at the end; otherwise the heap's tuple at the current rid")
        // @end
    }

    /// The record id the cursor is at.
    pub fn get_rid(&self) -> Rid {
        // @begin 3c-02
        self.rid
        //~ todo!("3c-02: the record id the cursor is at")
        // @end
    }

    /// Whether the cursor is past the last tuple.
    pub fn is_end(&self) -> bool {
        // @begin 3c-02
        !self.rid.page_id().is_valid()
        //~ todo!("3c-02: the iterator is at the end when its rid is in no page")
        // @end
    }

    /// Moves to the next tuple: the next slot of the page, or slot 0 of the next page, or the end; and the end if it reaches the stopping rid.
    pub fn advance(&mut self) {
        // @begin 3c-02
        if self.is_end() {
            return;
        }
        let guard = self.heap.bpm.read_page(self.rid.page_id());
        let page = TablePage::new(&guard[..]);
        let next_tuple_id = self.rid.slot_num() + 1;
        self.rid = Rid::new(self.rid.page_id(), next_tuple_id);
        if self.rid == self.stop_at_rid {
            self.rid = Rid::new(PageId::INVALID, 0);
        } else if next_tuple_id < page.get_num_tuples() {
            // still in this page
        } else {
            // the next page's first tuple, or the end if there is no next page
            self.rid = Rid::new(page.get_next_page_id().unwrap_or(PageId::INVALID), 0);
        }
        //~ todo!("3c-02: at the end do nothing; slot + 1; stop (the end) if that is the stopping rid; if the page has more tuples stay; otherwise go to slot 0 of the next page, or the end")
        // @end
    }
}

/// `for (meta, tuple) in heap.make_iterator()`: every tuple from the cursor to the end, deleted ones included.
impl Iterator for TableIterator<'_> {
    type Item = (TupleMeta, Tuple);

    fn next(&mut self) -> Option<(TupleMeta, Tuple)> {
        // @begin 3c-02
        if self.is_end() {
            return None;
        }
        let item = self.get_tuple().ok()?;
        self.advance();
        Some(item)
        //~ todo!("3c-02: None at the end; otherwise the current tuple, after moving on")
        // @end
    }
}
