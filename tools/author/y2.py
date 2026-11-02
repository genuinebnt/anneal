from author import T, write_track


def sub(s, old, new):
    """str.replace that fails loudly when `old` isn't there (a wrong solution that silently equals the reference)."""
    assert old in s, f"not found: {old[:60]!r}"
    return s.replace(old, new)


P = []

DROPS = """
        use std::cell::Cell;
        use std::rc::Rc;
        #[derive(Clone)]
        struct D(Rc<Cell<usize>>);
        impl Drop for D {
            fn drop(&mut self) {
                self.0.set(self.0.get() + 1);
            }
        }
"""

# ---------------------------------------------------------------------------------------------------------------
MYVEC = """
use std::alloc::{self, Layout};
use std::mem;
use std::ops::{Deref, DerefMut};
use std::ptr::{self, NonNull};

/// A growable array with its own allocation: pointer, capacity, length.
/// Invariant: `ptr[0..len]` are initialised, `ptr[len..cap]` are not, and `ptr` came from the global allocator
/// with `Layout::array::<T>(cap)` (or is dangling when `cap == 0` or `T` is zero-sized).
pub struct MyVec<T> {
    ptr: NonNull<T>,
    cap: usize,
    len: usize,
}

// SAFETY: MyVec owns its elements exactly like Vec<T> does.
unsafe impl<T: Send> Send for MyVec<T> {}
unsafe impl<T: Sync> Sync for MyVec<T> {}

impl<T> MyVec<T> {
    pub fn new() -> Self {
        // A zero-sized type never needs memory, so its capacity is "infinite".
        let cap = if mem::size_of::<T>() == 0 { usize::MAX } else { 0 };
        MyVec { ptr: NonNull::dangling(), cap, len: 0 }
    }

    pub fn len(&self) -> usize {
        self.len
    }

    pub fn is_empty(&self) -> bool {
        self.len == 0
    }

    pub fn capacity(&self) -> usize {
        self.cap
    }

    fn grow(&mut self) {
        assert!(mem::size_of::<T>() != 0, "capacity overflow");
        let new_cap = if self.cap == 0 { 4 } else { self.cap * 2 };
        let new_layout = Layout::array::<T>(new_cap).expect("capacity overflow");
        let raw = if self.cap == 0 {
            // SAFETY: new_layout has a non-zero size because T is not zero-sized and new_cap > 0.
            unsafe { alloc::alloc(new_layout) }
        } else {
            let old_layout = Layout::array::<T>(self.cap).unwrap();
            // SAFETY: ptr was allocated by this allocator with old_layout, and new_layout.size() > 0.
            unsafe { alloc::realloc(self.ptr.as_ptr() as *mut u8, old_layout, new_layout.size()) }
        };
        self.ptr = match NonNull::new(raw as *mut T) {
            Some(p) => p,
            None => alloc::handle_alloc_error(new_layout),
        };
        self.cap = new_cap;
    }

    pub fn push(&mut self, value: T) {
        if self.len == self.cap {
            self.grow();
        }
        // SAFETY: len < cap, so the slot is inside the allocation and holds no live value to overwrite.
        unsafe { ptr::write(self.ptr.as_ptr().add(self.len), value) };
        self.len += 1;
    }

    pub fn pop(&mut self) -> Option<T> {
        if self.len == 0 {
            return None;
        }
        self.len -= 1;
        // SAFETY: the slot at the old len - 1 was initialised, and len no longer covers it, so it is read out once.
        Some(unsafe { ptr::read(self.ptr.as_ptr().add(self.len)) })
    }

    /// Inserts at `index`, shifting later elements right. Panics if `index > len`.
    pub fn insert(&mut self, index: usize, value: T) {
        assert!(index <= self.len, "index out of bounds");
        if self.len == self.cap {
            self.grow();
        }
        // SAFETY: after the grow check there is room for one more element; `copy` handles the overlap.
        unsafe {
            let p = self.ptr.as_ptr().add(index);
            ptr::copy(p, p.add(1), self.len - index);
            ptr::write(p, value);
        }
        self.len += 1;
    }

    /// Removes and returns the element at `index`, shifting later elements left. Panics if `index >= len`.
    pub fn remove(&mut self, index: usize) -> T {
        assert!(index < self.len, "index out of bounds");
        self.len -= 1;
        // SAFETY: index was in bounds; we move the value out, then slide the tail over the hole.
        unsafe {
            let p = self.ptr.as_ptr().add(index);
            let out = ptr::read(p);
            ptr::copy(p.add(1), p, self.len - index);
            out
        }
    }

    /// Drops the elements past `new_len`.
    pub fn truncate(&mut self, new_len: usize) {
        while self.len > new_len {
            self.len -= 1;
            // SAFETY: that slot was initialised and is no longer counted in len, so it is dropped once.
            unsafe { ptr::drop_in_place(self.ptr.as_ptr().add(self.len)) };
        }
    }

    pub fn clear(&mut self) {
        self.truncate(0);
    }
}

impl<T> Deref for MyVec<T> {
    type Target = [T];
    fn deref(&self) -> &[T] {
        // SAFETY: the first len slots are initialised; a dangling pointer is fine for an empty slice.
        unsafe { std::slice::from_raw_parts(self.ptr.as_ptr(), self.len) }
    }
}

impl<T> DerefMut for MyVec<T> {
    fn deref_mut(&mut self) -> &mut [T] {
        // SAFETY: as in `deref`, and `&mut self` guarantees exclusive access.
        unsafe { std::slice::from_raw_parts_mut(self.ptr.as_ptr(), self.len) }
    }
}

impl<T> Drop for MyVec<T> {
    fn drop(&mut self) {
        self.clear();
        if self.cap != 0 && mem::size_of::<T>() != 0 {
            // SAFETY: ptr was allocated with exactly this layout and is freed once.
            unsafe { alloc::dealloc(self.ptr.as_ptr() as *mut u8, Layout::array::<T>(self.cap).unwrap()) };
        }
    }
}
"""

MYVEC_STARTER = """
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
"""

P.append(dict(
    slug="mini-vec", title="MiniVec: a Vec from raw allocation", level="hard", stage="build-it",
    tags=["unsafe", "alloc", "Layout", "NonNull", "ptr::read/write/copy", "Drop", "ZST", "Deref to slice"],
    teaches=["The three-field layout of `Vec`, and the invariant (slots below `len` are live, above are garbage) every `unsafe` block depends on.",
             "`alloc`/`realloc`/`dealloc` with a matching `Layout`, `handle_alloc_error`, and why a zero-sized type needs special handling.",
             "Moving values in and out of raw memory with `ptr::write`, `ptr::read` and `ptr::copy`, and dropping each element exactly once."],
    statement="""
        Implement `MyVec<T>` without using `Vec`. It holds a raw buffer you allocate with `std::alloc`, a capacity and a
        length. Capacity is **0** for a new vector, becomes **4** on the first push and **doubles** after that (`realloc`).

        `push`, `pop`, `insert` and `remove` move elements with raw pointer operations; `truncate` and `clear` drop elements; it
        dereferences to `[T]` so slices methods (`iter`, `sort`, indexing) work. Dropping a `MyVec` drops every element once
        and frees the buffer. A **zero-sized** `T` must never touch the allocator: its capacity is `usize::MAX`.

        Every `unsafe` block gets a `// SAFETY:` comment naming the invariant it relies on. The tests run with ordinary and
        panicking-drop types; run your solution under Miri (`cargo +nightly miri test`) as the follow-up exercise.
    """,
    starter=MYVEC_STARTER,
    solution=MYVEC,
    visible=[
        T("push_pop", "three values", "{ let mut v = MyVec::new(); v.push(1); v.push(2); v.push(3); (v.pop(), v.pop(), v.len(), v.pop(), v.pop()) }", "(Some(3), Some(2), 1, Some(1), None)"),
        T("derefs_to_a_slice", "index, iterate, sort", "{ let mut v = MyVec::new(); for x in [3, 1, 2] { v.push(x); } v.sort(); (v[0], v.iter().sum::<i32>(), v.to_vec()) }", "(1, 6, vec![1, 2, 3])"),
        T("insert_and_remove", "shift right then left", "{ let mut v = MyVec::new(); for x in [1, 2, 4] { v.push(x); } v.insert(2, 3); v.insert(0, 0); v.insert(5, 5); let r = v.remove(1); (r, v.to_vec()) }", "(1, vec![0, 2, 3, 4, 5])"),
        T("capacity_sequence", "0, 4, 8, 16", "{ let mut v = MyVec::new(); let mut caps = vec![v.capacity()]; for i in 0..9 { v.push(i); if i == 0 || i == 4 || i == 8 { caps.push(v.capacity()); } } caps }", "vec![0, 4, 8, 16]"),
        T("empty_vec", "no allocation yet", "{ let mut v: MyVec<String> = MyVec::new(); (v.len(), v.is_empty(), v.capacity(), v.pop(), v.iter().count()) }", "(0, true, 0, None, 0)"),
    ],
    hidden=[
        T("values_survive_reallocation", "strings with their own heap data", "{ let mut v = MyVec::new(); for i in 0..1000 { v.push(format!(\"item-{i}\")); } (v[0].clone(), v[999].clone(), v.iter().map(|s| s.len()).sum::<usize>()) }", "(\"item-0\".to_string(), \"item-999\".to_string(), 7890)"),
        T("zero_sized_types", "never allocates", "{ let mut v = MyVec::new(); for _ in 0..10_000 { v.push(()); } let n = v.len(); (n, v.capacity() == usize::MAX, v.pop(), v.len()) }", "(10_000, true, Some(()), 9_999)"),
        T("remove_at_the_ends", "first and last", "{ let mut v = MyVec::new(); for x in 1..=4 { v.push(x); } let a = v.remove(0); let b = v.remove(2); (a, b, v.to_vec()) }", "(1, 4, vec![2, 3])"),
        T("truncate_and_clear", "shorten then empty", "{ let mut v = MyVec::new(); for x in 0..10 { v.push(x); } v.truncate(20); let a = v.len(); v.truncate(4); let b = v.to_vec(); v.clear(); (a, b, v.len()) }", "(10, vec![0, 1, 2, 3], 0)"),
        T("insert_past_the_end_panics", "index > len", "std::panic::catch_unwind(|| { let mut v = MyVec::new(); v.push(1); v.insert(3, 2); }).is_err()", "true"),
        T("remove_out_of_range_panics", "index == len", "std::panic::catch_unwind(|| { let mut v = MyVec::new(); v.push(1); v.remove(1); }).is_err()", "true"),
        T("deref_mut_writes", "mutate through the slice", "{ let mut v = MyVec::new(); for x in 0..4 { v.push(x); } for x in v.iter_mut() { *x *= 10; } v.to_vec() }", "vec![0, 10, 20, 30]"),
        T("is_send_and_sync", "owning a Send type", "{ fn needs<X: Send + Sync>() {} needs::<MyVec<u32>>(); true }", "true"),
        """
        #[test]
        fn drops_every_element_exactly_once() {
            use std::cell::Cell;
            use std::rc::Rc;
            struct D(Rc<Cell<usize>>);
            impl Drop for D {
                fn drop(&mut self) {
                    self.0.set(self.0.get() + 1);
                }
            }
            let c = Rc::new(Cell::new(0));
            {
                let mut v = MyVec::new();
                for _ in 0..10 {
                    v.push(D(c.clone()));
                }
                check!("nothing dropped while they are in the vector", c.get(), 0);
                let popped = v.pop().unwrap();
                check!("pop moves out without dropping", c.get(), 0);
                drop(popped);
                check!("the popped value dropped once", c.get(), 1);
                let removed = v.remove(3);
                check!("remove moves out without dropping", c.get(), 1);
                drop(removed);
                v.truncate(5);
                check!("truncate to 5 dropped the tail: 1 + 1 + 3", c.get(), 5);
                v.insert(0, D(c.clone()));
                v.clear();
                check!("clear dropped the remaining 6: 5 + 6", c.get(), 11);
                v.push(D(c.clone()));
            }
            check!("dropping the vector dropped the last element", c.get(), 12);
        }

        #[test]
        fn random_ops_vs_std_vec() {
            let mut rng = anneal_prelude::Rng::new(6601);
            let mut mine = MyVec::new();
            let mut model: Vec<String> = Vec::new();
            for step in 0..20_000 {
                match rng.below(6) {
                    0 | 1 => {
                        mine.push(format!("p{step}"));
                        model.push(format!("p{step}"));
                    }
                    2 => check!("pop", mine.pop(), model.pop()),
                    3 => {
                        let i = rng.below(model.len() + 1);
                        mine.insert(i, format!("i{step}"));
                        model.insert(i, format!("i{step}"));
                    }
                    4 if !model.is_empty() => {
                        let i = rng.below(model.len());
                        check!("remove", mine.remove(i), model.remove(i));
                    }
                    _ if rng.below(50) == 0 => {
                        let n = rng.below(model.len() + 1);
                        mine.truncate(n);
                        model.truncate(n);
                    }
                    _ => {}
                }
                check!("same length", mine.len(), model.len());
            }
            check!("same contents", mine.to_vec(), model);
        }
        """,
    ],
    wrong=dict(
        forgets_to_drop_elements=sub(MYVEC, "    fn drop(&mut self) {\n        self.clear();\n", "    fn drop(&mut self) {\n"),
        grows_by_one=sub(MYVEC, "if self.cap == 0 { 4 } else { self.cap * 2 }", "self.cap + 1"),
        remove_loses_the_last_element=sub(MYVEC, "ptr::copy(p.add(1), p, self.len - index);\n            out", "ptr::copy(p.add(1), p, (self.len - index).saturating_sub(1));\n            out"),
        truncate_leaks=sub(MYVEC, "while self.len > new_len {\n            self.len -= 1;\n            // SAFETY: that slot was initialised and is no longer counted in len, so it is dropped once.\n            unsafe { ptr::drop_in_place(self.ptr.as_ptr().add(self.len)) };\n        }", "if new_len < self.len {\n            self.len = new_len;\n        }"),
        zst_goes_to_the_allocator=sub(MYVEC, "let cap = if mem::size_of::<T>() == 0 { usize::MAX } else { 0 };", "let cap = 0;"),
    ),
    hints=[("approach", "Three fields: `ptr: NonNull<T>`, `cap`, `len`. `push` grows when `len == cap`, writes with `ptr::write(ptr.add(len), v)` and bumps `len`. `pop` decrements `len` first, then `ptr::read`s that slot. `insert`/`remove` use `ptr::copy` (memmove: overlapping is allowed) to shift the tail."),
           ("rust", "Grow: `Layout::array::<T>(new_cap)`; first time `alloc::alloc(layout)`, later `alloc::realloc(ptr as *mut u8, old_layout, new_layout.size())`; null means `handle_alloc_error(layout)`. `NonNull::dangling()` is the pointer for an empty or zero-sized vec. `Drop`: `truncate(0)` then `dealloc` with the layout for `cap`, only if `cap != 0` and `T` is not zero-sized."),
           ("edge", "Update `len` before running anything that could panic (a `Drop` impl): `truncate` decrements `len` first, then `drop_in_place`s. `remove` shifts `len - index - 1` elements *after* decrementing `len`, which is `len - index` afterwards. Never allocate with a zero-size layout: that is undefined behaviour.")],
    notes=("This is `RawVec` plus `Vec` from the standard library in about 100 lines, the same shape as the Rustonomicon's 'Implementing Vec' chapter. The skill it builds is stating an invariant (live prefix, dead suffix, allocation layout) and checking each `unsafe` block against it: that habit is what makes unsafe code in a database buffer pool or a lock-free queue reviewable.", "O(1) amortised push, O(n) insert/remove", "O(capacity)"),
    follow_up="1) Run it under Miri (`cargo +nightly miri test`) and then plant a bug (read `ptr.add(len)` in `pop`) to see the error. 2) Add `IntoIter` (it must own the buffer and drop the unconsumed tail) and `drain`; why is leaking a `Drain` (the Leakpocalypse) the reason `Drain` sets `len = 0` first? 3) Split out a `RawVec<T>` that owns only the allocation, so `Drop` for the buffer and for the elements are separate. 4) Add `with_capacity` and `reserve`, with `Layout::array` overflow checks: what attack does the overflow check stop? 5) What changes for `T` with alignment above 16, or for `Vec<T, A>` with a custom allocator?",
    related=["F3", "C3", "D5"],
))

# ---------------------------------------------------------------------------------------------------------------
LIST = """
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
                self.tail = (*t.as_ptr()).prev;
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
"""

LIST_STARTER = """
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
"""

P.append(dict(
    slug="unsafe-linked-list", title="An unsafe doubly linked list", level="hard", stage="build-it",
    tags=["unsafe", "NonNull", "Box::leak / from_raw", "PhantomData", "DoubleEndedIterator", "iterative Drop", "invariants"],
    teaches=["Owning heap nodes through `NonNull` pointers: `Box::leak` to create, `Box::from_raw` to free exactly once.",
             "Writing the list's invariants first (ends have no outer link, `n.next.prev == n`, `len` counts nodes) and checking each operation against them.",
             "Iterators that hand out `&T` / `&mut T` safely: `PhantomData<&'a T>`, a remaining-count so the two ends never cross, disjoint `&mut`."],
    statement="""
        Implement the doubly linked list that safe Rust famously cannot express cheaply: `push_front`, `push_back`,
        `pop_front`, `pop_back`, `front`, `back`, `len`, `append` (O(1), empties `other`), `retain` (unlink in place), and
        `iter` / `iter_mut` that are **double-ended** (calling `next` and `next_back` alternately yields every element once and
        the two ends meet cleanly).

        Nodes are heap allocations referenced by `NonNull`. Dropping the list drops every element once, **without recursion**
        (a million-node list must not overflow the stack). Document the invariants in a comment and give each `unsafe` block a
        `// SAFETY:` justification.
    """,
    starter=LIST_STARTER,
    solution=LIST,
    visible=[
        T("push_pop_both_ends", "mix of ends", "{ let mut l = List::new(); l.push_back(2); l.push_back(3); l.push_front(1); (l.pop_front(), l.pop_back(), l.pop_back(), l.pop_back(), l.len()) }", "(Some(1), Some(3), Some(2), None, 0)"),
        T("iter_in_order", "front to back", "{ let mut l = List::new(); for x in [1, 2, 3] { l.push_back(x); } l.push_front(0); l.iter().copied().collect::<Vec<_>>() }", "vec![0, 1, 2, 3]"),
        T("iter_reversed", "back to front", "{ let mut l = List::new(); for x in [1, 2, 3] { l.push_back(x); } l.iter().rev().copied().collect::<Vec<_>>() }", "vec![3, 2, 1]"),
        T("front_and_back", "peek", "{ let mut l = List::new(); l.push_back('a'); l.push_back('b'); (l.front().copied(), l.back().copied()) }", "(Some('a'), Some('b'))"),
        T("append_moves_everything", "other ends up empty", "{ let mut a = List::new(); let mut b = List::new(); a.push_back(1); b.push_back(2); b.push_back(3); a.append(&mut b); (a.iter().copied().collect::<Vec<_>>(), a.len(), b.len(), b.pop_front()) }", "(vec![1, 2, 3], 3, 0, None)"),
        T("retain_keeps_matches", "keep even numbers", "{ let mut l = List::new(); for x in 1..=8 { l.push_back(x); } l.retain(|x| x % 2 == 0); (l.iter().copied().collect::<Vec<_>>(), l.len()) }", "(vec![2, 4, 6, 8], 4)"),
    ],
    hidden=[
        T("empty_list", "nothing there", "{ let mut l: List<u8> = List::new(); (l.pop_front(), l.pop_back(), l.front().copied(), l.back().copied(), l.is_empty(), l.iter().next().copied()) }", "(None, None, None, None, true, None)"),
        T("single_element_both_ways", "the one node is both head and tail", "{ let mut l = List::new(); l.push_back(1); let a = l.pop_back(); l.push_front(2); let b = l.pop_front(); l.push_back(3); (a, b, l.pop_back(), l.pop_front(), l.len()) }", "(Some(1), Some(2), Some(3), None, 0)"),
        T("push_front_on_an_empty_list_sets_the_tail", "then pop_back and back()", "{ let mut l = List::new(); l.push_front(1); (l.back().copied(), l.iter().rev().count(), l.pop_back(), l.len()) }", "(Some(1), 1, Some(1), 0)"),
        T("ends_meet_in_the_middle", "alternate next and next_back", "{ let mut l = List::new(); for x in 1..=5 { l.push_back(x); } let mut it = l.iter(); let v = (it.next().copied(), it.next_back().copied(), it.next().copied(), it.next_back().copied(), it.next().copied(), it.next().copied(), it.next_back().copied()); v }", "(Some(1), Some(5), Some(2), Some(4), Some(3), None, None)"),
        T("iter_mut_edits", "double every element", "{ let mut l = List::new(); for x in 1..=4 { l.push_back(x); } for x in l.iter_mut() { *x *= 2; } l.iter().copied().collect::<Vec<_>>() }", "vec![2, 4, 6, 8]"),
        T("iter_mut_from_the_back", "mutate while reversed", "{ let mut l = List::new(); for x in 1..=3 { l.push_back(x); } for (i, x) in l.iter_mut().rev().enumerate() { *x += i * 10; } l.iter().copied().collect::<Vec<_>>() }", "vec![21, 12, 3]"),
        T("append_to_and_from_empty", "empty cases", "{ let mut a: List<i32> = List::new(); let mut b = List::new(); b.push_back(1); a.append(&mut b); let mut c = List::new(); a.append(&mut c); a.push_back(2); (a.iter().copied().collect::<Vec<_>>(), a.back().copied()) }", "(vec![1, 2], Some(2))"),
        T("retain_everything_and_nothing", "all, then none", "{ let mut l = List::new(); for x in 0..5 { l.push_back(x); } l.retain(|_| true); let n = l.len(); l.retain(|_| false); (n, l.len(), l.pop_front(), l.front().copied()) }", "(5, 0, None, None)"),
        T("retain_first_and_last", "remove head and tail", "{ let mut l = List::new(); for x in 0..5 { l.push_back(x); } l.retain(|x| *x != 0 && *x != 4); l.push_front(9); l.push_back(8); (l.iter().copied().collect::<Vec<_>>(), l.iter().rev().copied().collect::<Vec<_>>()) }", "(vec![9, 1, 2, 3, 8], vec![8, 3, 2, 1, 9])"),
        """
        #[test]
        fn drops_every_element_once() {
            use std::cell::Cell;
            use std::rc::Rc;
            struct D(Rc<Cell<usize>>);
            impl Drop for D {
                fn drop(&mut self) {
                    self.0.set(self.0.get() + 1);
                }
            }
            let c = Rc::new(Cell::new(0));
            {
                let mut l = List::new();
                for _ in 0..50 {
                    l.push_back(D(c.clone()));
                }
                let a = l.pop_front();
                check!("pop moves out without dropping", c.get(), 0);
                drop(a);
                let mut n = 0;
                l.retain(|_| {
                    n += 1;
                    n % 2 == 0
                });
                check!("retain dropped 1 + 25 of the 50", c.get(), 1 + 25);
            }
            check!("the list drop took the other 24", c.get(), 50);
        }

        #[test]
        fn a_million_nodes_drop_without_recursion() {
            let mut l = List::new();
            for i in 0..1_000_000u32 {
                l.push_back(i);
            }
            check!("len", l.len(), 1_000_000);
            drop(l);
        }

        #[test]
        fn random_ops_vs_vecdeque() {
            use std::collections::VecDeque;
            let mut rng = anneal_prelude::Rng::new(6602);
            let mut l: List<u32> = List::new();
            let mut m: VecDeque<u32> = VecDeque::new();
            let mut spare: List<u32> = List::new();
            for step in 0..20_000u32 {
                match rng.below(8) {
                    0 => { l.push_front(step); m.push_front(step); }
                    1 | 2 => { l.push_back(step); m.push_back(step); }
                    3 => check!("pop_front", l.pop_front(), m.pop_front()),
                    4 => check!("pop_back", l.pop_back(), m.pop_back()),
                    5 => {
                        for k in 0..rng.below(4) as u32 {
                            spare.push_back(step * 10 + k);
                            m.push_back(step * 10 + k);
                        }
                        l.append(&mut spare);
                    }
                    6 if rng.below(40) == 0 => {
                        let modulus = 2 + rng.below(3) as u32;
                        l.retain(|x| x % modulus != 0);
                        m.retain(|x| x % modulus != 0);
                    }
                    _ => {}
                }
                check!("len", l.len(), m.len());
            }
            check!("contents front to back", l.iter().copied().collect::<Vec<_>>(), m.iter().copied().collect::<Vec<_>>());
            check!("contents back to front", l.iter().rev().copied().collect::<Vec<_>>(), m.iter().rev().copied().collect::<Vec<_>>());
            check!("spare stayed empty", spare.len(), 0);
        }
        """,
    ],
    wrong=dict(
        drop_leaks_nodes=sub(LIST, "        while self.pop_front().is_some() {}\n", "        self.head = None;\n"),
        append_forgets_len=sub(LIST, "self.len += other.len;\n        other.head = None;", "other.head = None;"),
        next_back_walks_forward=sub(LIST, "                self.tail = (*t.as_ptr()).prev;\n                &(*t.as_ptr()).elem", "                self.tail = (*t.as_ptr()).next;\n                &(*t.as_ptr()).elem"),
        pop_back_keeps_len=sub(LIST, "            self.len -= 1;\n            boxed.elem\n        })\n    }\n\n    pub fn front", "            boxed.elem\n        })\n    }\n\n    pub fn front"),
        push_front_on_empty_forgets_tail=sub(LIST, "None => self.tail = Some(node),\n        }\n        self.head = Some(node);", "None => {}\n        }\n        self.head = Some(node);"),
    ),
    hints=[("approach", "Write the invariants as a comment: both ends `None` together, `head.prev == None`, `tail.next == None`, `n.next.prev == n`, `len` counts nodes. Every method is 'make the new links, then restore the invariants at the ends'. `push_front` on an empty list must set the tail too."),
           ("rust", "Create: `NonNull::from(Box::leak(Box::new(node)))`. Free: `Box::from_raw(ptr.as_ptr())` once, then read its `next`/`prev` fields before the box goes away. Iterators keep `head`, `tail` and a **remaining count**; stop when the count is 0 so `next` and `next_back` never hand out the same node."),
           ("edge", "`Drop` must loop (`while self.pop_front().is_some() {}`): the derived recursive drop of a `Box<Node>` chain overflows the stack at around 100k nodes. In `retain` read `next` before you free the current node.")],
    notes=("`std::collections::LinkedList` is exactly this (and the 'Learn Rust With Entirely Too Many Linked Lists' book builds the same thing in stages). It exists in `std` mostly as a lesson: for real work a `VecDeque` is faster, and intrusive lists (the node lives inside the object, as in the Linux kernel's `list_head`) are where the technique pays off, for example in an LRU cache with O(1) unlink by handle.", "O(1) per end operation", "O(n)"),
    follow_up="1) Run the tests under Miri; then change `Iter` to keep `&'a Node<T>` instead of `NonNull` and see which aliasing rule `IterMut` would break (Stacked/Tree Borrows). 2) Add a `Cursor`/`CursorMut` with `move_next`, `insert_after`, `remove_current`, and `split_off`: which invariants does `split_off` have to repair? 3) Make an intrusive LRU: a `HashMap<K, NonNull<Node>>` plus this list, with O(1) get-and-promote. 4) Why does `PhantomData<Box<Node<T>>>` matter for the drop checker, and what changes if you leave it out? 5) Should `List<T>` be `Send`/`Sync`? Write the `unsafe impl`s and the argument.",
    related=["Y2", "D5", "S7"],
))

# ---------------------------------------------------------------------------------------------------------------
AVEC = """
use std::mem::{self, MaybeUninit};
use std::ops::{Deref, DerefMut};
use std::ptr;

/// A vector with room for `N` elements stored inline (no heap). Slots `buf[..len]` are initialised, the rest are not.
pub struct ArrayVec<T, const N: usize> {
    buf: [MaybeUninit<T>; N],
    len: usize,
}

impl<T, const N: usize> ArrayVec<T, N> {
    pub fn new() -> Self {
        // SAFETY: an array of `MaybeUninit` needs no initialisation.
        ArrayVec { buf: unsafe { MaybeUninit::<[MaybeUninit<T>; N]>::uninit().assume_init() }, len: 0 }
    }

    pub fn len(&self) -> usize {
        self.len
    }

    pub fn is_empty(&self) -> bool {
        self.len == 0
    }

    pub fn capacity(&self) -> usize {
        N
    }

    pub fn is_full(&self) -> bool {
        self.len == N
    }

    /// Returns the value back if the vector is full.
    pub fn try_push(&mut self, value: T) -> Result<(), T> {
        if self.len == N {
            return Err(value);
        }
        self.buf[self.len].write(value);
        self.len += 1;
        Ok(())
    }

    pub fn pop(&mut self) -> Option<T> {
        if self.len == 0 {
            return None;
        }
        self.len -= 1;
        // SAFETY: slot len (the old last) was initialised and is no longer counted, so it is read out once.
        Some(unsafe { self.buf[self.len].assume_init_read() })
    }

    /// Inserts at `index` (0..=len); returns the value if full. Panics if `index > len`.
    pub fn try_insert(&mut self, index: usize, value: T) -> Result<(), T> {
        assert!(index <= self.len, "index out of bounds");
        if self.len == N {
            return Err(value);
        }
        // SAFETY: there is a free slot at len; `copy` handles the overlap while shifting the tail right by one.
        unsafe {
            let p = self.buf.as_mut_ptr().add(index);
            ptr::copy(p, p.add(1), self.len - index);
            (*p).write(value);
        }
        self.len += 1;
        Ok(())
    }

    /// Removes and returns the element at `index`, keeping order. Panics if `index >= len`.
    pub fn remove(&mut self, index: usize) -> T {
        assert!(index < self.len, "index out of bounds");
        self.len -= 1;
        // SAFETY: index was live; move it out, then slide the tail over the hole.
        unsafe {
            let p = self.buf.as_mut_ptr().add(index);
            let out = (*p).assume_init_read();
            ptr::copy(p.add(1), p, self.len - index);
            out
        }
    }

    /// Removes the element at `index` by swapping the last element into its place (O(1), order not kept).
    pub fn swap_remove(&mut self, index: usize) -> T {
        assert!(index < self.len, "index out of bounds");
        let last = self.len - 1;
        self.as_mut_slice().swap(index, last);
        self.pop().unwrap()
    }

    pub fn as_slice(&self) -> &[T] {
        // SAFETY: the first len slots are initialised and MaybeUninit<T> has the layout of T.
        unsafe { std::slice::from_raw_parts(self.buf.as_ptr() as *const T, self.len) }
    }

    pub fn as_mut_slice(&mut self) -> &mut [T] {
        // SAFETY: as in `as_slice`, with exclusive access.
        unsafe { std::slice::from_raw_parts_mut(self.buf.as_mut_ptr() as *mut T, self.len) }
    }

    /// The elements as a real `[T; N]` if exactly `N` are present, else `self` back.
    pub fn into_array(self) -> Result<[T; N], Self> {
        if self.len != N {
            return Err(self);
        }
        // SAFETY: all N slots are initialised; we read them out as an array and forget `self` so they are not dropped twice.
        let arr = unsafe { ptr::read(self.buf.as_ptr() as *const [T; N]) };
        mem::forget(self);
        Ok(arr)
    }
}

impl<T, const N: usize> Deref for ArrayVec<T, N> {
    type Target = [T];
    fn deref(&self) -> &[T] {
        self.as_slice()
    }
}

impl<T, const N: usize> DerefMut for ArrayVec<T, N> {
    fn deref_mut(&mut self) -> &mut [T] {
        self.as_mut_slice()
    }
}

impl<T, const N: usize> Drop for ArrayVec<T, N> {
    fn drop(&mut self) {
        // SAFETY: exactly the first len slots are live, and each is dropped once here.
        unsafe { ptr::drop_in_place(self.as_mut_slice()) };
    }
}
"""

AVEC_STARTER = """
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
"""

P.append(dict(
    slug="array-vec", title="ArrayVec: a const-generic vector on the stack", level="medium", stage="understand-it",
    tags=["MaybeUninit", "const generics", "unsafe", "drop_in_place", "mem::forget", "no heap", "smallvec"],
    teaches=["`MaybeUninit<T>`: storage that is not yet a `T`, and the invariant 'a prefix is initialised' that replaces the compiler's guarantee.",
             "A const generic `N` as a type-level capacity, so `ArrayVec<u8, 16>` is a plain 16-byte-plus-length value with no allocation.",
             "Taking values out of a type that implements `Drop`: `mem::forget` after reading them out, or you drop them twice."],
    statement="""
        Implement `ArrayVec<T, N>`: up to `N` elements stored **inline**, in `[MaybeUninit<T>; N]` plus a length. It is what
        `smallvec`, `arrayvec` and embedded code use when a heap allocation per small list is too expensive.

        `try_push` and `try_insert` give the value back (`Err(value)`) when full instead of allocating or panicking.
        `pop`, `remove` (order kept), `swap_remove` (O(1), order not kept), `as_slice`, `Deref` to `[T]`, and `into_array`
        (`Ok([T; N])` if exactly full, else `Err(self)`) round it out. Dropping the vector drops exactly the live elements.
    """,
    starter=AVEC_STARTER,
    solution=AVEC,
    visible=[
        T("push_until_full", "capacity 3", "{ let mut v: ArrayVec<i32, 3> = ArrayVec::new(); let r: Vec<Result<(), i32>> = (1..=4).map(|x| v.try_push(x)).collect(); (r, v.len(), v.is_full(), v.to_vec()) }", "(vec![Ok(()), Ok(()), Ok(()), Err(4)], 3, true, vec![1, 2, 3])"),
        T("pop_order", "last in, first out", "{ let mut v: ArrayVec<_, 4> = ArrayVec::new(); v.try_push('a').unwrap(); v.try_push('b').unwrap(); (v.pop(), v.pop(), v.pop(), v.len()) }", "(Some('b'), Some('a'), None, 0)"),
        T("insert_and_remove_keep_order", "middle edits", "{ let mut v: ArrayVec<i32, 6> = ArrayVec::new(); for x in [1, 2, 4] { v.try_push(x).unwrap(); } v.try_insert(2, 3).unwrap(); v.try_insert(0, 0).unwrap(); let r = v.remove(1); (r, v.to_vec()) }", "(1, vec![0, 2, 3, 4])"),
        T("swap_remove_is_unordered", "last element fills the hole", "{ let mut v: ArrayVec<i32, 5> = ArrayVec::new(); for x in 1..=5 { v.try_push(x).unwrap(); } let r = v.swap_remove(1); (r, v.to_vec()) }", "(2, vec![1, 5, 3, 4])"),
        T("derefs_to_a_slice", "slice methods", "{ let mut v: ArrayVec<i32, 4> = ArrayVec::new(); for x in [3, 1, 2] { v.try_push(x).unwrap(); } v.sort(); (v.iter().sum::<i32>(), v.contains(&2), v.to_vec()) }", "(6, true, vec![1, 2, 3])"),
    ],
    hidden=[
        T("zero_capacity", "N = 0", "{ let mut v: ArrayVec<i32, 0> = ArrayVec::new(); (v.try_push(1), v.pop(), v.is_full(), v.is_empty(), v.capacity()) }", "(Err(1), None, true, true, 0)"),
        T("insert_at_len_is_allowed", "append through insert", "{ let mut v: ArrayVec<i32, 3> = ArrayVec::new(); v.try_insert(0, 1).unwrap(); v.try_insert(1, 2).unwrap(); (v.to_vec(), v.try_insert(2, 3), v.try_insert(0, 9)) }", "(vec![1, 2], Ok(()), Err(9))"),
        T("insert_past_len_panics", "index > len", "std::panic::catch_unwind(|| { let mut v: ArrayVec<i32, 4> = ArrayVec::new(); v.try_push(1).unwrap(); let _ = v.try_insert(3, 2); }).is_err()", "true"),
        T("remove_out_of_range_panics", "index == len", "std::panic::catch_unwind(|| { let mut v: ArrayVec<i32, 4> = ArrayVec::new(); v.try_push(1).unwrap(); v.remove(1); }).is_err()", "true"),
        T("into_array_when_full", "exactly N elements", "{ let mut v: ArrayVec<String, 2> = ArrayVec::new(); v.try_push(\"a\".to_string()).unwrap(); v.try_push(\"b\".to_string()).unwrap(); v.into_array().ok() }", "Some([\"a\".to_string(), \"b\".to_string()])"),
        T("into_array_when_not_full", "gives the vector back", "{ let mut v: ArrayVec<i32, 3> = ArrayVec::new(); v.try_push(7).unwrap(); match v.into_array() { Ok(_) => None, Err(back) => Some(back.to_vec()) } }", "Some(vec![7])"),
        T("strings_move_without_copying_bits_twice", "owned heap data", "{ let mut v: ArrayVec<String, 8> = ArrayVec::new(); for i in 0..8 { v.try_push(format!(\"s{i}\")).unwrap(); } let a = v.remove(2); let b = v.swap_remove(0); (a, b, v.len(), v[0].clone()) }", "(\"s2\".to_string(), \"s0\".to_string(), 6, \"s7\".to_string())"),
        T("is_inline", "no heap, size is N elements plus a length", "std::mem::size_of::<ArrayVec<u32, 16>>() <= 16 * 4 + 8", "true"),
        """
        #[test]
        fn drops_exactly_the_live_elements() {
            use std::cell::Cell;
            use std::rc::Rc;
            struct D(Rc<Cell<usize>>);
            impl Drop for D {
                fn drop(&mut self) {
                    self.0.set(self.0.get() + 1);
                }
            }
            let c = Rc::new(Cell::new(0));
            {
                let mut v: ArrayVec<D, 8> = ArrayVec::new();
                for _ in 0..8 {
                    v.try_push(D(c.clone())).ok().unwrap();
                }
                let rejected = v.try_push(D(c.clone()));
                check!("a rejected push hands the value back, not dropped yet", (rejected.is_err(), c.get()), (true, 0));
                drop(rejected);
                check!("dropping the rejected value drops it once", c.get(), 1);
                drop(v.pop());
                drop(v.remove(0));
                drop(v.swap_remove(0));
                check!("three removed values dropped: 1 + 3", c.get(), 4);
            }
            check!("the five left dropped with the vector: 4 + 5", c.get(), 9);
            let c2 = Rc::new(Cell::new(0));
            let mut full: ArrayVec<D, 3> = ArrayVec::new();
            for _ in 0..3 {
                full.try_push(D(c2.clone())).ok().unwrap();
            }
            let arr = full.into_array().ok().unwrap();
            check!("into_array moves the elements: no drops yet", c2.get(), 0);
            drop(arr);
            check!("each of the three dropped once", c2.get(), 3);
        }

        #[test]
        fn random_ops_vs_std_vec() {
            let mut rng = anneal_prelude::Rng::new(6603);
            let mut mine: ArrayVec<String, 16> = ArrayVec::new();
            let mut model: Vec<String> = Vec::new();
            for step in 0..20_000 {
                match rng.below(5) {
                    0 | 1 => {
                        let r = mine.try_push(format!("p{step}"));
                        if model.len() < 16 {
                            model.push(format!("p{step}"));
                            check!("push accepted below capacity", r.is_ok(), true);
                        } else {
                            check!("push rejected at capacity", r, Err(format!("p{step}")));
                        }
                    }
                    2 => check!("pop", mine.pop(), model.pop()),
                    3 => {
                        let i = rng.below(model.len() + 1);
                        let r = mine.try_insert(i, format!("i{step}"));
                        if model.len() < 16 {
                            model.insert(i, format!("i{step}"));
                            check!("insert accepted below capacity", r.is_ok(), true);
                        } else {
                            check!("insert rejected at capacity", r, Err(format!("i{step}")));
                        }
                    }
                    _ if !model.is_empty() => {
                        let i = rng.below(model.len());
                        if rng.bool() {
                            check!("remove", mine.remove(i), model.remove(i));
                        } else {
                            check!("swap_remove", mine.swap_remove(i), model.swap_remove(i));
                        }
                    }
                    _ => {}
                }
                check!("same contents", mine.to_vec(), model.clone());
            }
        }
        """,
    ],
    wrong=dict(
        drop_leaks=sub(AVEC, "        unsafe { ptr::drop_in_place(self.as_mut_slice()) };\n", "        let _ = &self.buf;\n"),
        full_push_swallows_the_value=sub(AVEC, "pub fn try_push(&mut self, value: T) -> Result<(), T> {\n        if self.len == N {\n            return Err(value);\n        }", "pub fn try_push(&mut self, value: T) -> Result<(), T> {\n        if self.len == N {\n            return Ok(());\n        }"),
        remove_loses_the_last_element=sub(AVEC, "ptr::copy(p.add(1), p, self.len - index);\n            out", "ptr::copy(p.add(1), p, (self.len - index).saturating_sub(1));\n            out"),
        swap_remove_keeps_order=sub(AVEC, "let last = self.len - 1;\n        self.as_mut_slice().swap(index, last);\n        self.pop().unwrap()", "self.remove(index)"),
        insert_at_len_rejected=sub(AVEC, "assert!(index <= self.len, \"index out of bounds\");\n        if self.len == N {\n            return Err(value);\n        }", "assert!(index < self.len, \"index out of bounds\");\n        if self.len == N {\n            return Err(value);\n        }"),
    ),
    hints=[("approach", "`buf: [MaybeUninit<T>; N]` and `len`; the invariant is 'exactly `buf[..len]` is initialised'. `try_push` writes into `buf[len]` with `MaybeUninit::write`; `pop` decrements `len` then `assume_init_read`s that slot; `insert`/`remove` shift the tail with `ptr::copy`."),
           ("rust", "Create the array with `unsafe { MaybeUninit::<[MaybeUninit<T>; N]>::uninit().assume_init() }` (sound because the inner type needs no init). View the prefix as `&[T]` with `slice::from_raw_parts(self.buf.as_ptr() as *const T, self.len)`. `Drop`: `ptr::drop_in_place(self.as_mut_slice())`."),
           ("edge", "`into_array`: after `ptr::read`ing all `N` slots as a `[T; N]` you must `mem::forget(self)`, or `Drop` drops the same elements again. `try_push` on a full vector must give the value back, not drop it. `N = 0` has to work.")],
    notes=("This is the core of the `arrayvec` and `heapless` crates, and `smallvec` adds a spill-to-heap case. Const generics move the capacity into the type, so there is no allocation, no pointer chase and the size is known at compile time: the right shape for per-packet scratch lists, embedded firmware, or the small-neighbours case of a graph node.", "O(1) push/pop, O(n) insert/remove", "inline: N * size_of::<T>() + 8"),
    follow_up="1) Add `IntoIterator` that owns the elements (it must drop the unconsumed ones and track a start index). 2) Make `new` a `const fn` (what does `MaybeUninit::uninit_array` or an inline const give you?). 3) Add `extend_from_slice` for `T: Copy` using one `copy_nonoverlapping`, and `drain(range)` and explain why a leaked `Drain` must not leave `len` pointing at moved-out slots. 4) Implement a `SmallVec<T, N>` enum of inline-or-`Vec`, and decide when to spill. 5) Which of `Send`/`Sync`/`Clone`/`Debug` are derived for free, and why is `Clone` not (a panic mid-clone must drop what was cloned so far)?",
    related=["F2", "F3", "D13"],
))

P.sort(key=lambda p: ["array-vec", "mini-vec", "unsafe-linked-list"].index(p["slug"]))

STAGES = [("understand-it", "Understand it", "medium"), ("build-it", "Build it", "hard")]

if __name__ == "__main__":
    n = write_track("y2-unsafe-rust", "Y2", "Unsafe Rust", "Y", "sde3", 2,
                    "Unsafe Rust by building the standard containers yourself: an inline array vector, a vector on raw allocation, and a doubly linked list.",
                    STAGES, P)
    print("Y2", n)
