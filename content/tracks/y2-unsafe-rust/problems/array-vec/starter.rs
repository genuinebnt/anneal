use std::mem::MaybeUninit;
use std::ops::{Deref, DerefMut};

/// A vector with room for `N` elements stored inline (no heap). Slots `buf[..len]` are initialised, the rest are not.
pub struct ArrayVec<T, const N: usize> {
    buf: [MaybeUninit<T>; N],
    len: usize,
}

impl<T, const N: usize> ArrayVec<T, N> {
    pub fn new() -> Self {
        todo!()
    }

    pub fn len(&self) -> usize {
        todo!()
    }

    pub fn is_empty(&self) -> bool {
        todo!()
    }

    pub fn capacity(&self) -> usize {
        todo!()
    }

    pub fn is_full(&self) -> bool {
        todo!()
    }

    /// Returns the value back if the vector is full.
    pub fn try_push(&mut self, value: T) -> Result<(), T> {
        todo!()
    }

    pub fn pop(&mut self) -> Option<T> {
        todo!()
    }

    /// Inserts at `index` (0..=len); returns the value if full. Panics if `index > len`.
    pub fn try_insert(&mut self, index: usize, value: T) -> Result<(), T> {
        todo!()
    }

    /// Removes and returns the element at `index`, keeping order. Panics if `index >= len`.
    pub fn remove(&mut self, index: usize) -> T {
        todo!()
    }

    /// Removes the element at `index` by swapping the last element into its place (O(1), order not kept).
    pub fn swap_remove(&mut self, index: usize) -> T {
        todo!()
    }

    pub fn as_slice(&self) -> &[T] {
        todo!()
    }

    pub fn as_mut_slice(&mut self) -> &mut [T] {
        todo!()
    }

    /// The elements as a real `[T; N]` if exactly `N` are present, else `self` back.
    pub fn into_array(self) -> Result<[T; N], Self> {
        todo!()
    }
}

impl<T, const N: usize> Deref for ArrayVec<T, N> {
    type Target = [T];
    fn deref(&self) -> &[T] {
        todo!()
    }
}

impl<T, const N: usize> DerefMut for ArrayVec<T, N> {
    fn deref_mut(&mut self) -> &mut [T] {
        todo!()
    }
}

// TODO: `Drop`: drop exactly the initialised prefix.
