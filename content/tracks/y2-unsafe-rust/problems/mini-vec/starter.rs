use std::ops::{Deref, DerefMut};

/// A growable array with its own allocation: pointer, capacity, length.
/// Capacity starts at 0, becomes 4 on the first push, then doubles. A zero-sized `T` never allocates.
pub struct MyVec<T> {
    // TODO: a `NonNull<T>`, a capacity and a length (and say why it is `Send`/`Sync`)
    _marker: std::marker::PhantomData<T>,
}

// TODO: SAFETY comments; MyVec owns its elements like Vec<T> does.
unsafe impl<T: Send> Send for MyVec<T> {}
unsafe impl<T: Sync> Sync for MyVec<T> {}

impl<T> MyVec<T> {
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

    pub fn push(&mut self, value: T) {
        todo!()
    }

    pub fn pop(&mut self) -> Option<T> {
        todo!()
    }

    /// Inserts at `index`, shifting later elements right. Panics if `index > len`.
    pub fn insert(&mut self, index: usize, value: T) {
        todo!()
    }

    /// Removes and returns the element at `index`, shifting later elements left. Panics if `index >= len`.
    pub fn remove(&mut self, index: usize) -> T {
        todo!()
    }

    /// Drops the elements past `new_len`.
    pub fn truncate(&mut self, new_len: usize) {
        todo!()
    }

    pub fn clear(&mut self) {
        todo!()
    }
}

impl<T> Deref for MyVec<T> {
    type Target = [T];
    fn deref(&self) -> &[T] {
        todo!()
    }
}

impl<T> DerefMut for MyVec<T> {
    fn deref_mut(&mut self) -> &mut [T] {
        todo!()
    }
}

// TODO: `Drop`: drop the elements, then free the buffer (but only if there is one).
