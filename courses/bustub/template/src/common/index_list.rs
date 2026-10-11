//! A list whose elements can be reached, moved and removed by a handle in constant time.
//!
//! What a replacer needs to move a frame to the back or remove it without scanning.

use std::marker::PhantomData;

/// Names one element of an [`IndexList`]. It stays valid until that element is removed; after that it must never name another
/// element, even if the list reuses the space.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct Handle {
    // TODO(1c-01): what a handle must remember to find its element again, and to notice that the element is gone.
}

// TODO(1c-01): private types and helpers go here.

/// An ordered list with handles. Elements are ordered front to back; new elements go at the back.
pub struct IndexList<T> {
    // TODO(1c-01): the fields are yours.
    _element: PhantomData<T>,
}

impl<T> IndexList<T> {
    /// An empty list.
    pub fn new() -> IndexList<T> {
        todo!("1c-01: an empty list")
    }

    /// How many elements are in the list.
    pub fn len(&self) -> usize {
        todo!("1c-01: how many elements are in the list")
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }


    /// The element `handle` names, or `None` if it was removed.
    pub fn get(&self, handle: Handle) -> Option<&T> {
        todo!("1c-01: the element the handle names, if it is still in the list")
    }

    /// The first element, without removing it.
    pub fn front(&self) -> Option<&T> {
        todo!("1c-01: the first element, if any")
    }

    /// The elements from front to back.
    pub fn iter(&self) -> impl Iterator<Item = &T> + '_ {
        std::iter::from_fn(|| -> Option<&T> { todo!("1c-01: walk the list from front to back") })
    }

    /// Adds `value` at the back and returns a handle to it.
    pub fn push_back(&mut self, value: T) -> Handle {
        todo!("1c-01: put the value after the current last element and return its handle")
    }


    /// Removes and returns the first element.
    pub fn pop_front(&mut self) -> Option<T> {
        todo!("1c-01: remove the first element, if there is one")
    }

    /// Removes the element `handle` names. `None` if it was already removed (even if its space has been reused since).
    pub fn remove(&mut self, handle: Handle) -> Option<T> {
        todo!("1c-01: remove the element the handle names, if it is still there")
    }

    /// Moves the element to the back; its handle stays valid. Returns false if the handle is stale.
    pub fn move_to_back(&mut self, handle: Handle) -> bool {
        todo!("1c-01: put the element after the last one; return whether the handle was still valid")
    }
}

impl<T> Default for IndexList<T> {
    fn default() -> Self {
        Self::new()
    }
}
