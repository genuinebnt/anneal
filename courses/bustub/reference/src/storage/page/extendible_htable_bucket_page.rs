//! The bottom level of an extendible hash table. A bucket is a small unsorted array of `(key, value)` pairs. Keys are unique.
//!
//! ```text
//! | size u32 | max_size u32 | (key, value) | (key, value) | ... |
//! ```

// @begin 2b-02
use std::marker::PhantomData;

use super::layout::HTABLE_BUCKET_PAGE_METADATA_SIZE;
use super::page_array::PageArray;
use super::page_bytes::*;
use crate::storage::index::fixed_size::{array_size, FixedSize};
use crate::storage::index::generic_key::KeyComparator;

pub struct ExtendibleHTableBucketPage<B, K, V> {
    page: B,
    _entry: PhantomData<(K, V)>,
}

const SIZE_OFFSET: usize = 0;
const MAX_SIZE_OFFSET: usize = 4;

impl<B, K, V> ExtendibleHTableBucketPage<B, K, V> {
    pub fn new(page: B) -> ExtendibleHTableBucketPage<B, K, V> {
        ExtendibleHTableBucketPage { page, _entry: PhantomData }
    }
}

impl<B: AsRef<[u8]>, K: FixedSize, V: FixedSize> ExtendibleHTableBucketPage<B, K, V> {
    /// How many pairs fit in a bucket page.
    pub fn capacity() -> usize {
        array_size(HTABLE_BUCKET_PAGE_METADATA_SIZE, <(K, V)>::SIZE)
    }

    pub fn size(&self) -> u32 {
        read_u32(self.page.as_ref(), SIZE_OFFSET)
    }

    pub fn max_size(&self) -> u32 {
        read_u32(self.page.as_ref(), MAX_SIZE_OFFSET)
    }

    pub fn is_full(&self) -> bool {
        self.size() >= self.max_size()
    }

    pub fn is_empty(&self) -> bool {
        self.size() == 0
    }

    fn entries(&self) -> PageArray<&[u8], (K, V)> {
        PageArray::new(&self.page.as_ref()[HTABLE_BUCKET_PAGE_METADATA_SIZE..])
    }

    /// The pair in slot `bucket_idx`. Panics past `size`.
    pub fn entry_at(&self, bucket_idx: u32) -> (K, V) {
        assert!(bucket_idx < self.size(), "slot {bucket_idx} is past the bucket's size");
        self.entries().get(bucket_idx as usize)
    }

    pub fn key_at(&self, bucket_idx: u32) -> K {
        self.entry_at(bucket_idx).0
    }

    pub fn value_at(&self, bucket_idx: u32) -> V {
        self.entry_at(bucket_idx).1
    }

    /// The value stored for `key`, if any. The array is unsorted, so this looks at every entry.
    pub fn lookup(&self, key: &K, cmp: &impl KeyComparator<K>) -> Option<V> {
        (0..self.size()).map(|i| self.entry_at(i)).find(|(k, _)| cmp.compare(k, key).is_eq()).map(|(_, v)| v)
    }
}

impl<B: AsRef<[u8]> + AsMut<[u8]>, K: FixedSize, V: FixedSize> ExtendibleHTableBucketPage<B, K, V> {
    /// Formats a fresh page: empty, with room for `max_size` pairs (at most [`capacity`](Self::capacity)).
    pub fn init(&mut self, max_size: u32) {
        assert!(max_size as usize <= Self::capacity(), "{max_size} pairs do not fit in a bucket page");
        write_u32(self.page.as_mut(), SIZE_OFFSET, 0);
        write_u32(self.page.as_mut(), MAX_SIZE_OFFSET, max_size);
    }

    fn entries_mut(&mut self) -> PageArray<&mut [u8], (K, V)> {
        PageArray::new(&mut self.page.as_mut()[HTABLE_BUCKET_PAGE_METADATA_SIZE..])
    }

    /// Adds the pair. `false` (and nothing changes) if the bucket is full or already holds `key`.
    pub fn insert(&mut self, key: &K, value: &V, cmp: &impl KeyComparator<K>) -> bool
    where
        K: Clone,
        V: Clone,
    {
        if self.is_full() || self.lookup(key, cmp).is_some() {
            return false;
        }
        let size = self.size();
        self.entries_mut().set(size as usize, &(key.clone(), value.clone()));
        write_u32(self.page.as_mut(), SIZE_OFFSET, size + 1);
        true
    }

    /// Removes the pair in slot `bucket_idx`, shifting the later ones down so the order is kept.
    pub fn remove_at(&mut self, bucket_idx: u32) {
        let size = self.size();
        assert!(bucket_idx < size, "slot {bucket_idx} is past the bucket's size");
        self.entries_mut().remove_at(bucket_idx as usize, size as usize);
        write_u32(self.page.as_mut(), SIZE_OFFSET, size - 1);
    }

    /// Removes the pair with this key. `false` if there isn't one.
    pub fn remove(&mut self, key: &K, cmp: &impl KeyComparator<K>) -> bool {
        match (0..self.size()).find(|&i| cmp.compare(&self.key_at(i), key).is_eq()) {
            Some(i) => {
                self.remove_at(i);
                true
            }
            None => false,
        }
    }
}
//~ // TODO(2b-02): your page type goes here: the table module names it, and what it stores and how is up to you.
// @end
