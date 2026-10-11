//! Ownership shapes for systems code: a value moves out of a place, a structure refers to its parts by handle, not by reference.

use std::mem;

/// A stack as a linked list: each node owns the next.
pub struct Stack<T> {
    _stack: std::marker::PhantomData<T>,
    // TODO(r-03): your fields: the first node (each node owns the next) and the length
}

// TODO(r-03): the node type of your own

impl<T> Stack<T> {
    /// An empty stack.
    pub fn new() -> Stack<T> {
        todo!("r-03: an empty stack")
    }

    /// The number of values.
    pub fn len(&self) -> usize {
        todo!("r-03: the number of values")
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// Pushes `value` on top.
    pub fn push(&mut self, value: T) {
        todo!("r-03: the new node owns the old head: take it out of self with Option::take")
    }

    /// Removes and returns the top value.
    pub fn pop(&mut self) -> Option<T> {
        todo!("r-03: take the head; the head becomes its next; give back the value")
    }

    /// The top value, without removing it.
    pub fn peek(&self) -> Option<&T> {
        todo!("r-03: a reference to the top value, without moving it (as_ref and map)")
    }

    /// A mutable reference to the top value.
    pub fn peek_mut(&mut self) -> Option<&mut T> {
        todo!("r-03: a mutable reference to the top value (as_mut)")
    }

    /// Turns the stack upside down in place, in `O(n)` and without allocating.
    pub fn reverse(&mut self) {
        todo!("r-03: walk the old list taking each node and pushing it onto a new list (mem::take, mem::replace, no clones)")
    }

    /// The values from the top down.
    pub fn to_vec(&self) -> Vec<T>
    where
        T: Clone,
    {
        todo!("r-03: follow the links with as_deref, cloning each value")
    }
}

impl<T> Default for Stack<T> {
    fn default() -> Self {
        Stack::new()
    }
}

impl<T> Drop for Stack<T> {
    fn drop(&mut self) {
        // r-03: a loop that takes each node's next before the node is dropped (a recursive drop of a long list overflows the stack)
    }
}

/// A handle to a value of a [`Slab`]: an index and a generation, so that a handle to a removed value never finds the value that reuses
/// its slot.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct Handle {
    index: usize,
    generation: u32,
}

/// An arena: values live in a `Vec`, are referred to by [`Handle`], and can be removed; slots are reused. This is how Rust builds
/// graphs, lists and trees whose parts refer to each other (nothing here borrows from anything).
pub struct Slab<T> {
    _slab: std::marker::PhantomData<T>,
    // TODO(r-03): your fields: the slots (each with a value or a place in a free list, and a generation), the free slots, the number of values
}

// TODO(r-03): the slot type of your own

impl<T> Slab<T> {
    /// An empty slab.
    pub fn new() -> Slab<T> {
        todo!("r-03: an empty slab")
    }

    /// The number of values.
    pub fn len(&self) -> usize {
        todo!("r-03: the number of values")
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// Stores the value (in a freed slot if there is one) and returns its handle.
    pub fn insert(&mut self, value: T) -> Handle {
        todo!("r-03: reuse a freed slot (its generation was bumped when it was freed) or push a new one; the handle names the slot and its generation")
    }

    /// The value `handle` names. `None` if its slot does not exist, was reused by a newer generation or holds no value.
    pub fn get(&self, handle: Handle) -> Option<&T> {
        todo!("r-03: None for a slot that does not exist, has another generation or holds no value")
    }

    /// As [`get`](Self::get), mutably.
    pub fn get_mut(&mut self, handle: Handle) -> Option<&mut T> {
        todo!("r-03: as get, mutably")
    }

    /// Takes the value out; the handle (and every copy of it) is dead from now on.
    pub fn remove(&mut self, handle: Handle) -> Option<T> {
        todo!("r-03: take the value out of its slot (Option::take); bump the slot's generation; remember the slot as free")
    }
}

impl<T> Default for Slab<T> {
    fn default() -> Self {
        Slab::new()
    }
}
