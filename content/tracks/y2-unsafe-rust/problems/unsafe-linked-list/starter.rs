use std::marker::PhantomData;

/// A doubly linked list that owns heap nodes through raw pointers (`NonNull<Node<T>>`).
/// Write the invariants down as a comment before you write any `unsafe`.
pub struct List<T> {
    // TODO: head and tail links, a length, and a PhantomData that says the list owns its nodes
    _own: PhantomData<T>,
}

impl<T> List<T> {
    pub fn new() -> Self {
        todo!()
    }

    pub fn len(&self) -> usize {
        todo!()
    }

    pub fn is_empty(&self) -> bool {
        todo!()
    }

    pub fn push_front(&mut self, elem: T) {
        todo!()
    }

    pub fn push_back(&mut self, elem: T) {
        todo!()
    }

    pub fn pop_front(&mut self) -> Option<T> {
        todo!()
    }

    pub fn pop_back(&mut self) -> Option<T> {
        todo!()
    }

    pub fn front(&self) -> Option<&T> {
        todo!()
    }

    pub fn back(&self) -> Option<&T> {
        todo!()
    }

    /// Moves every element of `other` to the back of `self` in O(1); `other` is left empty.
    pub fn append(&mut self, other: &mut List<T>) {
        todo!()
    }

    /// Keeps the elements for which `keep` returns true, dropping the rest.
    pub fn retain<F: FnMut(&T) -> bool>(&mut self, keep: F) {
        todo!()
    }

    pub fn iter(&self) -> Iter<'_, T> {
        todo!()
    }

    pub fn iter_mut(&mut self) -> IterMut<'_, T> {
        todo!()
    }
}

// TODO: `Drop` that does not recurse (a million nodes must not overflow the stack)

impl<T> Default for List<T> {
    fn default() -> Self {
        List::new()
    }
}

pub struct Iter<'a, T> {
    _m: PhantomData<&'a T>,
}

impl<'a, T> Iterator for Iter<'a, T> {
    type Item = &'a T;
    fn next(&mut self) -> Option<&'a T> {
        todo!()
    }
}

impl<'a, T> DoubleEndedIterator for Iter<'a, T> {
    fn next_back(&mut self) -> Option<&'a T> {
        todo!()
    }
}

pub struct IterMut<'a, T> {
    _m: PhantomData<&'a mut T>,
}

impl<'a, T> Iterator for IterMut<'a, T> {
    type Item = &'a mut T;
    fn next(&mut self) -> Option<&'a mut T> {
        todo!()
    }
}

impl<'a, T> DoubleEndedIterator for IterMut<'a, T> {
    fn next_back(&mut self) -> Option<&'a mut T> {
        todo!()
    }
}
