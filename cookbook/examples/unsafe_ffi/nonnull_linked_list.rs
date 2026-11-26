//! RECIPE 102 (unsafe): a doubly linked `Deque<T>` with `NonNull`, `Box::into_raw` and `PhantomData`.
//! LEVEL: advanced
//! Use in: learning how std::collections::LinkedList works, intrusive/arena lists, any structure with back-pointers.   Run: cargo run --release --example nonnull_linked_list
#![allow(dead_code)]
//!
//! Safe Rust cannot express a node pointing both forward and back (it would need Rc<RefCell<..>> + Weak). So nodes
//! are heap-allocated with `Box`, leaked into raw pointers with `Box::into_raw`, and reclaimed with `Box::from_raw`.
//! INVARIANTS (every unsafe block below relies on these):
//!   I1. Every node reachable from `head`/`tail` was created by `Box::into_raw` and is freed only by `Box::from_raw`.
//!   I2. `head` is None iff `tail` is None iff `len == 0`; `head.prev == None`, `tail.next == None`.
//!   I3. For every node n: `n.next == Some(m)` implies `m.prev == Some(n)` (and symmetrically).
//!   I4. The list owns its nodes uniquely: no other pointer is used for access while a `&mut Deque` exists.
//! Why `NonNull<T>`: non-null (so `Option<NonNull<_>>` is pointer-sized) and COVARIANT in T, unlike `*mut T`.
//! Why `PhantomData<Box<Node<T>>>`: tells dropck that Deque OWNS (and drops) values of type T, so a `T` containing a
//! borrow must strictly outlive the list. Iterators carry `PhantomData<&'a T>` / `<&'a mut T>` to tie the borrow
//! of the list to the items. `Send`/`Sync` are not automatic for raw pointers, so we `unsafe impl` them with T bounds.
//! COMMON USES: LRU caches (O(1) unlink), task/timer queues, std::collections::LinkedList, intrusive lists in kernels.
//! In practice `VecDeque` is faster for almost everything; linked lists win when you hold node handles for O(1) removal.
//! Study (cite only): https://rust-unofficial.github.io/too-many-lists/
//! and https://github.com/rust-lang/rust/blob/master/library/alloc/src/collections/linked_list.rs
//! Read: https://doc.rust-lang.org/std/ptr/struct.NonNull.html and https://doc.rust-lang.org/nomicon/
use std::collections::VecDeque;
use std::marker::PhantomData;
use std::ptr::NonNull;
use std::sync::atomic::{AtomicUsize, Ordering::SeqCst};

type Link<T> = Option<NonNull<Node<T>>>;
struct Node<T> {
    elem: T,
    prev: Link<T>,
    next: Link<T>,
}

struct Deque<T> {
    head: Link<T>,
    tail: Link<T>,
    len: usize,
    _owns: PhantomData<Box<Node<T>>>,
}
// SAFETY: Deque uniquely owns its nodes (I4), exactly like Box/Vec, so thread-safety follows T.
unsafe impl<T: Send> Send for Deque<T> {}
// SAFETY: `&Deque<T>` only exposes `&T`.
unsafe impl<T: Sync> Sync for Deque<T> {}

impl<T> Deque<T> {
    fn new() -> Self {
        Self {
            head: None,
            tail: None,
            len: 0,
            _owns: PhantomData,
        }
    }
    fn len(&self) -> usize {
        self.len
    }
    fn alloc(elem: T, prev: Link<T>, next: Link<T>) -> NonNull<Node<T>> {
        let raw = Box::into_raw(Box::new(Node { elem, prev, next }));
        // SAFETY: Box::into_raw never returns null.
        unsafe { NonNull::new_unchecked(raw) }
    }
    fn push_front(&mut self, elem: T) {
        let new = Self::alloc(elem, None, self.head);
        match self.head {
            // SAFETY: `h` is a live node (I1) and we hold `&mut self` (I4); this restores I3 for the new link.
            Some(h) => unsafe { (*h.as_ptr()).prev = Some(new) },
            None => self.tail = Some(new),
        }
        self.head = Some(new);
        self.len += 1;
    }
    fn push_back(&mut self, elem: T) {
        let new = Self::alloc(elem, self.tail, None);
        match self.tail {
            // SAFETY: `t` is a live node (I1), unique access via `&mut self` (I4).
            Some(t) => unsafe { (*t.as_ptr()).next = Some(new) },
            None => self.head = Some(new),
        }
        self.tail = Some(new);
        self.len += 1;
    }
    fn pop_front(&mut self) -> Option<T> {
        self.head.map(|h| {
            // SAFETY: `h` came from Box::into_raw (I1) and after this line is unlinked, so it is freed exactly once.
            let node = unsafe { Box::from_raw(h.as_ptr()) };
            self.head = node.next;
            match self.head {
                // SAFETY: the new head is live (I1); clear its dangling back-pointer to the freed node (I2/I3).
                Some(n) => unsafe { (*n.as_ptr()).prev = None },
                None => self.tail = None,
            }
            self.len -= 1;
            node.elem
        })
    }
    fn pop_back(&mut self) -> Option<T> {
        self.tail.map(|t| {
            // SAFETY: same argument as pop_front, mirrored.
            let node = unsafe { Box::from_raw(t.as_ptr()) };
            self.tail = node.prev;
            match self.tail {
                // SAFETY: the new tail is live (I1); clear its forward pointer to the freed node.
                Some(p) => unsafe { (*p.as_ptr()).next = None },
                None => self.head = None,
            }
            self.len -= 1;
            node.elem
        })
    }
    fn iter(&self) -> Iter<'_, T> {
        Iter {
            front: self.head,
            back: self.tail,
            len: self.len,
            _m: PhantomData,
        }
    }
    fn iter_mut(&mut self) -> IterMut<'_, T> {
        IterMut {
            front: self.head,
            back: self.tail,
            len: self.len,
            _m: PhantomData,
        }
    }
}
impl<T> Drop for Deque<T> {
    fn drop(&mut self) {
        while self.pop_front().is_some() {} // iterative: recursion would overflow the stack on long lists
    }
}

struct Iter<'a, T> {
    front: Link<T>,
    back: Link<T>,
    len: usize, // remaining items; guarantees front and back never cross
    _m: PhantomData<&'a T>,
}
impl<'a, T> Iterator for Iter<'a, T> {
    type Item = &'a T;
    fn next(&mut self) -> Option<&'a T> {
        if self.len == 0 {
            return None;
        }
        self.front.map(|n| {
            // SAFETY: the iterator borrows the list for 'a (shared), so nodes stay alive and unmodified; len > 0.
            let node = unsafe { &*n.as_ptr() };
            self.front = node.next;
            self.len -= 1;
            &node.elem
        })
    }
}
impl<'a, T> DoubleEndedIterator for Iter<'a, T> {
    fn next_back(&mut self) -> Option<&'a T> {
        if self.len == 0 {
            return None;
        }
        self.back.map(|n| {
            // SAFETY: as in `next`.
            let node = unsafe { &*n.as_ptr() };
            self.back = node.prev;
            self.len -= 1;
            &node.elem
        })
    }
}
struct IterMut<'a, T> {
    front: Link<T>,
    back: Link<T>,
    len: usize,
    _m: PhantomData<&'a mut T>,
}
impl<'a, T> Iterator for IterMut<'a, T> {
    type Item = &'a mut T;
    fn next(&mut self) -> Option<&'a mut T> {
        if self.len == 0 {
            return None;
        }
        self.front.map(|n| {
            // SAFETY: the list is mutably borrowed for 'a; each node is yielded at most once (len bounds it), so
            // the `&mut` items never alias each other.
            let node = unsafe { &mut *n.as_ptr() };
            self.front = node.next;
            self.len -= 1;
            &mut node.elem
        })
    }
}

static CREATED: AtomicUsize = AtomicUsize::new(0);
static DROPPED: AtomicUsize = AtomicUsize::new(0);
struct Tracked(u64);
impl Tracked {
    fn new(v: u64) -> Self {
        CREATED.fetch_add(1, SeqCst);
        Tracked(v)
    }
}
impl Drop for Tracked {
    fn drop(&mut self) {
        DROPPED.fetch_add(1, SeqCst);
    }
}

struct Rng(u64);
impl Rng {
    fn next(&mut self) -> u64 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        self.0
    }
}

fn main() {
    let mut d = Deque::new();
    d.push_back(2);
    d.push_back(3);
    d.push_front(1);
    assert_eq!(d.iter().copied().collect::<Vec<_>>(), [1, 2, 3]);
    assert_eq!(d.iter().rev().copied().collect::<Vec<_>>(), [3, 2, 1]);
    d.iter_mut().for_each(|x| *x *= 10);
    assert_eq!(
        (d.pop_front(), d.pop_back(), d.pop_back(), d.pop_back()),
        (Some(10), Some(30), Some(20), None)
    );
    assert_eq!(d.len(), 0);
    println!("basic deque ok");

    // Model test vs VecDeque with drop accounting
    let mut rng = Rng(0xABCDEF0123456789);
    for round in 0..5 {
        let mut mine: Deque<Tracked> = Deque::new();
        let mut model: VecDeque<u64> = VecDeque::new();
        for step in 0..4000u64 {
            let v = round * 100_000 + step;
            match rng.next() % 6 {
                0 | 1 => {
                    mine.push_front(Tracked::new(v));
                    model.push_front(v);
                }
                2 | 3 => {
                    mine.push_back(Tracked::new(v));
                    model.push_back(v);
                }
                4 => assert_eq!(mine.pop_front().map(|t| t.0), model.pop_front()),
                _ => assert_eq!(mine.pop_back().map(|t| t.0), model.pop_back()),
            }
            assert_eq!(mine.len(), model.len());
            if step % 50 == 0 {
                assert!(mine.iter().map(|t| t.0).eq(model.iter().copied()));
                assert!(
                    mine.iter()
                        .rev()
                        .map(|t| t.0)
                        .eq(model.iter().rev().copied())
                );
                mine.iter_mut().for_each(|t| t.0 += 1);
                model.iter_mut().for_each(|x| *x += 1);
            }
        }
        // `mine` dropped here with elements remaining
    }
    let (c, dr) = (CREATED.load(SeqCst), DROPPED.load(SeqCst));
    assert_eq!(c, dr, "leak or double free: created {c}, dropped {dr}");
    let mut big = Deque::new();
    for i in 0..200_000 {
        big.push_back(i);
    }
    drop(big);
    println!("random ops agree with VecDeque; {c} Tracked values created and dropped exactly once");
}
