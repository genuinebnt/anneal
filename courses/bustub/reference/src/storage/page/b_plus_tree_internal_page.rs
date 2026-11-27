//! A B+ tree internal page: separator keys and the pages below them.

// @begin 2c-01
use std::marker::PhantomData;

use super::b_plus_tree_page::*;
use super::page_array::PageArray;
use crate::common::config::PageId;
use crate::storage::index::fixed_size::{array_size, FixedSize};
use crate::storage::index::generic_key::KeyComparator;

pub struct BPlusTreeInternalPage<B, K> {
    page: B,
    _key: PhantomData<K>,
}

impl<B, K> BPlusTreeInternalPage<B, K> {
    pub fn new(page: B) -> BPlusTreeInternalPage<B, K> {
        BPlusTreeInternalPage { page, _key: PhantomData }
    }
}

impl<B: AsRef<[u8]>, K: FixedSize> BPlusTreeInternalPage<B, K> {
    /// How many `(key, child)` pairs fit in a page. BusTub's `INTERNAL_PAGE_SLOT_CNT`.
    pub fn capacity() -> usize {
        array_size(INTERNAL_PAGE_HEADER_SIZE, <(K, PageId)>::SIZE)
    }

    pub fn size(&self) -> u32 {
        BPlusTreePage::new(self.page.as_ref()).size()
    }

    pub fn max_size(&self) -> u32 {
        BPlusTreePage::new(self.page.as_ref()).max_size()
    }

    pub fn min_size(&self) -> u32 {
        BPlusTreePage::new(self.page.as_ref()).min_size()
    }

    fn entries(&self) -> PageArray<&[u8], (K, PageId)> {
        PageArray::new(&self.page.as_ref()[INTERNAL_PAGE_HEADER_SIZE..])
    }

    /// The pair in slot `index` (slot 0's key is meaningless). Panics past `size`.
    pub fn entry_at(&self, index: u32) -> (K, PageId) {
        assert!(index < self.size(), "slot {index} is past the page's size {}", self.size());
        self.entries().get(index as usize)
    }

    pub fn key_at(&self, index: u32) -> K {
        self.entry_at(index).0
    }

    pub fn value_at(&self, index: u32) -> PageId {
        self.entry_at(index).1
    }

    /// The slot holding child `value`, if any. BusTub's `ValueIndex` (it returns -1 when missing).
    pub fn value_index(&self, value: PageId) -> Option<u32> {
        (0..self.size()).find(|&i| self.value_at(i) == value)
    }

    /// The child to follow when searching for `key`: the child `i` with `key(i) <= key < key(i + 1)`, i.e. the last slot whose key is
    /// at most `key` (slot 0 if there is none). The keys of slots `1..size` are sorted; slot 0's key is not looked at.
    pub fn child_for(&self, key: &K, cmp: &impl KeyComparator<K>) -> PageId {
        let keys = PageArray::<_, (K, PageId)>::new(&self.page.as_ref()[INTERNAL_PAGE_HEADER_SIZE + <(K, PageId)>::SIZE..]);
        // the first of keys 1..size that is greater than `key`; the child before it is the one to follow
        let first_greater = keys.lower_bound(self.size() as usize - 1, |(k, _)| {
            if cmp.compare(k, key).is_le() {
                std::cmp::Ordering::Less
            } else {
                std::cmp::Ordering::Greater
            }
        });
        self.value_at(first_greater as u32)
    }
}

impl<B: AsRef<[u8]> + AsMut<[u8]>, K: FixedSize> BPlusTreeInternalPage<B, K> {
    /// Formats a fresh page: an internal page with no children, holding at most `max_size`.
    pub fn init(&mut self, max_size: u32) {
        assert!(max_size as usize <= Self::capacity(), "{max_size} children do not fit in an internal page");
        let mut header = BPlusTreePage::new(self.page.as_mut());
        header.set_page_type(IndexPageType::Internal);
        header.set_size(0);
        header.set_max_size(max_size);
    }

    pub fn set_size(&mut self, size: u32) {
        BPlusTreePage::new(self.page.as_mut()).set_size(size);
    }

    fn entries_mut(&mut self) -> PageArray<&mut [u8], (K, PageId)> {
        PageArray::new(&mut self.page.as_mut()[INTERNAL_PAGE_HEADER_SIZE..])
    }

    /// Stores the pair in slot `index`. It does not change the size.
    pub fn set_entry_at(&mut self, index: u32, key: &K, value: PageId)
    where
        K: Clone,
    {
        self.entries_mut().set(index as usize, &(key.clone(), value));
    }

    pub fn set_key_at(&mut self, index: u32, key: &K)
    where
        K: Clone,
    {
        let value = self.entries().get(index as usize).1;
        self.set_entry_at(index, key, value);
    }

    pub fn set_value_at(&mut self, index: u32, value: PageId)
    where
        K: Clone,
    {
        let key = self.entries().get(index as usize).0;
        self.set_entry_at(index, &key, value);
    }

    /// Adds the pair `(key, child)` where it belongs: after the last key that is at most `key`. The new key is the lower bound of
    /// `child`, which therefore sits to the right of the child that was split. Panics if the page is full or `key` is already there.
    pub fn insert_child(&mut self, key: &K, child: PageId, cmp: &impl KeyComparator<K>)
    where
        K: Clone,
    {
        let size = self.size();
        assert!(size < self.max_size(), "the page is full: split it instead");
        let at = {
            let keys = PageArray::<_, (K, PageId)>::new(&self.page.as_ref()[INTERNAL_PAGE_HEADER_SIZE + <(K, PageId)>::SIZE..]);
            keys.lower_bound(size as usize - 1, |(k, _)| {
                if cmp.compare(k, key).is_le() {
                    std::cmp::Ordering::Less
                } else {
                    std::cmp::Ordering::Greater
                }
            }) as u32
                + 1
        };
        self.entries_mut().insert_at(at as usize, size as usize, &(key.clone(), child));
        self.set_size(size + 1);
    }

    /// Removes the pair in slot `index`, shifting the later ones down. Removing slot 0 makes the next pair slot 0, so its key
    /// becomes the unused one.
    pub fn remove_at(&mut self, index: u32) {
        let size = self.size();
        assert!(index < size, "slot {index} is past the page's size");
        self.entries_mut().remove_at(index as usize, size as usize);
        self.set_size(size - 1);
    }

    /// Adds a pair at the front, shifting the others right: the new pair becomes slot 0 (its key is the unused one). Used when an
    /// internal page borrows a child from its left neighbour.
    pub fn insert_at_front(&mut self, key: &K, child: PageId)
    where
        K: Clone,
    {
        let size = self.size();
        self.entries_mut().insert_at(0, size as usize, &(key.clone(), child));
        self.set_size(size + 1);
    }
}
//~ // TODO(2c-01): your page type goes here. The tree module (module 2c) names it, and what it stores and how is up to you.
// @end
