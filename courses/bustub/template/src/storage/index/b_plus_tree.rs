//! A B+ tree index whose nodes are pages in the buffer pool. Internal pages direct the search; leaf pages hold the `(key, value)` pairs and are
//! chained left to right for range scans. Keys are unique.
//!
//! A node is split when it overflows (a leaf the moment it holds `leaf_max_size` pairs, an internal page when it would need more than
//! `internal_max_size` children) and merged or topped up from a sibling when it underflows, so the tree grows and shrinks at the root and every
//! leaf is at the same depth.
//!
//! Latching is by guards, top down. Readers hold a parent until the child is latched ("latch crabbing"). Writers keep write guards on the whole
//! path (`Context::write_set`) until a child that cannot split or underflow is reached ("safe"), which lets them let go of everything above it;
//! and most writes try an optimistic path first that write-latches only the leaf.

use std::cmp::Ordering;
use std::marker::PhantomData;

use super::index_iterator::IndexIterator;
use crate::buffer::buffer_pool_manager::BufferPoolManager;
use crate::buffer::traced_buffer_pool_manager::TracedBufferPoolManager;
use crate::common::config::PageId;
use crate::storage::index::fixed_size::FixedSize;
use crate::storage::index::generic_key::KeyComparator;

// TODO(2c-01): your imports go here (your page types and the guards).

// TODO(2c-01): private types of yours go here (a write operation needs to remember the guards it holds).

/// A B+ tree over the buffer pool. `TOMBS` is the size of each leaf's tombstone buffer; 0 means deletes are physical.
pub struct BPlusTree<'a, K, V, C, const TOMBS: usize = 0> {
    /// The buffer pool the tree reads and writes through. It is public so that the pages the tree latched can be counted; every page access goes
    /// through it.
    pub bpm: TracedBufferPoolManager<'a>,
    _tree: PhantomData<(K, V, C)>,
    // TODO(2c-01): the fields are yours: the name, the comparator, the two sizes and the header page's id.
}

impl<'a, K, V, C, const TOMBS: usize> BPlusTree<'a, K, V, C, TOMBS>
where
    K: FixedSize + Clone,
    V: FixedSize + Clone,
    C: KeyComparator<K>,
{
    /// Creates an empty tree whose header page is `header_page_id` (the caller allocated it) and formats that page.
    /// A leaf holds at most `leaf_max_size` pairs (at least 2), an internal page at most `internal_max_size` children (at least 3).
    pub fn new(index_name: &str, header_page_id: PageId, bpm: &'a BufferPoolManager, cmp: C, leaf_max_size: u32, internal_max_size: u32) -> Self {
        todo!("2c-01: checks the sizes are usable, remembers the parameters and formats the header page as an empty tree")
    }

    /// As many pairs as fit in a leaf page.
    pub fn default_leaf_max_size() -> u32 {
        todo!("2c-01: how many pairs fit in one of your leaf pages")
    }

    /// As many children as fit in an internal page.
    pub fn default_internal_max_size() -> u32 {
        todo!("2c-01: how many children fit in one of your internal pages")
    }

    /// The name the tree was created with.
    pub fn index_name(&self) -> &str {
        todo!("2c-01: the name the tree was created with")
    }

    /// Whether the tree has no root.
    pub fn is_empty(&self) -> bool {
        todo!("2c-01: the tree is empty when the header names no root")
    }

    /// The root's page id; `PageId::INVALID` for an empty tree.
    pub fn get_root_page_id(&self) -> PageId {
        todo!("2c-01: read the root id from the header page (under its read latch)")
    }

    /// How many levels the tree has: 0 for an empty tree, 1 for a lone root leaf. Read-only.
    pub fn depth(&self) -> usize {
        todo!("2c-01: 0 for an empty tree; otherwise follow the first child from the root down to a leaf and count the levels")
    }

    /// How many `(key, value)` pairs each leaf holds physically (live or, with tombstones, deleted), from the leftmost leaf to the rightmost,
    /// following the links between leaves. Empty for an empty tree. Read-only.
    pub fn leaf_sizes(&self) -> Vec<usize> {
        todo!("2c-01: walk the leaves left to right (the leaf links) and collect how many pairs each holds")
    }

    /// The keys physically stored in each leaf, in order, tombstoned ones included, from the leftmost leaf to the rightmost. Read-only.
    pub fn leaf_keys(&self) -> Vec<Vec<K>> {
        Vec::new() // TODO(2d-01): the keys of every leaf, tombstoned ones included, left to right
    }

    /// The keys in each leaf's tombstone buffer, oldest first, for every leaf from left to right. Empty lists when `TOMBS` is 0. Read-only.
    pub fn leaf_tombstones(&self) -> Vec<Vec<K>> {
        Vec::new() // TODO(2d-01): the tombstone buffer of every leaf, left to right
    }

    // ------------------------------------------------------------------------------------------------------------------------
    // Search
    // ------------------------------------------------------------------------------------------------------------------------

    // TODO(2c-01): a private helper of yours: the stage page says what it is for. Its name and shape are your choice.

    /// The value stored for `key`: empty or one element, since keys are unique.
    pub fn get_value(&self, key: &K) -> Vec<V> {
        todo!("2c-01: find the leaf and look the key up in it")
    }

    // ------------------------------------------------------------------------------------------------------------------------
    // Insert
    // ------------------------------------------------------------------------------------------------------------------------

    /// Adds the pair. `false` (and nothing changes) if the key is already in the tree.
    pub fn insert(&self, key: &K, value: &V) -> bool {
        todo!("2c-01: latch the header; an empty tree gets a root leaf; otherwise latch down to the leaf (descend_for_write) and insert there")
    }

    // TODO(2c-01): a private helper of yours: the stage page says what it is for. Its name and shape are your choice.

    // TODO(2c-05): a private helper of yours: the stage page says what it is for. Its name and shape are your choice.

    // TODO(2c-02): a private helper of yours: the stage page says what it is for. Its name and shape are your choice.

    // ------------------------------------------------------------------------------------------------------------------------
    // Remove
    // ------------------------------------------------------------------------------------------------------------------------

    /// Removes the pair with this key, if there is one.
    pub fn remove(&self, key: &K) {
        todo!("2c-02: removes the key from its leaf; an emptied root leaf makes the tree empty, and a leaf below its minimum size is rebalanced")
    }

    // TODO(2c-05): a private helper of yours: the stage page says what it is for. Its name and shape are your choice.

    // TODO(2c-04): a private helper of yours: the stage page says what it is for. Its name and shape are your choice.

    // TODO(2c-04): a private helper of yours: the stage page says what it is for. Its name and shape are your choice.

    // TODO(2c-04): a private helper of yours: the stage page says what it is for. Its name and shape are your choice.

    // TODO(2c-04): a private helper of yours: the stage page says what it is for. Its name and shape are your choice.

    // ------------------------------------------------------------------------------------------------------------------------
    // Optimistic paths: assume nothing above the leaf changes
    // ------------------------------------------------------------------------------------------------------------------------

    // TODO(2c-05): a private helper of yours: the stage page says what it is for. Its name and shape are your choice.

    // TODO(2c-05): a private helper of yours: the stage page says what it is for. Its name and shape are your choice.

    // TODO(2c-05): a private helper of yours: the stage page says what it is for. Its name and shape are your choice.

    // ------------------------------------------------------------------------------------------------------------------------
    // Scans
    // ------------------------------------------------------------------------------------------------------------------------

    /// An iterator at the smallest key.
    pub fn begin(&self) -> IndexIterator<'a, K, V, TOMBS> {
        todo!("2c-01: the leftmost leaf, slot 0; an empty tree has no leaf, so its begin is its end")
    }

    /// An iterator at the first key that is not less than `key`.
    pub fn begin_at(&self, key: &K) -> IndexIterator<'a, K, V, TOMBS> {
        todo!("2c-01: the leaf for the key, at the first slot whose key is not less than it (IndexIterator::at moves on if that is past the leaf's end)")
    }

    /// The iterator past the last pair.
    pub fn end(&self) -> IndexIterator<'a, K, V, TOMBS> {
        todo!("2c-01: IndexIterator::end")
    }
}
