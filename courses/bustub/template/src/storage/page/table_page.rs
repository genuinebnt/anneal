//! A page that holds tuples. A tuple's address is its **slot number** in the page: tuples never move, so a record id (page, slot) stays valid;
//! deleting only marks the tuple. The usual layout is a **slotted page**: a slot array growing from the front and the tuples from the back.

use crate::common::config::PageId;
use crate::common::exception::Result;

// TODO(3b-03): your imports go here.
use crate::common::rid::Rid;
use crate::storage::table::tuple::{Tuple, TupleMeta};

/// A table page over `B`, the page's bytes (`&[u8]` to read, `&mut [u8]` to change).
pub struct TablePage<B> {
    _page: std::marker::PhantomData<B>,
    // TODO(3b-03): the fields are yours: the bytes of the page, and whatever else your layout needs to remember.
}

// TODO(3b-03): constants and helpers of your own go here.

impl<B> TablePage<B> {
    /// A view over the bytes of a page.
    pub fn new(page: B) -> TablePage<B> {
        todo!("3b-03: wrap the page's bytes")
    }
}

impl<B: AsRef<[u8]>> TablePage<B> {
    // TODO(3b-03): a private helper of yours.

    /// The number of tuples in the page (deleted ones included).
    pub fn get_num_tuples(&self) -> u32 {
        todo!("3b-03: the u16 at byte 4")
    }

    /// How many of them are marked deleted.
    pub fn get_num_deleted_tuples(&self) -> u32 {
        todo!("3b-03: the u16 at byte 6")
    }

    /// The next page of the table, if any.
    pub fn get_next_page_id(&self) -> Option<PageId> {
        todo!("3b-03: the id at byte 0 (INVALID is None)")
    }

    // TODO(3b-03): a private helper of yours.

    /// Where a tuple of this size would be stored (the offset of its first byte), or `None` if it does not fit: the new slot and the tuple must not
    /// overlap.
    pub fn get_next_tuple_offset(&self, _meta: &TupleMeta, tuple: &Tuple) -> Option<u16> {
        todo!("3b-03: the tuples grow down from the end of the page: the new tuple starts below the previous one (the page's end for the first); it fits if that is not below the end of the slot array including the new slot (header + 24 * (tuples + 1))")
    }

    /// The tuple in `rid`'s slot and its metadata. An out-of-range slot is an error.
    pub fn get_tuple(&self, rid: Rid) -> Result<(TupleMeta, Tuple)> {
        todo!("3b-04: Err for a slot past the last tuple; otherwise the slot's metadata and a tuple copied from its bytes (with this rid)")
    }

    /// Just the metadata of the slot.
    pub fn get_tuple_meta(&self, rid: Rid) -> Result<TupleMeta> {
        todo!("3b-04: Err for a slot past the last tuple; otherwise its metadata")
    }
}

impl<B: AsRef<[u8]> + AsMut<[u8]>> TablePage<B> {
    /// Formats a fresh page: no next page, no tuples.
    pub fn init(&mut self) {
        todo!("3b-03: no next page (INVALID), zero tuples, zero deleted")
    }

    // TODO(3b-03): a private helper of yours.

    /// Sets the next page of the table; `None` ends the chain.
    pub fn set_next_page_id(&mut self, next: Option<PageId>) {
        todo!("3b-03: remember the next page of the table")
    }

    // TODO(3b-03): a private helper of yours.

    /// Stores the tuple in the next slot, at the offset `get_next_tuple_offset` gives; returns the slot number, or `None` if it does not fit.
    pub fn insert_tuple(&mut self, meta: &TupleMeta, tuple: &Tuple) -> Option<u16> {
        todo!("3b-03: find the offset (None: no room); write the slot (offset, size, meta) and the tuple's bytes there; count one more tuple; return the slot number")
    }

    /// Replaces the slot's metadata. Marking a live tuple deleted counts it in `num_deleted_tuples`. A slot past the last tuple is an error.
    pub fn update_tuple_meta(&mut self, meta: &TupleMeta, rid: Rid) -> Result<()> {
        todo!("3b-04: Err for a bad slot; count a live tuple that becomes deleted; store the new metadata keeping the offset and size")
    }

    /// Overwrites the slot's tuple with one **of the same length**, and its metadata. "Unsafe" because nothing stops two writers: the
    /// caller must hold the page's write latch.
    pub fn update_tuple_in_place_unsafe(&mut self, meta: &TupleMeta, tuple: &Tuple, rid: Rid) -> Result<()> {
        todo!("3b-04: Err for a bad slot or a tuple of a different length; count a newly deleted tuple; store the metadata and overwrite the bytes")
    }
}
