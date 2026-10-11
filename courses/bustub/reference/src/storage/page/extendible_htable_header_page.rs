//! The top of an extendible hash table. It maps the **upper bits** of a hash to one of up to 2^max_depth directory pages, so a table can have many
//! directories, each growing independently.
//!
//! ```text
//! | directory_page_ids [i32; 512] | max_depth u32 |
//! ```
//!
//! The view reads and writes these fields by offset in the page's bytes.

// @begin 2b-02
use super::layout::*;
use super::page_bytes::*;
use crate::common::config::PageId;

/// A view of a page as a header page. `B` is the page's bytes: `&[u8]` to read, `&mut [u8]` to read and write.
pub struct ExtendibleHTableHeaderPage<B> {
    page: B,
}

impl<B> ExtendibleHTableHeaderPage<B> {
    pub fn new(page: B) -> ExtendibleHTableHeaderPage<B> {
        ExtendibleHTableHeaderPage { page }
    }
}

impl<B: AsRef<[u8]>> ExtendibleHTableHeaderPage<B> {
    pub fn max_depth(&self) -> u32 {
        read_u32(self.page.as_ref(), HEADER_MAX_DEPTH_OFFSET)
    }

    /// How many directory slots the header uses: 2^max_depth.
    pub fn max_size(&self) -> u32 {
        1 << self.max_depth()
    }

    /// The directory page id in slot `directory_idx` (`PageId::INVALID` if there isn't one yet).
    pub fn get_directory_page_id(&self, directory_idx: u32) -> PageId {
        assert!(directory_idx < self.max_size(), "directory slot {directory_idx} is out of range");
        read_page_id(self.page.as_ref(), HEADER_DIRECTORY_PAGE_IDS_OFFSET + 4 * directory_idx as usize)
    }

    /// Which directory a hash belongs to: its **top** `max_depth` bits. With max depth 0 there is one directory, index 0.
    pub fn hash_to_directory_index(&self, hash: u32) -> u32 {
        match self.max_depth() {
            0 => 0,
            depth => hash >> (32 - depth),
        }
    }
}

impl<B: AsRef<[u8]> + AsMut<[u8]>> ExtendibleHTableHeaderPage<B> {
    /// Formats a fresh page: the given max depth, and every directory slot empty (`INVALID`). Panics if `max_depth` is more than 9.
    pub fn init(&mut self, max_depth: u32) {
        assert!(max_depth <= HTABLE_HEADER_MAX_DEPTH, "max depth {max_depth} does not fit in a header page");
        let page = self.page.as_mut();
        write_u32(page, HEADER_MAX_DEPTH_OFFSET, max_depth);
        for slot in 0..HTABLE_HEADER_ARRAY_SIZE {
            write_page_id(page, HEADER_DIRECTORY_PAGE_IDS_OFFSET + 4 * slot, PageId::INVALID);
        }
    }

    pub fn set_directory_page_id(&mut self, directory_idx: u32, directory_page_id: PageId) {
        assert!(directory_idx < self.max_size(), "directory slot {directory_idx} is out of range");
        write_page_id(self.page.as_mut(), HEADER_DIRECTORY_PAGE_IDS_OFFSET + 4 * directory_idx as usize, directory_page_id);
    }
}
//~ // TODO(2b-02): your page type goes here: the table module names it, and what it stores and how is up to you.
// @end
