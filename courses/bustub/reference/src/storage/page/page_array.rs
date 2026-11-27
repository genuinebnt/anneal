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
        // @begin 2a-02
        assert!(index < self.capacity(), "entry {index} is past the capacity {}", self.capacity());
        let at = index * T::SIZE;
        T::decode(&self.bytes.as_ref()[at..at + T::SIZE])
        //~ todo!("2a-02: the entry at index, decoded from its T::SIZE bytes; panic past the capacity")
        // @end
    }

    /// The first index in `0..len` whose entry is not `Less` than the target, according to `cmp` (which compares an entry with the
    /// target); `len` if every entry is less. The entries `0..len` must be sorted.
    pub fn lower_bound(&self, len: usize, mut cmp: impl FnMut(&T) -> Ordering) -> usize {
        // @begin 2a-02
        let (mut lo, mut hi) = (0, len);
        while lo < hi {
            let mid = lo + (hi - lo) / 2;
            if cmp(&self.get(mid)) == Ordering::Less {
                lo = mid + 1;
            } else {
                hi = mid;
            }
        }
        lo
        //~ todo!("2a-02: the first index in 0..len whose entry is not less than the target, in O(log len) comparisons")
        // @end
    }
}

impl<B: AsRef<[u8]> + AsMut<[u8]>, T: FixedSize> PageArray<B, T> {
    /// Stores `value` as entry `index`. Panics if `index` is past the capacity.
    pub fn set(&mut self, index: usize, value: &T) {
        // @begin 2a-02
        assert!(index < self.capacity(), "entry {index} is past the capacity {}", self.capacity());
        let at = index * T::SIZE;
        value.encode(&mut self.bytes.as_mut()[at..at + T::SIZE]);
        //~ todo!("2a-02: store the value as entry index; panic past the capacity")
        // @end
    }

    /// Inserts `value` at `index`, shifting entries `index..len` one place to the right. The array must have room for `len + 1`.
    pub fn insert_at(&mut self, index: usize, len: usize, value: &T) {
        // @begin 2a-02
        assert!(index <= len && len < self.capacity(), "no room to insert at {index} (len {len}, capacity {})", self.capacity());
        let size = T::SIZE;
        self.bytes.as_mut().copy_within(index * size..len * size, (index + 1) * size);
        self.set(index, value);
        //~ todo!("2a-02: entries index..len move one place right and the value takes index; panic without room for len + 1")
        // @end
    }

    /// Removes entry `index`, shifting entries `index + 1..len` one place to the left.
    pub fn remove_at(&mut self, index: usize, len: usize) {
        // @begin 2a-02
        assert!(index < len, "no entry {index} to remove (len {len})");
        let size = T::SIZE;
        self.bytes.as_mut().copy_within((index + 1) * size..len * size, index * size);
        //~ todo!("2a-02: entries index + 1..len move one place left; panic if index >= len")
        // @end
    }
}
