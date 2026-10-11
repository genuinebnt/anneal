//! A page that holds tuples. A tuple's address is its **slot number** in the page: tuples never move, so a record id (page, slot) stays valid;
//! deleting only marks the tuple. The usual layout is a **slotted page**: a slot array growing from the front and the tuples from the back.

use crate::common::config::PageId;
use crate::common::exception::Result;

// @begin 3b-03
use super::page_bytes::*;
use crate::common::config::BUSTUB_PAGE_SIZE;
use crate::common::exception::{Exception, ExceptionType};
//~ // TODO(3b-03): your imports go here.
// @end
use crate::common::rid::Rid;
use crate::storage::table::tuple::{Tuple, TupleMeta};

/// A table page over `B`, the page's bytes (`&[u8]` to read, `&mut [u8]` to change).
pub struct TablePage<B> {
    // @begin 3b-03
    page: B,
    //~ _page: std::marker::PhantomData<B>,
    //~ // TODO(3b-03): the fields are yours: the bytes of the page, and whatever else your layout needs to remember.
    // @end
}

// @begin 3b-03
pub const TABLE_PAGE_HEADER_SIZE: usize = 8;
pub const TUPLE_INFO_SIZE: usize = 24;

const NEXT_PAGE_ID_OFFSET: usize = 0;
const NUM_TUPLES_OFFSET: usize = 4;
const NUM_DELETED_OFFSET: usize = 6;

fn slot_at(index: usize) -> usize {
    TABLE_PAGE_HEADER_SIZE + TUPLE_INFO_SIZE * index
}

fn out_of_range() -> Exception {
    Exception::new(ExceptionType::Invalid, "Tuple ID out of range")
}
//~ // TODO(3b-03): constants and helpers of your own go here.
// @end

impl<B> TablePage<B> {
    /// A view over the bytes of a page.
    pub fn new(page: B) -> TablePage<B> {
        // @begin 3b-03
        TablePage { page }
        //~ todo!("3b-03: wrap the page's bytes")
        // @end
    }
}

impl<B: AsRef<[u8]>> TablePage<B> {
    // @begin 3b-03
    fn bytes(&self) -> &[u8] {
        self.page.as_ref()
    }

    //~ // TODO(3b-03): a private helper of yours.
    // @end

    /// The number of tuples in the page (deleted ones included).
    pub fn get_num_tuples(&self) -> u32 {
        // @begin 3b-03
        u16::from_le_bytes(self.bytes()[NUM_TUPLES_OFFSET..NUM_TUPLES_OFFSET + 2].try_into().unwrap()) as u32
        //~ todo!("3b-03: the u16 at byte 4")
        // @end
    }

    /// How many of them are marked deleted.
    pub fn get_num_deleted_tuples(&self) -> u32 {
        // @begin 3b-03
        u16::from_le_bytes(self.bytes()[NUM_DELETED_OFFSET..NUM_DELETED_OFFSET + 2].try_into().unwrap()) as u32
        //~ todo!("3b-03: the u16 at byte 6")
        // @end
    }

    /// The next page of the table, if any.
    pub fn get_next_page_id(&self) -> Option<PageId> {
        // @begin 3b-03
        read_optional_page_id(self.bytes(), NEXT_PAGE_ID_OFFSET)
        //~ todo!("3b-03: the id at byte 0 (INVALID is None)")
        // @end
    }

    // @begin 3b-03
    fn slot(&self, index: u32) -> (u16, u16, TupleMeta) {
        let at = slot_at(index as usize);
        let b = &self.bytes()[at..at + TUPLE_INFO_SIZE];
        (
            u16::from_le_bytes([b[0], b[1]]),
            u16::from_le_bytes([b[2], b[3]]),
            TupleMeta { ts: i64::from_le_bytes(b[8..16].try_into().unwrap()), is_deleted: b[16] != 0 },
        )
    }

    //~ // TODO(3b-03): a private helper of yours.
    // @end

    /// Where a tuple of this size would be stored (the offset of its first byte), or `None` if it does not fit: the new slot and the tuple must not
    /// overlap.
    pub fn get_next_tuple_offset(&self, _meta: &TupleMeta, tuple: &Tuple) -> Option<u16> {
        // @begin 3b-03
        let n = self.get_num_tuples();
        let slot_end_offset = if n > 0 { self.slot(n - 1).0 as usize } else { BUSTUB_PAGE_SIZE };
        let tuple_offset = slot_end_offset.checked_sub(tuple.get_length() as usize)?;
        let slots_end = TABLE_PAGE_HEADER_SIZE + TUPLE_INFO_SIZE * (n as usize + 1);
        (tuple_offset >= slots_end).then_some(tuple_offset as u16)
        //~ todo!("3b-03: tuples grow down from the end of the page: the new one starts below the previous one and must not reach the slot array, counting the new slot")
        // @end
    }

    /// The tuple in `rid`'s slot and its metadata. An out-of-range slot is an error.
    pub fn get_tuple(&self, rid: Rid) -> Result<(TupleMeta, Tuple)> {
        // @begin 3b-04
        let id = rid.slot_num();
        if id >= self.get_num_tuples() {
            return Err(out_of_range());
        }
        let (offset, size, meta) = self.slot(id);
        Ok((meta, Tuple::from_bytes(rid, &self.bytes()[offset as usize..offset as usize + size as usize])))
        //~ todo!("3b-04: Err for a slot past the last tuple; otherwise the slot's metadata and a tuple copied from its bytes (with this rid)")
        // @end
    }

    /// Just the metadata of the slot.
    pub fn get_tuple_meta(&self, rid: Rid) -> Result<TupleMeta> {
        // @begin 3b-04
        let id = rid.slot_num();
        if id >= self.get_num_tuples() {
            return Err(out_of_range());
        }
        Ok(self.slot(id).2)
        //~ todo!("3b-04: Err for a slot past the last tuple; otherwise its metadata")
        // @end
    }
}

impl<B: AsRef<[u8]> + AsMut<[u8]>> TablePage<B> {
    /// Formats a fresh page: no next page, no tuples.
    pub fn init(&mut self) {
        // @begin 3b-03
        write_optional_page_id(self.page.as_mut(), NEXT_PAGE_ID_OFFSET, None);
        self.set_u16(NUM_TUPLES_OFFSET, 0);
        self.set_u16(NUM_DELETED_OFFSET, 0);
        //~ todo!("3b-03: no next page (INVALID), zero tuples, zero deleted")
        // @end
    }

    // @begin 3b-03
    fn set_u16(&mut self, at: usize, value: u16) {
        self.page.as_mut()[at..at + 2].copy_from_slice(&value.to_le_bytes());
    }

    //~ // TODO(3b-03): a private helper of yours.
    // @end

    /// Sets the next page of the table; `None` ends the chain.
    pub fn set_next_page_id(&mut self, next: Option<PageId>) {
        // @begin 3b-03
        write_optional_page_id(self.page.as_mut(), NEXT_PAGE_ID_OFFSET, next);
        //~ todo!("3b-03: remember the next page of the table")
        // @end
    }

    // @begin 3b-03
    fn set_slot(&mut self, index: u32, offset: u16, size: u16, meta: &TupleMeta) {
        let at = slot_at(index as usize);
        let b = &mut self.page.as_mut()[at..at + TUPLE_INFO_SIZE];
        b[0..2].copy_from_slice(&offset.to_le_bytes());
        b[2..4].copy_from_slice(&size.to_le_bytes());
        b[8..16].copy_from_slice(&meta.ts.to_le_bytes());
        b[16] = meta.is_deleted as u8;
    }

    //~ // TODO(3b-03): a private helper of yours.
    // @end

    /// Stores the tuple in the next slot, at the offset `get_next_tuple_offset` gives; returns the slot number, or `None` if it does not fit.
    pub fn insert_tuple(&mut self, meta: &TupleMeta, tuple: &Tuple) -> Option<u16> {
        // @begin 3b-03
        let offset = self.get_next_tuple_offset(meta, tuple)?;
        let id = self.get_num_tuples();
        self.set_slot(id, offset, tuple.get_length() as u16, meta);
        self.set_u16(NUM_TUPLES_OFFSET, id as u16 + 1);
        self.page.as_mut()[offset as usize..offset as usize + tuple.data().len()].copy_from_slice(tuple.data());
        Some(id as u16)
        //~ todo!("3b-03: stores the tuple in the next slot at the offset it fits at; None if it does not fit; the tuple count grows by one")
        // @end
    }

    /// Replaces the slot's metadata. Marking a live tuple deleted counts it in `num_deleted_tuples`. A slot past the last tuple is an error.
    pub fn update_tuple_meta(&mut self, meta: &TupleMeta, rid: Rid) -> Result<()> {
        // @begin 3b-04
        let id = rid.slot_num();
        if id >= self.get_num_tuples() {
            return Err(out_of_range());
        }
        let (offset, size, old) = self.slot(id);
        if !old.is_deleted && meta.is_deleted {
            self.set_u16(NUM_DELETED_OFFSET, self.get_num_deleted_tuples() as u16 + 1);
        }
        self.set_slot(id, offset, size, meta);
        Ok(())
        //~ todo!("3b-04: Err for a bad slot; count a live tuple that becomes deleted; store the new metadata keeping the offset and size")
        // @end
    }

    /// Overwrites the slot's tuple with one **of the same length**, and its metadata. "Unsafe" because nothing stops two writers: the
    /// caller must hold the page's write latch.
    pub fn update_tuple_in_place_unsafe(&mut self, meta: &TupleMeta, tuple: &Tuple, rid: Rid) -> Result<()> {
        // @begin 3b-04
        let id = rid.slot_num();
        if id >= self.get_num_tuples() {
            return Err(out_of_range());
        }
        let (offset, size, old) = self.slot(id);
        if size as u32 != tuple.get_length() {
            return Err(Exception::new(ExceptionType::Invalid, "Tuple size mismatch"));
        }
        if !old.is_deleted && meta.is_deleted {
            self.set_u16(NUM_DELETED_OFFSET, self.get_num_deleted_tuples() as u16 + 1);
        }
        self.set_slot(id, offset, size, meta);
        self.page.as_mut()[offset as usize..offset as usize + size as usize].copy_from_slice(tuple.data());
        Ok(())
        //~ todo!("3b-04: Err for a bad slot or a tuple of a different length; count a newly deleted tuple; store the metadata and overwrite the bytes")
        // @end
    }
}
