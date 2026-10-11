//! A list whose elements can be reached, moved and removed by a handle in constant time.
//!
//! What a replacer needs to move a frame to the back or remove it without scanning.

// @begin 1c-01
//~ use std::marker::PhantomData;
// @end

/// Names one element of an [`IndexList`]. It stays valid until that element is removed; after that it must never name another
/// element, even if the list reuses the space.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct Handle {
    // @begin 1c-01
    index: usize,
    generation: u32,
    //~ // TODO(1c-01): what a handle must remember to find its element again, and to notice that the element is gone.
    // @end
}

// @begin 1c-01
struct Node<T> {
    /// `None` while the node is on the free list.
    value: Option<T>,
    prev: Option<usize>,
    next: Option<usize>,
    /// Bumped each time the node is freed, so old handles to it stop matching.
    generation: u32,
}
//~ // TODO(1c-01): private types and helpers go here.
// @end

/// An ordered list with handles. Elements are ordered front to back; new elements go at the back.
pub struct IndexList<T> {
    // @begin 1c-01
    nodes: Vec<Node<T>>,
    head: Option<usize>,
    tail: Option<usize>,
    /// Indices of freed nodes, ready for reuse.
    free: Vec<usize>,
    len: usize,
    //~ // TODO(1c-01): the fields are yours.
    //~ _element: PhantomData<T>,
    // @end
}

impl<T> IndexList<T> {
    /// An empty list.
    pub fn new() -> IndexList<T> {
        // @begin 1c-01
        IndexList { nodes: Vec::new(), head: None, tail: None, free: Vec::new(), len: 0 }
        //~ todo!("1c-01: an empty list")
        // @end
    }

    /// How many elements are in the list.
    pub fn len(&self) -> usize {
        // @begin 1c-01
        self.len
        //~ todo!("1c-01: how many elements are in the list")
        // @end
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    // @begin 1c-01
    /// The node `handle` names, if the element is still in the list.
    fn index_of(&self, handle: Handle) -> Option<usize> {
        let node = self.nodes.get(handle.index)?;
        (node.generation == handle.generation && node.value.is_some()).then_some(handle.index)
    }
    // @end

    /// The element `handle` names, or `None` if it was removed.
    pub fn get(&self, handle: Handle) -> Option<&T> {
        // @begin 1c-01
        self.nodes[self.index_of(handle)?].value.as_ref()
        //~ todo!("1c-01: the element the handle names, if it is still in the list")
        // @end
    }

    /// The first element, without removing it.
    pub fn front(&self) -> Option<&T> {
        // @begin 1c-01
        self.nodes[self.head?].value.as_ref()
        //~ todo!("1c-01: the first element, if any")
        // @end
    }

    /// The elements from front to back.
    pub fn iter(&self) -> impl Iterator<Item = &T> + '_ {
        // @begin 1c-01
        let mut at = self.head;
        std::iter::from_fn(move || {
            let node = &self.nodes[at?];
            at = node.next;
            node.value.as_ref()
        })
        //~ std::iter::from_fn(|| -> Option<&T> { todo!("1c-01: walk the list from front to back") })
        // @end
    }

    /// Adds `value` at the back and returns a handle to it.
    pub fn push_back(&mut self, value: T) -> Handle {
        // @begin 1c-01
        let mut node = Node { value: Some(value), prev: self.tail, next: None, generation: 0 };
        let index;
        if let Some(free) = self.free.pop() {
            node.generation = self.nodes[free].generation;
            self.nodes[free] = node;
            index = free;
        } else {
            self.nodes.push(node);
            index = self.nodes.len() - 1;
        }
        match self.tail {
            Some(tail) => self.nodes[tail].next = Some(index),
            None => self.head = Some(index),
        }
        self.tail = Some(index);
        self.len += 1;
        Handle { index, generation: self.nodes[index].generation }
        //~ todo!("1c-01: put the value after the current last element and return its handle")
        // @end
    }

    // @begin 1c-01
    /// Unlinks node `index` and frees it. The caller knows it is in the list.
    fn unlink(&mut self, index: usize) -> T {
        let (prev, next) = (self.nodes[index].prev, self.nodes[index].next);
        match prev {
            Some(p) => self.nodes[p].next = next,
            None => self.head = next,
        }
        match next {
            Some(n) => self.nodes[n].prev = prev,
            None => self.tail = prev,
        }
        let node = &mut self.nodes[index];
        let value = node.value.take().expect("a linked node holds a value");
        node.prev = None;
        node.next = None;
        node.generation += 1;
        self.free.push(index);
        self.len -= 1;
        value
    }
    // @end

    /// Removes and returns the first element.
    pub fn pop_front(&mut self) -> Option<T> {
        // @begin 1c-01
        let head = self.head?;
        Some(self.unlink(head))
        //~ todo!("1c-01: remove the first element, if there is one")
        // @end
    }

    /// Removes the element `handle` names. `None` if it was already removed (even if its space has been reused since).
    pub fn remove(&mut self, handle: Handle) -> Option<T> {
        // @begin 1c-01
        let index = self.index_of(handle)?;
        Some(self.unlink(index))
        //~ todo!("1c-01: remove the element the handle names, if it is still there")
        // @end
    }

    /// Moves the element to the back; its handle stays valid. Returns false if the handle is stale.
    pub fn move_to_back(&mut self, handle: Handle) -> bool {
        // @begin 1c-01
        let Some(index) = self.index_of(handle) else { return false };
        if self.tail == Some(index) {
            return true;
        }
        let (prev, next) = (self.nodes[index].prev, self.nodes[index].next);
        match prev {
            Some(p) => self.nodes[p].next = next,
            None => self.head = next,
        }
        if let Some(n) = next {
            self.nodes[n].prev = prev;
        }
        let old_tail = self.tail.expect("a non-empty list has a tail");
        self.nodes[old_tail].next = Some(index);
        self.nodes[index].prev = Some(old_tail);
        self.nodes[index].next = None;
        self.tail = Some(index);
        true
        //~ todo!("1c-01: put the element after the last one; return whether the handle was still valid")
        // @end
    }
}

impl<T> Default for IndexList<T> {
    fn default() -> Self {
        Self::new()
    }
}
