//! Ownership shapes for systems code: a value moves out of a place, a structure refers to its parts by handle, not by reference.

use std::mem;

/// A stack as a linked list: each node owns the next.
pub struct Stack<T> {
    // @begin r-03
    head: Option<Box<Node<T>>>,
    len: usize,
    //~ _stack: std::marker::PhantomData<T>,
    //~ // TODO(r-03): your fields: the first node (each node owns the next) and the length
    // @end
}

// @begin r-03
struct Node<T> {
    value: T,
    next: Option<Box<Node<T>>>,
}
//~ // TODO(r-03): the node type of your own
// @end

impl<T> Stack<T> {
    /// An empty stack.
    pub fn new() -> Stack<T> {
        // @begin r-03
        Stack { head: None, len: 0 }
        //~ todo!("r-03: an empty stack")
        // @end
    }

    /// The number of values.
    pub fn len(&self) -> usize {
        // @begin r-03
        self.len
        //~ todo!("r-03: the number of values")
        // @end
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// Pushes `value` on top.
    pub fn push(&mut self, value: T) {
        // @begin r-03
        let next = self.head.take();
        self.head = Some(Box::new(Node { value, next }));
        self.len += 1;
        //~ todo!("r-03: the new node owns the old head: take it out of self with Option::take")
        // @end
    }

    /// Removes and returns the top value.
    pub fn pop(&mut self) -> Option<T> {
        // @begin r-03
        let node = self.head.take()?;
        self.head = node.next;
        self.len -= 1;
        Some(node.value)
        //~ todo!("r-03: take the head; the head becomes its next; give back the value")
        // @end
    }

    /// The top value, without removing it.
    pub fn peek(&self) -> Option<&T> {
        // @begin r-03
        self.head.as_ref().map(|n| &n.value)
        //~ todo!("r-03: a reference to the top value, without moving it (as_ref and map)")
        // @end
    }

    /// A mutable reference to the top value.
    pub fn peek_mut(&mut self) -> Option<&mut T> {
        // @begin r-03
        self.head.as_mut().map(|n| &mut n.value)
        //~ todo!("r-03: a mutable reference to the top value (as_mut)")
        // @end
    }

    /// Turns the stack upside down in place, in `O(n)` and without allocating.
    pub fn reverse(&mut self) {
        // @begin r-03
        let mut reversed: Option<Box<Node<T>>> = None;
        let mut current = mem::take(&mut self.head);
        while let Some(mut node) = current {
            current = mem::replace(&mut node.next, reversed);
            reversed = Some(node);
        }
        self.head = reversed;
        //~ todo!("r-03: walk the old list taking each node and pushing it onto a new list (mem::take, mem::replace, no clones)")
        // @end
    }

    /// The values from the top down.
    pub fn to_vec(&self) -> Vec<T>
    where
        T: Clone,
    {
        // @begin r-03
        let mut out = Vec::with_capacity(self.len);
        let mut cur = self.head.as_deref();
        while let Some(node) = cur {
            out.push(node.value.clone());
            cur = node.next.as_deref();
        }
        out
        //~ todo!("r-03: follow the links with as_deref, cloning each value")
        // @end
    }
}

impl<T> Default for Stack<T> {
    fn default() -> Self {
        Stack::new()
    }
}

impl<T> Drop for Stack<T> {
    fn drop(&mut self) {
        // @begin r-03
        // dropping the head would drop the whole chain recursively: a long stack would overflow the call stack
        let mut cur = self.head.take();
        while let Some(mut node) = cur {
            cur = node.next.take();
        }
        //~ // r-03: a loop that takes each node's next before the node is dropped (a recursive drop of a long list overflows the stack)
        // @end
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
    // @begin r-03
    slots: Vec<Slot<T>>,
    free: Vec<usize>,
    len: usize,
    //~ _slab: std::marker::PhantomData<T>,
    //~ // TODO(r-03): your fields: the slots (each with a value or a place in a free list, and a generation), the free slots, the number of values
    // @end
}

// @begin r-03
struct Slot<T> {
    generation: u32,
    value: Option<T>,
}
//~ // TODO(r-03): the slot type of your own
// @end

impl<T> Slab<T> {
    /// An empty slab.
    pub fn new() -> Slab<T> {
        // @begin r-03
        Slab { slots: Vec::new(), free: Vec::new(), len: 0 }
        //~ todo!("r-03: an empty slab")
        // @end
    }

    /// The number of values.
    pub fn len(&self) -> usize {
        // @begin r-03
        self.len
        //~ todo!("r-03: the number of values")
        // @end
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// Stores the value (in a freed slot if there is one) and returns its handle.
    pub fn insert(&mut self, value: T) -> Handle {
        // @begin r-03
        self.len += 1;
        match self.free.pop() {
            Some(index) => {
                let slot = &mut self.slots[index];
                slot.value = Some(value);
                Handle { index, generation: slot.generation }
            }
            None => {
                self.slots.push(Slot { generation: 0, value: Some(value) });
                Handle { index: self.slots.len() - 1, generation: 0 }
            }
        }
        //~ todo!("r-03: reuse a freed slot (its generation was bumped when it was freed) or push a new one; the handle names the slot and its generation")
        // @end
    }

    /// The value `handle` names. `None` if its slot does not exist, was reused by a newer generation or holds no value.
    pub fn get(&self, handle: Handle) -> Option<&T> {
        // @begin r-03
        let slot = self.slots.get(handle.index)?;
        if slot.generation != handle.generation {
            return None;
        }
        slot.value.as_ref()
        //~ todo!("r-03: None for a slot that does not exist, has another generation or holds no value")
        // @end
    }

    /// As [`get`](Self::get), mutably.
    pub fn get_mut(&mut self, handle: Handle) -> Option<&mut T> {
        // @begin r-03
        let slot = self.slots.get_mut(handle.index)?;
        if slot.generation != handle.generation {
            return None;
        }
        slot.value.as_mut()
        //~ todo!("r-03: as get, mutably")
        // @end
    }

    /// Takes the value out; the handle (and every copy of it) is dead from now on.
    pub fn remove(&mut self, handle: Handle) -> Option<T> {
        // @begin r-03
        let slot = self.slots.get_mut(handle.index)?;
        if slot.generation != handle.generation {
            return None;
        }
        let value = slot.value.take()?;
        slot.generation += 1;
        self.free.push(handle.index);
        self.len -= 1;
        Some(value)
        //~ todo!("r-03: take the value out of its slot (Option::take); bump the slot's generation; remember the slot as free")
        // @end
    }
}

impl<T> Default for Slab<T> {
    fn default() -> Self {
        Slab::new()
    }
}
