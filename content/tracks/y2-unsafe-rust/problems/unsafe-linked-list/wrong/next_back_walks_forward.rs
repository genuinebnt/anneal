use std::marker::PhantomData;
use std::ptr::NonNull;

type Link<T> = Option<NonNull<Node<T>>>;

struct Node<T> {
    elem: T,
    prev: Link<T>,
    next: Link<T>,
}

/// A doubly linked list that owns heap nodes through raw pointers.
/// Invariants: `head`/`tail` are both `None` or both point at live nodes; `head.prev` and `tail.next` are `None`;
/// for every node `n`, `n.next.prev == n` and `n.prev.next == n`; `len` counts the nodes.
pub struct List<T> {
    head: Link<T>,
    tail: Link<T>,
    len: usize,
    _own: PhantomData<Box<Node<T>>>, // we own the nodes: tells the drop checker and variance about it
}

impl<T> List<T> {
    pub fn new() -> Self {
        List { head: None, tail: None, len: 0, _own: PhantomData }
    }

    pub fn len(&self) -> usize {
        self.len
    }

    pub fn is_empty(&self) -> bool {
        self.len == 0
    }

    pub fn push_front(&mut self, elem: T) {
        let node = NonNull::from(Box::leak(Box::new(Node { elem, prev: None, next: self.head })));
        match self.head {
            // SAFETY: head points at a live node we own, and we hold `&mut self`.
            Some(h) => unsafe { (*h.as_ptr()).prev = Some(node) },
            None => self.tail = Some(node),
        }
        self.head = Some(node);
        self.len += 1;
    }

    pub fn push_back(&mut self, elem: T) {
        let node = NonNull::from(Box::leak(Box::new(Node { elem, prev: self.tail, next: None })));
        match self.tail {
            // SAFETY: tail points at a live node we own, and we hold `&mut self`.
            Some(t) => unsafe { (*t.as_ptr()).next = Some(node) },
            None => self.head = Some(node),
        }
        self.tail = Some(node);
        self.len += 1;
    }

    pub fn pop_front(&mut self) -> Option<T> {
        self.head.map(|h| {
            // SAFETY: h was made by Box::leak in a push and is unlinked below, so the Box is rebuilt exactly once.
            let boxed = unsafe { Box::from_raw(h.as_ptr()) };
            self.head = boxed.next;
            match self.head {
                // SAFETY: the new head is a live node we own.
                Some(n) => unsafe { (*n.as_ptr()).prev = None },
                None => self.tail = None,
            }
            self.len -= 1;
            boxed.elem
        })
    }

    pub fn pop_back(&mut self) -> Option<T> {
        self.tail.map(|t| {
            // SAFETY: as in pop_front.
            let boxed = unsafe { Box::from_raw(t.as_ptr()) };
            self.tail = boxed.prev;
            match self.tail {
                // SAFETY: the new tail is a live node we own.
                Some(n) => unsafe { (*n.as_ptr()).next = None },
                None => self.head = None,
            }
            self.len -= 1;
            boxed.elem
        })
    }

    pub fn front(&self) -> Option<&T> {
        // SAFETY: head is a live node; the returned borrow is tied to &self.
        self.head.map(|h| unsafe { &(*h.as_ptr()).elem })
    }

    pub fn back(&self) -> Option<&T> {
        // SAFETY: tail is a live node; the returned borrow is tied to &self.
        self.tail.map(|t| unsafe { &(*t.as_ptr()).elem })
    }

    /// Moves every element of `other` to the back of `self` in O(1); `other` is left empty.
    pub fn append(&mut self, other: &mut List<T>) {
        let Some(other_head) = other.head else { return };
        match self.tail {
            Some(t) => {
                // SAFETY: both ends are live nodes owned by the two lists we hold `&mut` to.
                unsafe {
                    (*t.as_ptr()).next = Some(other_head);
                    (*other_head.as_ptr()).prev = Some(t);
                }
            }
            None => self.head = Some(other_head),
        }
        self.tail = other.tail;
        self.len += other.len;
        other.head = None;
        other.tail = None;
        other.len = 0;
    }

    /// Keeps the elements for which `keep` returns true, dropping the rest.
    pub fn retain<F: FnMut(&T) -> bool>(&mut self, mut keep: F) {
        let mut cur = self.head;
        while let Some(n) = cur {
            // SAFETY: n is a live node; we read its `next` before possibly freeing it.
            let next = unsafe { (*n.as_ptr()).next };
            let keep_it = keep(unsafe { &(*n.as_ptr()).elem });
            if !keep_it {
                // SAFETY: n is live and gets unlinked and freed exactly once here.
                unsafe {
                    let boxed = Box::from_raw(n.as_ptr());
                    match boxed.prev {
                        Some(p) => (*p.as_ptr()).next = boxed.next,
                        None => self.head = boxed.next,
                    }
                    match boxed.next {
                        Some(nx) => (*nx.as_ptr()).prev = boxed.prev,
                        None => self.tail = boxed.prev,
                    }
                }
                self.len -= 1;
            }
            cur = next;
        }
    }

    pub fn iter(&self) -> Iter<'_, T> {
        Iter { head: self.head, tail: self.tail, len: self.len, _borrow: PhantomData }
    }

    pub fn iter_mut(&mut self) -> IterMut<'_, T> {
        IterMut { head: self.head, tail: self.tail, len: self.len, _borrow: PhantomData }
    }
}

impl<T> Drop for List<T> {
    fn drop(&mut self) {
        // Iterative, so a million-node list does not overflow the stack the way a recursive drop would.
        while self.pop_front().is_some() {}
    }
}

impl<T> Default for List<T> {
    fn default() -> Self {
        List::new()
    }
}

pub struct Iter<'a, T> {
    head: Link<T>,
    tail: Link<T>,
    len: usize,
    _borrow: PhantomData<&'a T>,
}

impl<'a, T> Iterator for Iter<'a, T> {
    type Item = &'a T;

    fn next(&mut self) -> Option<&'a T> {
        if self.len == 0 {
            return None;
        }
        self.head.map(|h| {
            self.len -= 1;
            // SAFETY: len > 0 means h is a live node no `next_back` has yielded; the list is borrowed for 'a.
            unsafe {
                self.head = (*h.as_ptr()).next;
                &(*h.as_ptr()).elem
            }
        })
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        (self.len, Some(self.len))
    }
}

impl<'a, T> DoubleEndedIterator for Iter<'a, T> {
    fn next_back(&mut self) -> Option<&'a T> {
        if self.len == 0 {
            return None;
        }
        self.tail.map(|t| {
            self.len -= 1;
            // SAFETY: as in `next`, from the other end.
            unsafe {
                self.tail = (*t.as_ptr()).next;
                &(*t.as_ptr()).elem
            }
        })
    }
}

pub struct IterMut<'a, T> {
    head: Link<T>,
    tail: Link<T>,
    len: usize,
    _borrow: PhantomData<&'a mut T>,
}

impl<'a, T> Iterator for IterMut<'a, T> {
    type Item = &'a mut T;

    fn next(&mut self) -> Option<&'a mut T> {
        if self.len == 0 {
            return None;
        }
        self.head.map(|h| {
            self.len -= 1;
            // SAFETY: each node is yielded at most once, so the `&mut` borrows are disjoint.
            unsafe {
                self.head = (*h.as_ptr()).next;
                &mut (*h.as_ptr()).elem
            }
        })
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        (self.len, Some(self.len))
    }
}

impl<'a, T> DoubleEndedIterator for IterMut<'a, T> {
    fn next_back(&mut self) -> Option<&'a mut T> {
        if self.len == 0 {
            return None;
        }
        self.tail.map(|t| {
            self.len -= 1;
            // SAFETY: as in `next`, from the other end.
            unsafe {
                self.tail = (*t.as_ptr()).prev;
                &mut (*t.as_ptr()).elem
            }
        })
    }
}
