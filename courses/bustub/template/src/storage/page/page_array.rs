//! An array of fixed-size entries stored in a page. The array does not know how many entries are live (the page's header says), so operations
//! that shift entries take the current length.

use std::cmp::Ordering;
use std::marker::PhantomData;

use crate::storage::index::fixed_size::FixedSize;

/// A view of entries of type `T` laid end to end in `B`, usually a slice of a page (`&[u8]` to read, `&mut [u8]` to write).
pub struct PageArray<B, T> {
    bytes: B,
    _entry: PhantomData<T>,
}

impl<B, T: FixedSize> PageArray<B, T> {
    pub fn new(bytes: B) -> PageArray<B, T> {
        PageArray { bytes, _entry: PhantomData }
    }
}

impl<B: AsRef<[u8]>, T: FixedSize> PageArray<B, T> {
    /// How many entries fit in the bytes.
    pub fn capacity(&self) -> usize {
        self.bytes.as_ref().len() / T::SIZE
    }

    /// The entry at `index`. Panics if `index` is past the capacity.
    pub fn get(&self, index: usize) -> T {
        todo!("2a-03: panic past the capacity; otherwise decode the T::SIZE bytes at index * T::SIZE")
    }

    /// The first index in `0..len` whose entry is not `Less` than the target, according to `cmp` (which compares an entry with the
    /// target); `len` if every entry is less. The entries `0..len` must be sorted.
    pub fn lower_bound(&self, len: usize, mut cmp: impl FnMut(&T) -> Ordering) -> usize {
        todo!("2a-04: binary search for the first entry that is not less than the target")
    }
}

impl<B: AsRef<[u8]> + AsMut<[u8]>, T: FixedSize> PageArray<B, T> {
    /// Stores `value` as entry `index`. Panics if `index` is past the capacity.
    pub fn set(&mut self, index: usize, value: &T) {
        todo!("2a-03: panic past the capacity; otherwise encode the value into the T::SIZE bytes at index * T::SIZE")
    }

    /// Inserts `value` at `index`, shifting entries `index..len` one place to the right. The array must have room for `len + 1`.
    pub fn insert_at(&mut self, index: usize, len: usize, value: &T) {
        todo!("2a-03: move the bytes of entries index..len one entry to the right (copy_within handles the overlap), then set the entry")
    }

    /// Removes entry `index`, shifting entries `index + 1..len` one place to the left.
    pub fn remove_at(&mut self, index: usize, len: usize) {
        todo!("2a-03: move the bytes of entries index + 1..len one entry to the left")
    }
}
