//! A range scan over the leaves of a B+ tree. It remembers a position (a leaf page and a slot in it) and, when asked for the next pair, latches
//! the leaf just long enough to copy the pair out. Nothing stays latched between calls, so a scan never blocks writers for longer than one read.
//! The tree may change between two calls: a scan never returns a key twice or out of order, but it may miss a key that a writer moved or
//! added while it ran.
//!
//! It is an `Iterator` (`for (k, v) in tree.begin()`); [`IndexIterator::is_end`] and `==` are there for callers that compare against the end.

use crate::storage::index::fixed_size::FixedSize;

// @begin 2c-04
use std::marker::PhantomData;

use crate::buffer::buffer_pool_manager::BufferPoolManager;
use crate::common::config::PageId;
// @begin 2e-03
use crate::storage::index::generic_key::KeyComparator;
// @end
use crate::storage::page::b_plus_tree_leaf_page::BPlusTreeLeafPage;
use crate::storage::page::b_plus_tree_page::BPlusTreePage;
//~ // TODO(2c-04): your imports go here.
// @end

pub struct IndexIterator<'a, K, V, const TOMBS: usize = 0> {
    // @begin 2c-04
    bpm: &'a BufferPoolManager,
    /// The leaf holding the current pair; `None` once the scan is past the last pair (the end).
    page_id: Option<PageId>,
    /// The slot of the current pair in that leaf. Always a real slot unless `page_id` is `None`.
    index: u32,
    // @begin 2e-03
    /// The last key returned, and the order to compare the next one with: a pair at or before it was returned already.
    last: Option<K>,
    cmp: Option<Box<dyn KeyComparator<K> + 'a>>,
    // @end
    _entry: PhantomData<(K, V)>,
    //~ _iterator: std::marker::PhantomData<(&'a (), K, V)>,
    //~ // TODO(2c-04): the fields are yours: where the scan is, and what it needs to read the next pair.
    // @end
}

// @begin 2c-04
impl<'a, K: FixedSize + Clone, V: FixedSize + Clone, const TOMBS: usize> IndexIterator<'a, K, V, TOMBS> {
    /// The end: past the last pair of the last leaf.
    pub fn end(bpm: &'a BufferPoolManager) -> IndexIterator<'a, K, V, TOMBS> {
        IndexIterator {
            bpm,
            page_id: None,
            index: 0,
            // @begin 2e-03
            last: None,
            cmp: None,
            // @end
            _entry: PhantomData,
        }
    }

    /// An iterator at slot `index` of leaf `page_id`, moved forward to the first slot that exists if `index` is past the leaf's end
    /// (the next leaf's first pair, and so on; the end if there are none). The tree creates iterators with this.
    pub fn at(bpm: &'a BufferPoolManager, page_id: PageId, index: u32) -> IndexIterator<'a, K, V, TOMBS> {
        let mut it = IndexIterator {
            bpm,
            page_id: Some(page_id),
            index,
            // @begin 2e-03
            last: None,
            cmp: None,
            // @end
            _entry: PhantomData,
        };
        it.skip_past_the_end_of_leaves();
        it
    }

    // @begin 2e-03
    /// Orders the keys by `cmp`, so the scan can tell a pair the tree moved under it (one at or before the last key it returned) and
    /// step over it.
    pub fn ordered_by(mut self, cmp: Box<dyn KeyComparator<K> + 'a>) -> Self {
        self.cmp = Some(cmp);
        self
    }
    // @end

    /// While the position is past the last pair of its leaf, move to the first slot of the next leaf (or to the end).
    fn skip_past_the_end_of_leaves(&mut self) {
        while let Some(page_id) = self.page_id {
            let guard = self.bpm.read_page(page_id);
            // a leaf merged away and freed since the last call is no longer a leaf: the scan cannot go on from it
            if !BPlusTreePage::new(&guard[..]).is_leaf_page() {
                self.page_id = None;
                return;
            }
            let leaf = BPlusTreeLeafPage::<_, K, V, TOMBS>::new(&guard[..]);
            // @begin 2d-01
            // a deleted (tombstoned) pair is not part of the scan: step over it
            while self.index < leaf.size() && leaf.is_deleted_at(self.index) {
                self.index += 1;
            }
            //~ // TODO(2d-01): with tombstones, skip the slots whose key is tombstoned
            // @end
            if self.index < leaf.size() {
                return;
            }
            self.page_id = leaf.next_page_id();
            self.index = 0;
        }
    }
}
//~ // TODO(2c-04): constructors and helpers of your own (the tree builds an iterator at a leaf and a slot, or at the end).
// @end

impl<K: FixedSize + Clone, V: FixedSize + Clone, const TOMBS: usize> IndexIterator<'_, K, V, TOMBS> {
    /// True once the scan is past the last pair.
    pub fn is_end(&self) -> bool {
        // @begin 2c-04
        self.page_id.is_none()
        //~ todo!("2c-04: there is no pair left to read")
        // @end
    }
}

impl<K: FixedSize + Clone, V: FixedSize + Clone, const TOMBS: usize> Iterator for IndexIterator<'_, K, V, TOMBS> {
    type Item = (K, V);

    /// The current pair, and move to the next one.
    fn next(&mut self) -> Option<(K, V)> {
        // @begin 2c-04
        // Nothing is latched between calls, so the leaf may have shrunk (a remove, a split) or stopped being a leaf (merged away)
        // since the position was found: check under the latch, and move on instead of reading a slot that is gone.
        loop {
            let page_id = self.page_id?;
            let pair = {
                let guard = self.bpm.read_page(page_id);
                if !BPlusTreePage::new(&guard[..]).is_leaf_page() {
                    self.page_id = None;
                    return None;
                }
                let leaf = BPlusTreeLeafPage::<_, K, V, TOMBS>::new(&guard[..]);
                (self.index < leaf.size()).then(|| leaf.entry_at(self.index))
            };
            match pair {
                Some(pair) => {
                    self.index += 1;
                    self.skip_past_the_end_of_leaves();
                    // @begin 2e-03
                    // a pair at or before the last key returned was moved here by a split, a borrow or an insert: it was returned already
                    if let (Some(last), Some(cmp)) = (&self.last, &self.cmp) {
                        if cmp.compare(&pair.0, last).is_le() {
                            continue;
                        }
                    }
                    self.last = Some(pair.0.clone());
                    // @end
                    return Some(pair);
                }
                None => self.skip_past_the_end_of_leaves(),
            }
        }
        //~ todo!("2c-04: copy the pair at the current position out of its leaf (latch, read, unlatch), advance, and return the pair; None at the end")
        // @end
    }
}

/// Two iterators are equal when they are at the same place.
impl<K, V, const TOMBS: usize> PartialEq for IndexIterator<'_, K, V, TOMBS> {
    fn eq(&self, other: &Self) -> bool {
        // @begin 2c-04
        self.page_id == other.page_id && self.index == other.index
        //~ todo!("2c-04: the same place in the same leaf, or both at the end")
        // @end
    }
}
