//! A range scan over the leaves of a B+ tree. It remembers a position (a leaf page and a slot in it) and, when asked for the next pair, latches
//! the leaf just long enough to copy the pair out. Nothing stays latched between calls, so a scan never blocks writers for longer than one read;
//! the price is that a scan that races with a writer may see the tree change under it.
//!
//! It is an `Iterator` (`for (k, v) in tree.begin()`); [`IndexIterator::is_end`] and `==` are there for callers that compare against the end.

use crate::storage::index::fixed_size::FixedSize;

// TODO(2c-03): your imports go here.

pub struct IndexIterator<'a, K, V, const TOMBS: usize = 0> {
    _iterator: std::marker::PhantomData<(&'a (), K, V)>,
    // TODO(2c-03): the fields are yours: where the scan is, and what it needs to read the next pair.
}

// TODO(2c-03): constructors and helpers of your own (the tree builds an iterator at a leaf and a slot, or at the end).

impl<K: FixedSize + Clone, V: FixedSize + Clone, const TOMBS: usize> IndexIterator<'_, K, V, TOMBS> {
    /// True once the scan is past the last pair.
    pub fn is_end(&self) -> bool {
        todo!("2c-03: there is no pair left to read")
    }
}

impl<K: FixedSize + Clone, V: FixedSize + Clone, const TOMBS: usize> Iterator for IndexIterator<'_, K, V, TOMBS> {
    type Item = (K, V);

    /// The current pair, and move to the next one.
    fn next(&mut self) -> Option<(K, V)> {
        todo!("2c-03: copy the pair at the current position out of its leaf (latch, read, unlatch), advance, and return the pair; None at the end")
    }
}

/// Two iterators are equal when they are at the same place.
impl<K, V, const TOMBS: usize> PartialEq for IndexIterator<'_, K, V, TOMBS> {
    fn eq(&self, other: &Self) -> bool {
        todo!("2c-03: the same place in the same leaf, or both at the end")
    }
}
