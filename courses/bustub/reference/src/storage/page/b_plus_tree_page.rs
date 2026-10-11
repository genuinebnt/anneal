//! The part of a B+ tree page that every kind of page shares.

// @begin 2c-01
use super::page_bytes::*;

/// BusTub's `IndexPageType`. The numbers are stored in the page, so they are part of the file format.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum IndexPageType {
    Invalid = 0,
    Leaf = 1,
    Internal = 2,
}

pub const PAGE_TYPE_OFFSET: usize = 0;
pub const SIZE_OFFSET: usize = 4;
pub const MAX_SIZE_OFFSET: usize = 8;
/// Bytes of the header an internal page has (the three fields above).
pub const INTERNAL_PAGE_HEADER_SIZE: usize = 12;
/// A leaf page's header adds the next leaf's page id.
pub const LEAF_PAGE_HEADER_SIZE: usize = 16;

pub struct BPlusTreePage<B> {
    page: B,
}

impl<B> BPlusTreePage<B> {
    pub fn new(page: B) -> BPlusTreePage<B> {
        BPlusTreePage { page }
    }
}

impl<B: AsRef<[u8]>> BPlusTreePage<B> {
    pub fn page_type(&self) -> IndexPageType {
        match read_u32(self.page.as_ref(), PAGE_TYPE_OFFSET) {
            1 => IndexPageType::Leaf,
            2 => IndexPageType::Internal,
            _ => IndexPageType::Invalid,
        }
    }

    pub fn is_leaf_page(&self) -> bool {
        self.page_type() == IndexPageType::Leaf
    }

    /// Entries in the page: key/value pairs in a leaf, **children** in an internal page (an internal page has one key fewer).
    pub fn size(&self) -> u32 {
        read_u32(self.page.as_ref(), SIZE_OFFSET)
    }

    pub fn max_size(&self) -> u32 {
        read_u32(self.page.as_ref(), MAX_SIZE_OFFSET)
    }

    /// The fewest entries a page that is not the root may have. A leaf: `max_size / 2`. An internal page: `(max_size + 1) / 2`, which
    /// is `ceil(max_size / 2)` children. (BusTub's `GetMinSize`; the root is exempt, see the delete stages.)
    pub fn min_size(&self) -> u32 {
        match self.page_type() {
            IndexPageType::Leaf => self.max_size() / 2,
            _ => self.max_size().div_ceil(2),
        }
    }
}

impl<B: AsRef<[u8]> + AsMut<[u8]>> BPlusTreePage<B> {
    pub fn set_page_type(&mut self, page_type: IndexPageType) {
        write_u32(self.page.as_mut(), PAGE_TYPE_OFFSET, page_type as u32);
    }

    pub fn set_size(&mut self, size: u32) {
        write_u32(self.page.as_mut(), SIZE_OFFSET, size);
    }

    /// Adds `amount` (which may be negative) to the size. Panics if that would leave a negative size.
    pub fn change_size_by(&mut self, amount: i32) {
        let size = self.size().checked_add_signed(amount).expect("a page's size cannot go below zero");
        self.set_size(size);
    }

    pub fn set_max_size(&mut self, max_size: u32) {
        write_u32(self.page.as_mut(), MAX_SIZE_OFFSET, max_size);
    }
}
//~ // TODO(2c-01): your page type goes here. The tree module (module 2c) names it, and what it stores and how is up to you.
// @end
