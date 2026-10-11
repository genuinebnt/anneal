//! A B+ tree leaf page: the `(key, value)` pairs, a link to the next leaf and, for a tree with tombstones, a small buffer of deleted keys.

// @begin 2c-01
use std::cmp::Ordering;
use std::marker::PhantomData;

use super::b_plus_tree_page::*;
use super::page_array::PageArray;
use super::page_bytes::*;
use crate::common::config::PageId;
use crate::storage::index::fixed_size::{array_size, FixedSize};
use crate::storage::index::generic_key::KeyComparator;

const NEXT_PAGE_ID_OFFSET: usize = 12;
const NUM_TOMBSTONES_OFFSET: usize = 16;

pub struct BPlusTreeLeafPage<B, K, V, const TOMBS: usize = 0> {
    page: B,
    _entry: PhantomData<(K, V)>,
}

impl<B, K, V, const TOMBS: usize> BPlusTreeLeafPage<B, K, V, TOMBS> {
    pub fn new(page: B) -> BPlusTreeLeafPage<B, K, V, TOMBS> {
        BPlusTreeLeafPage { page, _entry: PhantomData }
    }
}

impl<B: AsRef<[u8]>, K: FixedSize, V: FixedSize, const TOMBS: usize> BPlusTreeLeafPage<B, K, V, TOMBS> {
    /// Bytes between the header and the entries: none without tombstones; with them a count and room for `TOMBS` keys.
    const TOMB_REGION: usize = if TOMBS == 0 { 0 } else { 4 + TOMBS * K::SIZE };
    /// Where the entries start in the page.
    const ENTRIES_AT: usize = LEAF_PAGE_HEADER_SIZE + Self::TOMB_REGION;

    /// How many pairs fit in a leaf. BusTub's `LEAF_PAGE_SLOT_CNT`.
    pub fn capacity() -> usize {
        array_size(Self::ENTRIES_AT, <(K, V)>::SIZE)
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

    /// The next leaf to the right, or `None` for the last leaf.
    pub fn next_page_id(&self) -> Option<PageId> {
        read_optional_page_id(self.page.as_ref(), NEXT_PAGE_ID_OFFSET)
    }

    fn entries(&self) -> PageArray<&[u8], (K, V)> {
        PageArray::new(&self.page.as_ref()[Self::ENTRIES_AT..])
    }

    /// The pair in slot `index`. Panics past `size`.
    pub fn entry_at(&self, index: u32) -> (K, V) {
        assert!(index < self.size(), "slot {index} is past the page's size {}", self.size());
        self.entries().get(index as usize)
    }

    pub fn key_at(&self, index: u32) -> K {
        self.entry_at(index).0
    }

    pub fn value_at(&self, index: u32) -> V {
        self.entry_at(index).1
    }

    /// The first slot whose key is not less than `key`; `size` if every key is less.
    pub fn lower_bound(&self, key: &K, cmp: &impl KeyComparator<K>) -> u32 {
        self.entries().lower_bound(self.size() as usize, |(k, _)| cmp.compare(k, key)) as u32
    }

    /// The slot holding exactly `key`, tombstoned or not.
    pub fn find(&self, key: &K, cmp: &impl KeyComparator<K>) -> Option<u32> {
        let at = self.lower_bound(key, cmp);
        (at < self.size() && cmp.compare(&self.key_at(at), key) == Ordering::Equal).then_some(at)
    }

    /// The value stored for `key`, if there is a live (not tombstoned) pair for it.
    pub fn lookup(&self, key: &K, cmp: &impl KeyComparator<K>) -> Option<V> {
        let at = self.find(key, cmp)?;
        if self.is_deleted_at(at) {
            return None;
        }
        Some(self.value_at(at))
    }

    // ---- tombstones (module 2d) ---------------------------------------------------------------------------------------------

    /// How many tombstones the buffer holds now (always 0 without tombstones).
    pub fn num_tombstones(&self) -> u32 {
        if TOMBS == 0 {
            return 0;
        }
        read_u32(self.page.as_ref(), NUM_TOMBSTONES_OFFSET)
    }

    /// The keys with pending deletes, in order of recency (oldest first). BusTub's `GetTombstones`.
    pub fn tombstones(&self) -> Vec<K> {
        (0..self.num_tombstones() as usize)
            .map(|i| {
                let at = NUM_TOMBSTONES_OFFSET + 4 + i * K::SIZE;
                K::decode(&self.page.as_ref()[at..at + K::SIZE])
            })
            .collect()
    }

    /// Is the pair in slot `index` deleted but still in the page? (Compares the stored bytes, so no comparator is needed: the tombstone
    /// is a copy of the entry's key.)
    pub fn is_deleted_at(&self, index: u32) -> bool {
        let bytes = |k: &K| {
            let mut b = vec![0u8; K::SIZE];
            k.encode(&mut b);
            b
        };
        let mine = bytes(&self.key_at(index));
        self.tombstones().iter().any(|t| bytes(t) == mine)
    }

    /// Is the pair for `key` deleted but still in the page?
    pub fn is_tombstoned(&self, key: &K, cmp: &impl KeyComparator<K>) -> bool {
        self.tombstones().iter().any(|t| cmp.compare(t, key) == Ordering::Equal)
    }
}

impl<B: AsRef<[u8]> + AsMut<[u8]>, K: FixedSize, V: FixedSize, const TOMBS: usize> BPlusTreeLeafPage<B, K, V, TOMBS> {
    /// Formats a fresh page: a leaf with no pairs, no tombstones and no next leaf, holding at most `max_size` (not more than fits).
    pub fn init(&mut self, max_size: u32) {
        assert!(max_size as usize <= Self::capacity(), "{max_size} pairs do not fit in a leaf page");
        let mut header = BPlusTreePage::new(self.page.as_mut());
        header.set_page_type(IndexPageType::Leaf);
        header.set_size(0);
        header.set_max_size(max_size);
        self.set_next_page_id(None);
        if TOMBS > 0 {
            write_u32(self.page.as_mut(), NUM_TOMBSTONES_OFFSET, 0);
        }
    }

    pub fn set_size(&mut self, size: u32) {
        BPlusTreePage::new(self.page.as_mut()).set_size(size);
    }

    pub fn set_next_page_id(&mut self, next: Option<PageId>) {
        write_optional_page_id(self.page.as_mut(), NEXT_PAGE_ID_OFFSET, next);
    }

    fn entries_mut(&mut self) -> PageArray<&mut [u8], (K, V)> {
        PageArray::new(&mut self.page.as_mut()[Self::ENTRIES_AT..])
    }

    /// Stores the pair in slot `index`. It does not change the size.
    pub fn set_entry_at(&mut self, index: u32, key: &K, value: &V)
    where
        K: Clone,
        V: Clone,
    {
        self.entries_mut().set(index as usize, &(key.clone(), value.clone()));
    }

    /// Replaces the whole tombstone buffer (oldest first). Panics if there are more than `TOMBS`.
    pub fn set_tombstones(&mut self, keys: &[K]) {
        assert!(keys.len() <= TOMBS, "{} tombstones do not fit in a buffer of {TOMBS}", keys.len());
        for (i, key) in keys.iter().enumerate() {
            let at = NUM_TOMBSTONES_OFFSET + 4 + i * K::SIZE;
            key.encode(&mut self.page.as_mut()[at..at + K::SIZE]);
        }
        if TOMBS > 0 {
            write_u32(self.page.as_mut(), NUM_TOMBSTONES_OFFSET, keys.len() as u32);
        }
    }

    /// Notes `key` as the newest tombstone. Panics if the buffer is full: make room first (`remove_oldest_tombstone`).
    pub fn add_tombstone(&mut self, key: &K)
    where
        K: Clone,
    {
        let mut keys = self.tombstones();
        assert!(keys.len() < TOMBS, "the tombstone buffer is full");
        keys.push(key.clone());
        self.set_tombstones(&keys);
    }

    /// Forgets `key`'s tombstone, if it has one. `true` if it had.
    pub fn remove_tombstone(&mut self, key: &K, cmp: &impl KeyComparator<K>) -> bool
    where
        K: Clone,
    {
        let mut keys = self.tombstones();
        let Some(at) = keys.iter().position(|t| cmp.compare(t, key) == Ordering::Equal) else { return false };
        keys.remove(at);
        self.set_tombstones(&keys);
        true
    }

    /// Adds the pair in key order. `false` (and nothing changes) if a live pair for `key` is already in the page. A *tombstoned* pair for
    /// the same key is brought back to life with the new value (its tombstone is dropped); no slot is used. Panics if the page has no
    /// room (a tree splits a leaf the moment it reaches `max_size`, so it always has room for one more).
    pub fn insert(&mut self, key: &K, value: &V, cmp: &impl KeyComparator<K>) -> bool
    where
        K: Clone,
        V: Clone,
    {
        let size = self.size();
        let at = self.lower_bound(key, cmp);
        if at < size && cmp.compare(&self.key_at(at), key) == Ordering::Equal {
            if self.remove_tombstone(key, cmp) {
                self.set_entry_at(at, key, value);
                return true;
            }
            return false;
        }
        assert!((size as usize) < Self::capacity(), "the leaf is full");
        self.entries_mut().insert_at(at as usize, size as usize, &(key.clone(), value.clone()));
        self.set_size(size + 1);
        true
    }

    /// Physically removes the pair for `key` (and its tombstone, if it has one), keeping the order. `false` if there is none.
    pub fn remove(&mut self, key: &K, cmp: &impl KeyComparator<K>) -> bool
    where
        K: Clone,
    {
        let size = self.size();
        let at = self.lower_bound(key, cmp);
        if at >= size || cmp.compare(&self.key_at(at), key) != Ordering::Equal {
            return false;
        }
        self.remove_at(at);
        true
    }

    /// Physically removes the pair in slot `index`, keeping the order, and its tombstone if it had one.
    pub fn remove_at(&mut self, index: u32)
    where
        K: Clone,
    {
        let size = self.size();
        assert!(index < size, "slot {index} is past the page's size");
        let key = self.key_at(index);
        self.drop_tombstone_of(&key);
        self.entries_mut().remove_at(index as usize, size as usize);
        self.set_size(size - 1);
    }

    /// Drops `key` from the tombstone buffer by byte comparison of the stored key (the key is physically in the page, so it matches).
    fn drop_tombstone_of(&mut self, key: &K)
    where
        K: Clone,
    {
        if TOMBS == 0 {
            return;
        }
        let bytes = |k: &K| {
            let mut b = vec![0u8; K::SIZE];
            k.encode(&mut b);
            b
        };
        let mine = bytes(key);
        let mut keys = self.tombstones();
        if let Some(at) = keys.iter().position(|t| bytes(t) == mine) {
            keys.remove(at);
            self.set_tombstones(&keys);
        }
    }

    /// Adds the pair at the front, shifting the others right. Used when a leaf borrows from its left neighbour.
    pub fn insert_at_front(&mut self, key: &K, value: &V)
    where
        K: Clone,
        V: Clone,
    {
        let size = self.size();
        self.entries_mut().insert_at(0, size as usize, &(key.clone(), value.clone()));
        self.set_size(size + 1);
    }

    /// Deletes `key` logically: the pair stays in the page and its key becomes the newest tombstone. If the buffer is full, the oldest
    /// tombstoned pair is removed for real first. `false` if there is no live pair for `key`. Only for `TOMBS > 0`.
    pub fn remove_logically(&mut self, key: &K, cmp: &impl KeyComparator<K>) -> bool
    where
        K: Clone,
    {
        assert!(TOMBS > 0, "a leaf without a tombstone buffer deletes for real");
        let Some(at) = self.find(key, cmp) else { return false };
        if self.is_deleted_at(at) {
            return false;
        }
        if self.tombstones().len() >= TOMBS {
            let oldest = self.tombstones()[0].clone();
            self.remove(&oldest, cmp); // really removes the pair (and with it the oldest tombstone)
        }
        self.add_tombstone(key);
        true
    }

    /// Really removes every tombstoned pair (and empties the buffer). Returns how many pairs went.
    pub fn purge_tombstones(&mut self, cmp: &impl KeyComparator<K>) -> usize
    where
        K: Clone,
    {
        let keys = self.tombstones();
        for key in &keys {
            self.remove(key, cmp);
        }
        keys.len()
    }
}
//~ // TODO(2c-01): your page type goes here. The tree module (module 2c) names it, and what it stores and how is up to you.
// @end
