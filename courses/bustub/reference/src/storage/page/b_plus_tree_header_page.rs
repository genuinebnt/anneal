//! The header page of a B+ tree: the one page whose id a tree is created with, and the page that says where the root is.

// @begin 2c-01
use super::page_bytes::*;
use crate::common::config::PageId;

pub struct BPlusTreeHeaderPage<B> {
    page: B,
}

impl<B> BPlusTreeHeaderPage<B> {
    pub fn new(page: B) -> BPlusTreeHeaderPage<B> {
        BPlusTreeHeaderPage { page }
    }
}

impl<B: AsRef<[u8]>> BPlusTreeHeaderPage<B> {
    /// The root's page id, or `PageId::INVALID` for an empty tree.
    pub fn root_page_id(&self) -> PageId {
        read_page_id(self.page.as_ref(), 0)
    }
}

impl<B: AsRef<[u8]> + AsMut<[u8]>> BPlusTreeHeaderPage<B> {
    /// Formats a fresh page: an empty tree. (A zero page would say the root is page 0.)
    pub fn init(&mut self) {
        self.set_root_page_id(PageId::INVALID);
    }

    pub fn set_root_page_id(&mut self, root_page_id: PageId) {
        write_page_id(self.page.as_mut(), 0, root_page_id);
    }
}
//~ // TODO(2c-01): your page type goes here. The tree module (module 2c) names it, and what it stores and how is up to you.
// @end
