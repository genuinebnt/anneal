//! RECIPE 632 (interview): "implement it yourself" Rust questions: Vec, Box, Rc/Weak, HashMap, String, iterator adapters, Mutex, scope, Cow, errors, operators.
//! LEVEL: advanced
//! Use in: Rust interviews that ask you to rebuild a std type, and as a tour of where `unsafe` is justified (every unsafe block here has a SAFETY comment).   Run: cargo run --release --example implement_std_from_scratch
//! COMMON USES: understanding std containers, writing custom collections and smart pointers, lock primitives, library-style iterator and operator APIs.
#![allow(dead_code)]
//! Read: https://rust-unofficial.github.io/too-many-lists/ , https://doc.rust-lang.org/nomicon/vec/vec.html , https://doc.rust-lang.org/nomicon/arc-mutex/arc.html
//! See also: examples/interview/rust_unsafe_memory_layout_qa.rs, examples/interview/linked_lists_in_rust.rs, examples/language/implementing_std_traits.rs
//!
//! QUESTION BANK
//! Q1. Implement `MyVec<T>`: growth, push/pop/insert/remove, drain, Deref to a slice, and a Drop that drops each element exactly once. [Hard] [asked: very common]
//! Q2. Implement `MyBox<T>`: heap allocation, Deref/DerefMut, Drop, and why `*b` can move out of a real Box. [Medium] [asked: very common]
//! Q3. Implement a single-threaded `MyRc<T>` and `MyWeak<T>` with strong/weak counts and upgrade. [Hard] [asked: very common]
//! Q4. Implement a generic Stack and a Queue from two stacks with amortized O(1) operations. [Easy] [asked: very common]
//! Q5. Implement a singly linked stack with iter, iter_mut, into_iter, and a Drop that survives a million nodes. [Medium] [asked: very common]
//! Q6. Implement `MyHashMap<K, V>` with separate chaining and resizing, looking up `String` keys by `&str`. [Hard] [asked: very common]
//! Q7. Implement `MyString`: UTF-8 invariant, push/pop of chars, and a safe `Deref<Target = str>`. [Medium] [asked: common]
//! Q8. Implement the iterator adapters Map, Filter, Zip, Take, Chain and Peekable as generic structs. [Medium] [asked: very common]
//! Q9. Re-implement Option's `map`, `and_then`, `filter`, `ok_or`, `zip` and `take` on your own enum. [Easy] [asked: very common]
//! Q10. Implement a spinlock `MyMutex<T>` with an RAII guard and the right memory orderings. [Hard] [asked: common]
//! Q11. Implement a `thread::scope`-like helper that lets threads borrow from the stack. [Hard] [asked: occasional]
//! Q12. Implement a `Cow`-like enum that allocates only when it must. [Medium] [asked: common]
//! Q13. Design an error enum with `From`, `Display`, `Error::source`, and use it with `?` (including into `Box<dyn Error>`). [Medium] [asked: very common]
//! Q14. Build a registry of named functions with `Box<dyn Fn>` / `Box<dyn FnMut>` and capturing closures. [Medium] [asked: common]
//! Q15. Implement `Display` for a matrix that respects precision and aligns columns, plus multiplication and indexing. [Medium] [asked: common]
//! Q16. Operator overloading for a Complex number: Add, Sub, Mul, Neg, AddAssign, scalar Mul both ways, and `Sum`. [Easy] [asked: very common]

use std::alloc::{self, Layout};
use std::cell::{Cell, RefCell, UnsafeCell};
use std::collections::hash_map::DefaultHasher;
use std::collections::{HashMap, VecDeque};
use std::error::Error;
use std::fmt;
use std::hash::{Hash, Hasher};
use std::iter::Sum;
use std::marker::PhantomData;
use std::mem::{self, ManuallyDrop, size_of};
use std::num::ParseIntError;
use std::ops::{Add, AddAssign, Deref, DerefMut, Index, IndexMut, Mul, Neg, Range, Sub};
use std::panic::{self, AssertUnwindSafe};
use std::ptr::{self, NonNull};
use std::rc::Rc;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::thread::{self, JoinHandle};

// ---------------------------------------------------------------------------------------------------------------------
// Shared plumbing: seeded xorshift and a drop-counting element type.
// ---------------------------------------------------------------------------------------------------------------------
struct Rng(u64);
impl Rng {
    fn next(&mut self) -> u64 {
        let mut x = self.0;
        x ^= x << 13;
        x ^= x >> 7;
        x ^= x << 17;
        self.0 = x;
        x
    }
    fn range(&mut self, n: usize) -> usize {
        (self.next() % n as u64) as usize
    }
}

/// Element whose Drop appends its id to a shared log: a leak shows up as a missing id, a double drop as a duplicate.
struct D {
    id: u32,
    log: Rc<RefCell<Vec<u32>>>,
}
impl Drop for D {
    fn drop(&mut self) {
        self.log.borrow_mut().push(self.id);
    }
}
fn assert_each_dropped_once(log: &RefCell<Vec<u32>>, created: u32) {
    let mut l = log.borrow().clone();
    l.sort_unstable();
    assert_eq!(
        l,
        (0..created).collect::<Vec<_>>(),
        "every created element must be dropped exactly once"
    );
}

// ---------------------------------------------------------------------------------------------------------------------
// Q1. Implement `MyVec<T>`.
// Layout: `ptr: NonNull<T>` to a heap buffer, `cap` slots allocated, `len` initialized. Slots `len..cap` are UNINITIALIZED memory, which is
// why you cannot use `*ptr.add(i) = v` for them (it would drop the garbage "old value"): use `ptr::write`. Growth doubles (amortized O(1)
// push); `realloc` moves the buffer. insert/remove shift with `ptr::copy` (memmove: ranges overlap). Drop runs `drop_in_place` on the
// initialized slice, then frees the buffer. Deref to `[T]` gives indexing, iteration, sort, len, etc. for free.
// Rust wrinkle: (1) the `len` field IS the safety invariant: decrement before `ptr::read` in pop/remove so a panic cannot cause a double drop.
// (2) `drain` shrinks `len` to the range start FIRST (leak amplification: if the Drain is leaked, the tail leaks but nothing is double-dropped),
// and `Drain::drop` drops unread items and slides the tail back. (3) Zero-sized types need special handling; this version rejects them.
// (4) `PhantomData<T>` tells dropck we own T; `unsafe impl Send/Sync` are manual because `NonNull` is neither.
// Interviewer is testing: alloc/realloc/Layout, uninitialized memory discipline, panic safety of len updates, and element Drop.
// Mistakes: assigning with `*p = v` into uninitialized memory; forgetting to drop elements before dealloc; off-by-one in insert's copy count;
// not handling capacity 0 (dealloc of a dangling pointer); capacity overflow.
// Follow-ups: ZST support (cap = usize::MAX), `IntoIter` that owns the buffer, `shrink_to_fit`, `Vec<MaybeUninit<T>>` / `RawVec` split, Miri.
// ---------------------------------------------------------------------------------------------------------------------
struct MyVec<T> {
    ptr: NonNull<T>,
    cap: usize,
    len: usize,
    _owns: PhantomData<T>,
}
// SAFETY: MyVec<T> uniquely owns its buffer, exactly like Vec<T>; sending or sharing it is as safe as sending or sharing the T values.
unsafe impl<T: Send> Send for MyVec<T> {}
unsafe impl<T: Sync> Sync for MyVec<T> {}

impl<T> MyVec<T> {
    fn new() -> Self {
        assert!(
            size_of::<T>() != 0,
            "zero-sized types are not supported by this teaching version"
        );
        MyVec {
            ptr: NonNull::dangling(),
            cap: 0,
            len: 0,
            _owns: PhantomData,
        }
    }
    fn capacity(&self) -> usize {
        self.cap
    }
    fn grow(&mut self) {
        let new_cap = if self.cap == 0 {
            4
        } else {
            self.cap.checked_mul(2).expect("capacity overflow")
        };
        let new_layout = Layout::array::<T>(new_cap).expect("capacity overflow"); // errors if the size exceeds isize::MAX
        // SAFETY: new_layout has non-zero size (T is not a ZST and new_cap >= 4). For realloc, `self.ptr` was allocated by this global
        // allocator with exactly `Layout::array::<T>(self.cap)` (same alignment), and the new size is non-zero and checked above.
        let p = unsafe {
            if self.cap == 0 {
                alloc::alloc(new_layout)
            } else {
                alloc::realloc(
                    self.ptr.as_ptr() as *mut u8,
                    Layout::array::<T>(self.cap).unwrap(),
                    new_layout.size(),
                )
            }
        };
        self.ptr = match NonNull::new(p as *mut T) {
            Some(p) => p,
            None => alloc::handle_alloc_error(new_layout),
        };
        self.cap = new_cap;
    }
    fn push(&mut self, v: T) {
        if self.len == self.cap {
            self.grow();
        }
        // SAFETY: len < cap after grow, so slot `len` is inside the allocation and uninitialized: `write` does not drop anything.
        unsafe { ptr::write(self.ptr.as_ptr().add(self.len), v) };
        self.len += 1;
    }
    fn pop(&mut self) -> Option<T> {
        if self.len == 0 {
            return None;
        }
        self.len -= 1; // shrink first: the slot is now logically uninitialized, so a panic below cannot double drop
        // SAFETY: slot `len` (the old last element) was initialized and is no longer counted, so we move it out exactly once.
        Some(unsafe { ptr::read(self.ptr.as_ptr().add(self.len)) })
    }
    fn insert(&mut self, i: usize, v: T) {
        assert!(i <= self.len, "insert index out of bounds");
        if self.len == self.cap {
            self.grow();
        }
        // SAFETY: i <= len < cap. We move the `len - i` initialized elements one slot right (ranges overlap, so `copy`, a memmove),
        // which stays inside the allocation, then write into the now-duplicated, logically vacated slot `i` without dropping it.
        unsafe {
            let p = self.ptr.as_ptr().add(i);
            ptr::copy(p, p.add(1), self.len - i);
            ptr::write(p, v);
        }
        self.len += 1;
    }
    fn remove(&mut self, i: usize) -> T {
        assert!(i < self.len, "remove index out of bounds");
        self.len -= 1;
        // SAFETY: i < old len. We move element `i` out, then slide the `len - i` elements after it one slot left; the vacated last slot is
        // beyond the new `len`, so nothing is dropped twice.
        unsafe {
            let p = self.ptr.as_ptr().add(i);
            let v = ptr::read(p);
            ptr::copy(p.add(1), p, self.len - i);
            v
        }
    }
    fn truncate(&mut self, n: usize) {
        if n >= self.len {
            return;
        }
        let tail = self.len - n;
        self.len = n; // set len first: if an element's Drop panics, the rest leaks instead of being dropped twice
        // SAFETY: slots n..n+tail were initialized and are no longer owned by `len`, so dropping them in place happens exactly once.
        unsafe {
            ptr::drop_in_place(ptr::slice_from_raw_parts_mut(
                self.ptr.as_ptr().add(n),
                tail,
            ))
        };
    }
    fn drain(&mut self, range: Range<usize>) -> Drain<'_, T> {
        assert!(
            range.start <= range.end && range.end <= self.len,
            "drain range out of bounds"
        );
        let tail_len = self.len - range.end;
        self.len = range.start; // leak amplification: a leaked Drain leaks the tail but cannot cause UB
        Drain {
            vec: self,
            idx: range.start,
            end: range.end,
            tail_start: range.end,
            tail_len,
        }
    }
}
impl<T> Deref for MyVec<T> {
    type Target = [T];
    fn deref(&self) -> &[T] {
        // SAFETY: ptr is non-null and aligned (dangling-but-aligned when len == 0 is allowed for empty slices), and the first `len`
        // slots are initialized; the shared borrow of self prevents mutation for the slice's lifetime.
        unsafe { std::slice::from_raw_parts(self.ptr.as_ptr(), self.len) }
    }
}
impl<T> DerefMut for MyVec<T> {
    fn deref_mut(&mut self) -> &mut [T] {
        // SAFETY: as in `deref`, and `&mut self` guarantees exclusive access to the buffer.
        unsafe { std::slice::from_raw_parts_mut(self.ptr.as_ptr(), self.len) }
    }
}
impl<T> Drop for MyVec<T> {
    fn drop(&mut self) {
        // SAFETY: the first `len` slots are initialized and owned by us; drop them once. The buffer (if any) was allocated with
        // `Layout::array::<T>(cap)` by the global allocator in `grow`, and cap == 0 means nothing was allocated.
        unsafe {
            ptr::drop_in_place(ptr::slice_from_raw_parts_mut(self.ptr.as_ptr(), self.len));
            if self.cap != 0 {
                alloc::dealloc(
                    self.ptr.as_ptr() as *mut u8,
                    Layout::array::<T>(self.cap).unwrap(),
                );
            }
        }
    }
}
impl<T> Extend<T> for MyVec<T> {
    fn extend<I: IntoIterator<Item = T>>(&mut self, it: I) {
        it.into_iter().for_each(|x| self.push(x));
    }
}
impl<T> FromIterator<T> for MyVec<T> {
    fn from_iter<I: IntoIterator<Item = T>>(it: I) -> Self {
        let mut v = MyVec::new();
        v.extend(it);
        v
    }
}
struct Drain<'a, T> {
    vec: &'a mut MyVec<T>,
    idx: usize,
    end: usize,
    tail_start: usize,
    tail_len: usize,
}
impl<T> Iterator for Drain<'_, T> {
    type Item = T;
    fn next(&mut self) -> Option<T> {
        if self.idx == self.end {
            return None;
        }
        // SAFETY: slots idx..end are initialized, no longer counted in vec.len, and each is read at most once because idx advances.
        let v = unsafe { ptr::read(self.vec.ptr.as_ptr().add(self.idx)) };
        self.idx += 1;
        Some(v)
    }
}
impl<T> Drop for Drain<'_, T> {
    fn drop(&mut self) {
        while self.next().is_some() {} // drop the elements the caller did not consume
        let start = self.vec.len;
        // SAFETY: the tail slots tail_start..tail_start+tail_len are initialized; `start <= tail_start`, so we slide them left inside the allocation
        // (overlap possible: memmove) and then the whole prefix of `start + tail_len` slots is initialized again.
        unsafe {
            let p = self.vec.ptr.as_ptr();
            ptr::copy(p.add(self.tail_start), p.add(start), self.tail_len);
        }
        self.vec.len = start + self.tail_len;
    }
}
fn q1() {
    let mut v: MyVec<i32> = MyVec::new();
    assert!(v.is_empty() && v.pop().is_none() && v.capacity() == 0);
    v.extend([1, 2, 3, 4, 5]);
    assert_eq!(&*v, &[1, 2, 3, 4, 5]);
    assert!(v.capacity() >= 5);
    v.insert(0, 0);
    v.insert(6, 6);
    assert_eq!(v.remove(3), 3);
    assert_eq!(v.iter().sum::<i32>(), 0 + 1 + 2 + 4 + 5 + 6);
    assert_eq!(v.drain(1..3).collect::<Vec<_>>(), vec![1, 2]);
    assert_eq!(&*v, &[0, 4, 5, 6]);
    v.sort_by(|a, b| b.cmp(a)); // slice methods through DerefMut
    assert_eq!(v[0], 6);
    let strings: MyVec<String> = ["a", "b", "c"].iter().map(|s| s.to_string()).collect();
    assert_eq!(strings.join("-"), "a-b-c");

    // 30k random operations against Vec<u32> of ids, with a drop log
    let log = Rc::new(RefCell::new(Vec::new()));
    let mut next_id = 0u32;
    let mut mk = |log: &Rc<RefCell<Vec<u32>>>| {
        next_id += 1;
        D {
            id: next_id - 1,
            log: log.clone(),
        }
    };
    let (mut mine, mut model): (MyVec<D>, Vec<u32>) = (MyVec::new(), Vec::new());
    let mut r = Rng(0x3232_3232_ABAB_0001);
    for step in 0..30_000 {
        match r.range(9) {
            0 | 1 | 2 => {
                let d = mk(&log);
                model.push(d.id);
                mine.push(d);
            }
            3 => assert_eq!(mine.pop().map(|d| d.id), model.pop()),
            4 => {
                let i = r.range(model.len() + 1);
                let d = mk(&log);
                model.insert(i, d.id);
                mine.insert(i, d);
            }
            5 if !model.is_empty() => {
                let i = r.range(model.len());
                assert_eq!(mine.remove(i).id, model.remove(i));
            }
            6 if !model.is_empty() => {
                let a = r.range(model.len() + 1);
                let b = a + r.range(model.len() - a + 1);
                let take = r.range(b - a + 1);
                let got: Vec<u32> = mine.drain(a..b).take(take).map(|d| d.id).collect(); // the rest of the range is dropped by Drain::drop
                let want: Vec<u32> = model.drain(a..b).take(take).collect();
                assert_eq!(got, want);
            }
            7 => {
                let n = r.range(model.len() + 1);
                mine.truncate(n);
                model.truncate(n);
            }
            _ => {
                mine.reverse();
                model.reverse();
            }
        }
        if model.len() > 150 {
            mine.truncate(50);
            model.truncate(50);
        }
        if step % 7 == 0 {
            assert_eq!(mine.iter().map(|d| d.id).collect::<Vec<_>>(), model);
        }
        assert!(mine.capacity() >= mine.len());
    }
    assert_eq!(mine.iter().map(|d| d.id).collect::<Vec<_>>(), model);
    drop(mine);
    assert_each_dropped_once(&log, next_id);
}

// ---------------------------------------------------------------------------------------------------------------------
// Q2. Implement `MyBox<T>`.
// A Box is a uniquely owning pointer to one heap value: allocate with `Layout::new::<T>()`, `ptr::write` the value, `Deref`/`DerefMut` hand out
// references, `Drop` runs the destructor (`drop_in_place`) and frees. Zero-sized T allocates nothing (dangling pointer). Deref is what makes
// `&MyBox<String>` coerce to `&str` and what lets `b.len()` auto-deref.
// Rust wrinkle: the built-in `Box` is magic: `*b` can MOVE the value out (`let x = *b;`) because the compiler knows Box's ownership. For a
// user type, `*mb` always yields a place behind `Deref`, so moving out is an error; provide `into_inner(self)` instead (read, free, `mem::forget`).
// Interviewer is testing: Deref vs DerefMut, deref coercion, Drop ordering, and knowing why Box is special-cased.
// Mistakes: freeing without dropping the value (leaks its resources); dropping then reading in `into_inner` (use-after-drop); forgetting
// `mem::forget(self)` so Drop runs a second time (double free).
// Follow-ups: `Box<dyn Trait>` (needs unsized coercion: `CoerceUnsized`, unstable, so std only), `Box::leak`, `Pin<Box<T>>`.
// ---------------------------------------------------------------------------------------------------------------------
struct MyBox<T> {
    ptr: NonNull<T>,
    _owns: PhantomData<T>,
}
impl<T> MyBox<T> {
    fn new(x: T) -> Self {
        let ptr = if size_of::<T>() == 0 {
            NonNull::dangling()
        } else {
            let layout = Layout::new::<T>();
            // SAFETY: the layout has non-zero size here (ZST handled above).
            let p = unsafe { alloc::alloc(layout) } as *mut T;
            NonNull::new(p).unwrap_or_else(|| alloc::handle_alloc_error(layout))
        };
        // SAFETY: `ptr` is valid for writes of one T (fresh allocation, or dangling for a ZST where writes are no-ops) and holds no value yet.
        unsafe { ptr.as_ptr().write(x) };
        MyBox {
            ptr,
            _owns: PhantomData,
        }
    }
    fn into_inner(self) -> T {
        // SAFETY: the pointee is initialized; we move it out once, free the memory it lived in, and `forget(self)` stops Drop from running again.
        let v = unsafe { self.ptr.as_ptr().read() };
        if size_of::<T>() != 0 {
            unsafe { alloc::dealloc(self.ptr.as_ptr() as *mut u8, Layout::new::<T>()) };
        }
        mem::forget(self);
        v
    }
}
impl<T> Deref for MyBox<T> {
    type Target = T;
    // SAFETY (both derefs): the pointer is valid and aligned for the whole life of the MyBox and points at an initialized T; `&self` / `&mut self`
    // give the matching shared / exclusive access.
    fn deref(&self) -> &T {
        unsafe { self.ptr.as_ref() }
    }
}
impl<T> DerefMut for MyBox<T> {
    fn deref_mut(&mut self) -> &mut T {
        unsafe { self.ptr.as_mut() }
    }
}
impl<T> Drop for MyBox<T> {
    fn drop(&mut self) {
        // SAFETY: the value is initialized and owned; drop it once, then free the allocation made in `new` with the same layout (none for ZSTs).
        unsafe {
            ptr::drop_in_place(self.ptr.as_ptr());
            if size_of::<T>() != 0 {
                alloc::dealloc(self.ptr.as_ptr() as *mut u8, Layout::new::<T>());
            }
        }
    }
}
fn q2() {
    let mut b = MyBox::new(String::from("hello"));
    b.push_str(" world"); // DerefMut + auto-deref method call
    assert_eq!(b.len(), 11);
    fn takes_str(s: &str) -> usize {
        s.len()
    }
    assert_eq!(takes_str(&b), 11); // &MyBox<String> -> &String -> &str (two deref coercions)
    assert_eq!(b.into_inner(), "hello world");
    let z = MyBox::new(()); // zero-sized: no allocation at all
    assert_eq!(*z, ());
    let log = Rc::new(RefCell::new(Vec::new()));
    let a = MyBox::new(D {
        id: 0,
        log: log.clone(),
    });
    let c = MyBox::new(D {
        id: 1,
        log: log.clone(),
    });
    let inner = c.into_inner(); // moved out, not dropped yet
    assert!(log.borrow().is_empty());
    drop(a);
    assert_eq!(*log.borrow(), vec![0]);
    drop(inner);
    assert_each_dropped_once(&log, 2);
    let big = MyBox::new([7u64; 1000]);
    assert_eq!(big.iter().sum::<u64>(), 7000);
}

// ---------------------------------------------------------------------------------------------------------------------
// Q3. Implement a single-threaded `MyRc<T>` and `MyWeak<T>`.
// One heap block `RcBox { strong, weak, value }` shared by all handles. `clone` bumps `strong`; dropping the LAST strong handle drops the value
// (even while Weak handles keep the block alive); the block itself is freed when `weak` reaches 0. Std's trick, copied here: all strong handles
// together hold ONE implicit weak count, so `weak` reaching 0 means "no strong and no explicit weak". `Weak::upgrade` returns Some only while
// strong > 0, and increments it. Counts are `Cell<usize>` because they mutate through `&self`.
// Rust wrinkle: the value is `ManuallyDrop<T>` so freeing the block (a `Box::from_raw`) does not drop it a second time; the value is dropped
// explicitly when strong hits 0. `NonNull<RcBox<T>>` makes the handles `!Send`/`!Sync` automatically (correct: the counts are not atomic).
// Interviewer is testing: the two-counter protocol and its ordering, why cycles leak, Weak for parent pointers, and why Rc is not Send.
// Mistakes: freeing the block at strong == 0 while a Weak still points at it (use-after-free); not holding the implicit weak (double free);
// `upgrade` that resurrects at strong == 0.
// Follow-ups: `Rc::make_mut` (clone-on-write), `Arc` (atomic counts, Release decrement + Acquire fence), `Rc<[T]>`/`Rc<str>` (unsized).
// ---------------------------------------------------------------------------------------------------------------------
struct RcBox<T> {
    strong: Cell<usize>,
    weak: Cell<usize>, // number of Weak handles + 1 while any strong handle exists
    value: ManuallyDrop<T>,
}
struct MyRc<T> {
    ptr: NonNull<RcBox<T>>,
    _owns: PhantomData<RcBox<T>>,
}
struct MyWeak<T> {
    ptr: NonNull<RcBox<T>>,
}
impl<T> MyRc<T> {
    fn new(v: T) -> Self {
        let b = Box::new(RcBox {
            strong: Cell::new(1),
            weak: Cell::new(1),
            value: ManuallyDrop::new(v),
        });
        MyRc {
            ptr: NonNull::from(Box::leak(b)),
            _owns: PhantomData,
        }
    }
    fn inner(&self) -> &RcBox<T> {
        // SAFETY: the block stays allocated while any strong handle exists (strong >= 1 implies weak >= 1), and we hold a strong handle.
        unsafe { self.ptr.as_ref() }
    }
    fn strong_count(this: &Self) -> usize {
        this.inner().strong.get()
    }
    fn weak_count(this: &Self) -> usize {
        this.inner().weak.get() - 1
    }
    fn ptr_eq(a: &Self, b: &Self) -> bool {
        a.ptr == b.ptr
    }
    fn downgrade(this: &Self) -> MyWeak<T> {
        this.inner().weak.set(this.inner().weak.get() + 1);
        MyWeak { ptr: this.ptr }
    }
    fn get_mut(this: &mut Self) -> Option<&mut T> {
        if this.inner().strong.get() == 1 && this.inner().weak.get() == 1 {
            // SAFETY: we are the only strong handle and no Weak exists, so nobody else can reach the value: exclusive access is real.
            Some(unsafe { &mut *(*this.ptr.as_ptr()).value })
        } else {
            None
        }
    }
}
impl<T> Clone for MyRc<T> {
    fn clone(&self) -> Self {
        self.inner().strong.set(self.inner().strong.get() + 1);
        MyRc {
            ptr: self.ptr,
            _owns: PhantomData,
        }
    }
}
impl<T> Deref for MyRc<T> {
    type Target = T;
    fn deref(&self) -> &T {
        &self.inner().value
    }
}
impl<T> Drop for MyRc<T> {
    fn drop(&mut self) {
        let s = self.inner().strong.get() - 1;
        self.inner().strong.set(s);
        if s == 0 {
            // SAFETY: strong just reached 0, so no other strong handle exists and the value is initialized: drop it exactly once. Weak handles
            // never touch `value` (upgrade checks strong first), so the drop cannot race with a reader.
            unsafe { ManuallyDrop::drop(&mut (*self.ptr.as_ptr()).value) };
            let w = self.inner().weak.get() - 1; // release the implicit weak shared by all strong handles
            self.inner().weak.set(w);
            if w == 0 {
                // SAFETY: no strong and no weak handles remain; the block came from `Box::leak`, and ManuallyDrop prevents a second value drop.
                unsafe { drop(Box::from_raw(self.ptr.as_ptr())) };
            }
        }
    }
}
impl<T> MyWeak<T> {
    fn upgrade(&self) -> Option<MyRc<T>> {
        // SAFETY: the block is alive while any Weak exists (weak count >= 1 for us), so reading the counters is valid.
        let b = unsafe { self.ptr.as_ref() };
        if b.strong.get() == 0 {
            return None;
        }
        b.strong.set(b.strong.get() + 1);
        Some(MyRc {
            ptr: self.ptr,
            _owns: PhantomData,
        })
    }
}
impl<T> Clone for MyWeak<T> {
    fn clone(&self) -> Self {
        // SAFETY: as in `upgrade`.
        let b = unsafe { self.ptr.as_ref() };
        b.weak.set(b.weak.get() + 1);
        MyWeak { ptr: self.ptr }
    }
}
impl<T> Drop for MyWeak<T> {
    fn drop(&mut self) {
        // SAFETY: our own weak count keeps the block allocated until we decrement it below.
        let b = unsafe { self.ptr.as_ref() };
        let w = b.weak.get() - 1;
        b.weak.set(w);
        if w == 0 {
            // SAFETY: last handle of any kind: free the block (the value, if it was ever alive, was already dropped when strong hit 0).
            unsafe { drop(Box::from_raw(self.ptr.as_ptr())) };
        }
    }
}
fn q3() {
    let log = Rc::new(RefCell::new(Vec::new()));
    let a = MyRc::new(D {
        id: 0,
        log: log.clone(),
    });
    let b = a.clone();
    let w = MyRc::downgrade(&a);
    assert_eq!((MyRc::strong_count(&a), MyRc::weak_count(&a)), (2, 1));
    assert!(MyRc::ptr_eq(&a, &b) && a.id == 0);
    drop(a);
    assert!(w.upgrade().is_some() && log.borrow().is_empty());
    drop(b); // last strong handle: value dropped NOW even though `w` is alive
    assert_eq!(*log.borrow(), vec![0]);
    assert!(w.upgrade().is_none());
    drop(w); // block freed (checked by Miri/ASan; here by not crashing)
    let mut solo = MyRc::new(5);
    *MyRc::get_mut(&mut solo).unwrap() += 1;
    let other = solo.clone();
    assert!(MyRc::get_mut(&mut solo).is_none());
    assert_eq!(*other, 6);
    // parent <-> child with a Weak back pointer: no cycle, everything is dropped
    struct Node {
        d: D,
        parent: RefCell<Option<MyWeak<Node>>>,
        kids: RefCell<Vec<MyRc<Node>>>,
    }
    let log2 = Rc::new(RefCell::new(Vec::new()));
    let mk = |id| {
        MyRc::new(Node {
            d: D {
                id,
                log: log2.clone(),
            },
            parent: RefCell::new(None),
            kids: RefCell::new(vec![]),
        })
    };
    let root = mk(0);
    for id in 1..=3 {
        let kid = mk(id);
        *kid.parent.borrow_mut() = Some(MyRc::downgrade(&root));
        root.kids.borrow_mut().push(kid);
    }
    let kid = root.kids.borrow()[1].clone();
    assert_eq!(
        kid.parent
            .borrow()
            .as_ref()
            .unwrap()
            .upgrade()
            .unwrap()
            .d
            .id,
        0
    );
    drop(root);
    assert_eq!(
        log2.borrow().len(),
        3,
        "root and two other kids are gone; `kid` is still held"
    );
    assert!(
        kid.parent.borrow().as_ref().unwrap().upgrade().is_none(),
        "the parent is dead"
    );
    drop(kid);
    assert_each_dropped_once(&log2, 4);
    // randomized: many handles cloned and dropped in random order, value dropped exactly once
    let mut r = Rng(0x0303_C0DE_0303_0303);
    for _ in 0..2000 {
        let log = Rc::new(RefCell::new(Vec::new()));
        let first = MyRc::new(D {
            id: 0,
            log: log.clone(),
        });
        let (mut strongs, mut weaks) = (vec![first], Vec::new());
        for _ in 0..20 {
            match r.range(4) {
                0 => {
                    let c = strongs[r.range(strongs.len().max(1)) % strongs.len().max(1)..]
                        .first()
                        .cloned();
                    if let Some(c) = c {
                        strongs.push(c.clone());
                    } else if let Some(w) = weaks.last().and_then(|w: &MyWeak<D>| w.upgrade()) {
                        strongs.push(w);
                    }
                }
                1 => {
                    if let Some(s) = strongs.first() {
                        weaks.push(MyRc::downgrade(s));
                    } else if let Some(w) = weaks.first() {
                        weaks.push(w.clone());
                    }
                }
                2 => {
                    if !strongs.is_empty() {
                        let i = r.range(strongs.len());
                        strongs.swap_remove(i);
                    }
                }
                _ => {
                    if !weaks.is_empty() {
                        let i = r.range(weaks.len());
                        weaks.swap_remove(i);
                    }
                }
            }
            assert_eq!(log.borrow().len(), if strongs.is_empty() { 1 } else { 0 });
            if let Some(s) = strongs.first() {
                assert_eq!(MyRc::strong_count(s), strongs.len());
                assert_eq!(MyRc::weak_count(s), weaks.len());
            }
        }
        drop(strongs);
        drop(weaks);
        assert_eq!(*log.borrow(), vec![0]);
    }
}

// ---------------------------------------------------------------------------------------------------------------------
// Q4. A generic Stack, and a Queue built from two stacks.
// Stack = a `Vec` with push/pop/peek. Queue from two stacks: `push` goes onto `inbox`; `pop` takes from `outbox`, and only when `outbox` is empty does
// it pour the whole inbox over (reversing the order). Each element is moved at most once, so any sequence of n operations costs O(n):
// AMORTIZED O(1) per operation, though a single pop can cost O(n).
// Rust wrinkle: pouring is `while let Some(x) = self.inbox.pop() { self.outbox.push(x) }`: ownership moves element by element, no clones, and `T` needs
// no bounds. `peek` takes `&mut self` because it may pour (or restructure with `as_slice().last()` tricks); say why in the interview.
// Interviewer is testing: the amortized argument (each element moves at most twice), not pouring when outbox is non-empty (breaks FIFO order).
// Mistakes: pouring on every pop (O(n) each); pouring while outbox still has items; claiming worst-case O(1).
// Follow-ups: stack from two queues; min-stack in O(1); a ring-buffer queue (`VecDeque`); concurrent queues (channels).
// ---------------------------------------------------------------------------------------------------------------------
#[derive(Default)]
struct Stack<T> {
    v: Vec<T>,
}
impl<T> Stack<T> {
    fn new() -> Self {
        Stack { v: Vec::new() }
    }
    fn push(&mut self, x: T) {
        self.v.push(x);
    }
    fn pop(&mut self) -> Option<T> {
        self.v.pop()
    }
    fn peek(&self) -> Option<&T> {
        self.v.last()
    }
    fn len(&self) -> usize {
        self.v.len()
    }
}
#[derive(Default)]
struct Queue<T> {
    inbox: Stack<T>,
    outbox: Stack<T>,
    moves: usize, // how many element transfers happened (for the amortized check)
}
impl<T> Queue<T> {
    fn new() -> Self {
        Queue {
            inbox: Stack::new(),
            outbox: Stack::new(),
            moves: 0,
        }
    }
    fn push(&mut self, x: T) {
        self.inbox.push(x);
    }
    fn refill(&mut self) {
        if self.outbox.len() == 0 {
            while let Some(x) = self.inbox.pop() {
                self.outbox.push(x);
                self.moves += 1;
            }
        }
    }
    fn pop(&mut self) -> Option<T> {
        self.refill();
        self.outbox.pop()
    }
    fn peek(&mut self) -> Option<&T> {
        self.refill();
        self.outbox.peek()
    }
    fn len(&self) -> usize {
        self.inbox.len() + self.outbox.len()
    }
}
fn q4() {
    let mut q = Queue::new();
    (1..=3).for_each(|x| q.push(x));
    assert_eq!(q.pop(), Some(1));
    q.push(4);
    assert_eq!((q.peek().copied(), q.len()), (Some(2), 3));
    let mut r = Rng(0x0404_4040_0404_4040);
    let (mut q, mut model, mut pushes) = (Queue::new(), VecDeque::new(), 0usize);
    for i in 0..30_000u32 {
        if r.range(5) < 3 {
            q.push(i);
            model.push_back(i);
            pushes += 1;
        } else {
            assert_eq!(q.pop(), model.pop_front());
        }
        if i % 11 == 0 {
            assert_eq!(q.peek().copied(), model.front().copied());
        }
        assert_eq!(q.len(), model.len());
    }
    assert!(
        q.moves <= pushes,
        "each element moves between the stacks at most once: amortized O(1)"
    );
    let mut s = Stack::new();
    s.push('a');
    assert_eq!(s.peek(), Some(&'a'));
    assert_eq!((s.pop(), s.pop()), (Some('a'), None));
}

// ---------------------------------------------------------------------------------------------------------------------
// Q5. A singly linked stack with `iter`, `iter_mut`, `into_iter`, and a Drop that survives a million nodes.
// `Option<Box<Node<T>>>` head; push = new node pointing at the old head; pop = `take()` the head and promote its `next`. Three iterators:
// `Iter` holds `Option<&Node>` and advances with `as_deref`; `IterMut` holds `Option<&mut Node>` and advances with `take()` so the unique borrow
// is MOVED (not reborrowed twice); `IntoIter` wraps the list and pops.
// Rust wrinkle: the default recursive Drop of a `Box` chain recurses once per node and overflows the stack around 100k nodes: write an iterative
// Drop that unlinks nodes one at a time. And `IterMut::next` must `take()` its cursor: `self.next.as_deref_mut()` alone would not compile because the
// returned `&mut T` would overlap the cursor's own borrow.
// Interviewer is testing: Option/Box ownership dance, lifetimes on iterator structs, the Drop overflow trap. (See linked_lists_in_rust.rs for more.)
// Mistakes: forgetting `take()` in pop (fights the borrow checker); recursive Drop; `peek` returning a clone.
// Follow-ups: a persistent `Rc` list with structural sharing; doubly linked list (Rc/Weak, or unsafe NonNull); queue with a tail pointer.
// ---------------------------------------------------------------------------------------------------------------------
type Link<T> = Option<Box<LNode<T>>>;
struct LNode<T> {
    elem: T,
    next: Link<T>,
}
struct List<T> {
    head: Link<T>,
    len: usize,
}
impl<T> List<T> {
    fn new() -> Self {
        List { head: None, len: 0 }
    }
    fn push(&mut self, elem: T) {
        let next = self.head.take();
        self.head = Some(Box::new(LNode { elem, next }));
        self.len += 1;
    }
    fn pop(&mut self) -> Option<T> {
        self.head.take().map(|n| {
            self.head = n.next;
            self.len -= 1;
            n.elem
        })
    }
    fn peek(&self) -> Option<&T> {
        self.head.as_ref().map(|n| &n.elem)
    }
    fn peek_mut(&mut self) -> Option<&mut T> {
        self.head.as_mut().map(|n| &mut n.elem)
    }
    fn iter(&self) -> Iter<'_, T> {
        Iter {
            next: self.head.as_deref(),
        }
    }
    fn iter_mut(&mut self) -> IterMut<'_, T> {
        IterMut {
            next: self.head.as_deref_mut(),
        }
    }
}
impl<T> Drop for List<T> {
    fn drop(&mut self) {
        let mut cur = self.head.take();
        while let Some(mut n) = cur {
            cur = n.next.take(); // detach before the node drops: its Drop then sees `next == None` and does not recurse
        }
    }
}
struct Iter<'a, T> {
    next: Option<&'a LNode<T>>,
}
impl<'a, T> Iterator for Iter<'a, T> {
    type Item = &'a T;
    fn next(&mut self) -> Option<&'a T> {
        self.next.map(|n| {
            self.next = n.next.as_deref();
            &n.elem
        })
    }
}
struct IterMut<'a, T> {
    next: Option<&'a mut LNode<T>>,
}
impl<'a, T> Iterator for IterMut<'a, T> {
    type Item = &'a mut T;
    fn next(&mut self) -> Option<&'a mut T> {
        self.next.take().map(|n| {
            self.next = n.next.as_deref_mut();
            &mut n.elem
        })
    }
}
struct IntoIter<T>(List<T>);
impl<T> Iterator for IntoIter<T> {
    type Item = T;
    fn next(&mut self) -> Option<T> {
        self.0.pop()
    }
}
impl<T> IntoIterator for List<T> {
    type Item = T;
    type IntoIter = IntoIter<T>;
    fn into_iter(self) -> IntoIter<T> {
        IntoIter(self)
    }
}
impl<'a, T> IntoIterator for &'a List<T> {
    type Item = &'a T;
    type IntoIter = Iter<'a, T>;
    fn into_iter(self) -> Iter<'a, T> {
        self.iter()
    }
}
fn q5() {
    let mut l = List::new();
    assert!(l.pop().is_none() && l.peek().is_none());
    let mut r = Rng(0x0505_5050_0505_5050);
    let mut model: Vec<i64> = Vec::new();
    for _ in 0..30_000 {
        match r.range(4) {
            0 | 1 => {
                let x = r.range(1000) as i64;
                l.push(x);
                model.push(x);
            }
            2 => assert_eq!(l.pop(), model.pop()),
            _ => {
                if let Some(t) = l.peek_mut() {
                    *t += 1;
                    *model.last_mut().unwrap() += 1;
                }
            }
        }
        assert_eq!((l.len, l.peek()), (model.len(), model.last()));
    }
    l.iter_mut().for_each(|x| *x *= 2);
    model.iter_mut().for_each(|x| *x *= 2);
    assert_eq!(
        l.iter().copied().collect::<Vec<_>>(),
        model.iter().rev().copied().collect::<Vec<_>>()
    );
    assert_eq!((&l).into_iter().count(), model.len());
    assert_eq!(
        l.into_iter().collect::<Vec<_>>(),
        model.into_iter().rev().collect::<Vec<_>>()
    );
    let mut big = List::new();
    for i in 0..1_000_000u32 {
        big.push(i);
    }
    drop(big); // would overflow the stack with the derived recursive Drop
}

// ---------------------------------------------------------------------------------------------------------------------
// Q6. Implement `MyHashMap<K, V>` with separate chaining and resizing.
// `buckets: Vec<Vec<(K, V)>>`; the bucket is `hash(key) % buckets.len()`; operations scan that (short) chain. Keep the load factor len/buckets at
// most 3/4: when an insert would exceed it, DOUBLE the bucket count and rehash by MOVING every entry into its new bucket (no clones). Average
// O(1) per op, amortized for the resize; worst case O(n) if every key collides (hash flooding, which is why std seeds SipHash randomly).
// Rust wrinkle: lookups take `&Q` where `K: std::borrow::Borrow<Q>` and `Q: Hash + Eq + ?Sized`, so a `MyHashMap<String, V>` is queried with a plain `&str`
// (this is exactly std's signature; `Borrow` promises K and Q hash and compare identically). `DefaultHasher::new()` is deterministic, keeping this
// recipe reproducible. `Vec::swap_remove` deletes from a chain in O(1) without shifting.
// Interviewer is testing: resize policy, `Borrow` for ergonomic lookups, ownership during rehash, what `insert` returns (the replaced value).
// Mistakes: resizing without rehashing; using `hash % cap` with the OLD cap after growth; leaking entries by `clear()` then reinserting clones;
// requiring `K: Clone`; returning `&V` that outlives a resize (the borrow checker prevents it, which is the point).
// Follow-ups: open addressing / Robin Hood / swiss tables; `entry` API; iteration order; shrinking; a custom `BuildHasher`.
// ---------------------------------------------------------------------------------------------------------------------
struct MyHashMap<K, V> {
    buckets: Vec<Vec<(K, V)>>,
    len: usize,
}
fn hash_of<Q: Hash + ?Sized>(q: &Q) -> u64 {
    let mut h = DefaultHasher::new();
    q.hash(&mut h);
    h.finish()
}
impl<K: Hash + Eq, V> MyHashMap<K, V> {
    fn new() -> Self {
        MyHashMap {
            buckets: (0..4).map(|_| Vec::new()).collect(),
            len: 0,
        }
    }
    fn len(&self) -> usize {
        self.len
    }
    fn bucket_of<Q: Hash + ?Sized>(&self, q: &Q) -> usize {
        (hash_of(q) % self.buckets.len() as u64) as usize
    }
    fn resize(&mut self) {
        let new_n = self.buckets.len() * 2;
        let old = mem::replace(&mut self.buckets, (0..new_n).map(|_| Vec::new()).collect());
        for (k, v) in old.into_iter().flatten() {
            let b = (hash_of(&k) % new_n as u64) as usize;
            self.buckets[b].push((k, v)); // moved, never cloned
        }
    }
    fn insert(&mut self, k: K, v: V) -> Option<V> {
        let b = self.bucket_of(&k);
        if let Some(slot) = self.buckets[b].iter_mut().find(|(ek, _)| *ek == k) {
            return Some(mem::replace(&mut slot.1, v));
        }
        if (self.len + 1) * 4 > self.buckets.len() * 3 {
            self.resize();
        }
        let b = self.bucket_of(&k); // recompute: the bucket count may have changed
        self.buckets[b].push((k, v));
        self.len += 1;
        None
    }
    fn get<Q>(&self, q: &Q) -> Option<&V>
    where
        K: std::borrow::Borrow<Q>,
        Q: Hash + Eq + ?Sized,
    {
        self.buckets[self.bucket_of(q)]
            .iter()
            .find(|(k, _)| std::borrow::Borrow::<Q>::borrow(k) == q)
            .map(|(_, v)| v)
    }
    fn get_mut<Q>(&mut self, q: &Q) -> Option<&mut V>
    where
        K: std::borrow::Borrow<Q>,
        Q: Hash + Eq + ?Sized,
    {
        let b = self.bucket_of(q);
        self.buckets[b]
            .iter_mut()
            .find(|(k, _)| std::borrow::Borrow::<Q>::borrow(&*k) == q)
            .map(|(_, v)| v)
    }
    fn contains_key<Q>(&self, q: &Q) -> bool
    where
        K: std::borrow::Borrow<Q>,
        Q: Hash + Eq + ?Sized,
    {
        self.get(q).is_some()
    }
    fn remove<Q>(&mut self, q: &Q) -> Option<V>
    where
        K: std::borrow::Borrow<Q>,
        Q: Hash + Eq + ?Sized,
    {
        let b = self.bucket_of(q);
        let i = self.buckets[b]
            .iter()
            .position(|(k, _)| std::borrow::Borrow::<Q>::borrow(k) == q)?;
        self.len -= 1;
        Some(self.buckets[b].swap_remove(i).1)
    }
    fn iter(&self) -> impl Iterator<Item = (&K, &V)> {
        self.buckets.iter().flatten().map(|(k, v)| (k, v))
    }
    fn longest_chain(&self) -> usize {
        self.buckets.iter().map(|b| b.len()).max().unwrap_or(0)
    }
}
fn q6() {
    let mut m: MyHashMap<String, i32> = MyHashMap::new();
    assert_eq!(m.insert("one".into(), 1), None);
    assert_eq!(m.insert("one".into(), 11), Some(1));
    assert_eq!(m.get("one"), Some(&11)); // &str lookup in a String-keyed map via Borrow
    *m.get_mut("one").unwrap() += 1;
    assert_eq!(
        (m.remove("one"), m.remove("one"), m.len()),
        (Some(12), None, 0)
    );

    // 30k random ops vs std HashMap, values are drop-counted
    let log = Rc::new(RefCell::new(Vec::new()));
    let mut next_id = 0u32;
    let (mut mine, mut oracle): (MyHashMap<u32, D>, HashMap<u32, u32>) =
        (MyHashMap::new(), HashMap::new());
    let mut r = Rng(0x0606_6060_0606_6060);
    for _ in 0..30_000 {
        let k = r.range(400) as u32;
        match r.range(5) {
            0 | 1 => {
                let d = D {
                    id: next_id,
                    log: log.clone(),
                };
                next_id += 1;
                let id = d.id;
                assert_eq!(mine.insert(k, d).map(|d| d.id), oracle.insert(k, id));
            }
            2 => assert_eq!(mine.remove(&k).map(|d| d.id), oracle.remove(&k)),
            3 => assert_eq!(mine.get(&k).map(|d| d.id), oracle.get(&k).copied()),
            _ => assert_eq!(mine.contains_key(&k), oracle.contains_key(&k)),
        }
        assert_eq!(mine.len(), oracle.len());
        assert!(
            mine.len() * 4 <= mine.buckets.len() * 3,
            "load factor stays <= 3/4"
        );
    }
    let mut a: Vec<(u32, u32)> = mine.iter().map(|(k, d)| (*k, d.id)).collect();
    let mut b: Vec<(u32, u32)> = oracle.iter().map(|(k, v)| (*k, *v)).collect();
    (a.sort(), b.sort());
    assert_eq!(a, b);
    println!(
        "    Q6: {} entries in {} buckets, longest chain {}",
        mine.len(),
        mine.buckets.len(),
        mine.longest_chain()
    );
    drop(mine);
    assert_each_dropped_once(&log, next_id);
    // string keys looked up by &str
    let mut sm: MyHashMap<String, usize> = MyHashMap::new();
    let mut so: HashMap<String, usize> = HashMap::new();
    for i in 0..5000usize {
        let key = format!("k{}", r.range(300));
        assert_eq!(sm.insert(key.clone(), i), so.insert(key, i));
        let probe = format!("k{}", r.range(300));
        assert_eq!(sm.get(probe.as_str()), so.get(probe.as_str()));
    }
}

// ---------------------------------------------------------------------------------------------------------------------
// Q7. Implement `MyString`.
// A `Vec<u8>` plus ONE invariant: the bytes are always valid UTF-8. Every mutator must preserve it: `push(char)` appends `char::encode_utf8`;
// `push_str` appends a `&str`'s bytes (valid by type); `pop` removes a whole last char (`len_utf8`); `truncate` asserts a char boundary. With the
// invariant, `as_str` can skip validation via `from_utf8_unchecked`, and `Deref<Target = str>` gives every `str` method. Constructing from raw
// bytes (`from_utf8`) VALIDATES, which is the only way untrusted bytes enter.
// Rust wrinkle: `unsafe` is justified by an invariant the type enforces privately (private field!). Making `buf` public would let any caller break it.
// `len()` is bytes, not chars; slicing at a non-boundary panics.
// Interviewer is testing: invariant-based unsafe, UTF-8 encoding facts (1-4 bytes), `is_char_boundary`, why `s[i]` indexing is not provided.
// Mistakes: popping one byte; `push` of `c as u8`; exposing `&mut Vec<u8>`; unchecked construction from user bytes.
// Follow-ups: `insert`/`remove` at a char boundary; small-string optimization; `Cow<str>`; `String::from_utf8_lossy`.
// ---------------------------------------------------------------------------------------------------------------------
#[derive(Default, Clone)]
struct MyString {
    buf: Vec<u8>, // invariant: always valid UTF-8 (the field is private to this module's API)
}
impl MyString {
    fn new() -> Self {
        MyString { buf: Vec::new() }
    }
    fn from_utf8(bytes: Vec<u8>) -> Result<Self, std::str::Utf8Error> {
        std::str::from_utf8(&bytes)?; // validate untrusted bytes
        Ok(MyString { buf: bytes })
    }
    fn as_str(&self) -> &str {
        // SAFETY: `buf` is only ever extended with whole `&str`/`char` encodings, shortened at char boundaries, or built from validated bytes,
        // so it is always valid UTF-8.
        unsafe { std::str::from_utf8_unchecked(&self.buf) }
    }
    fn push_str(&mut self, s: &str) {
        self.buf.extend_from_slice(s.as_bytes());
    }
    fn push(&mut self, c: char) {
        let mut tmp = [0u8; 4];
        self.buf
            .extend_from_slice(c.encode_utf8(&mut tmp).as_bytes());
    }
    fn pop(&mut self) -> Option<char> {
        let c = self.as_str().chars().next_back()?;
        self.buf.truncate(self.buf.len() - c.len_utf8());
        Some(c)
    }
    fn truncate(&mut self, n: usize) {
        if n <= self.buf.len() {
            assert!(
                self.as_str().is_char_boundary(n),
                "truncate must land on a char boundary"
            );
            self.buf.truncate(n);
        }
    }
}
impl Deref for MyString {
    type Target = str;
    fn deref(&self) -> &str {
        self.as_str()
    }
}
impl fmt::Display for MyString {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.pad(self.as_str())
    }
}
impl From<&str> for MyString {
    fn from(s: &str) -> Self {
        MyString {
            buf: s.as_bytes().to_vec(),
        }
    }
}
fn q7() {
    let mut s = MyString::from("héllo");
    assert_eq!((s.len(), s.chars().count()), (6, 5));
    s.push('🦀');
    s.push_str("日本");
    assert_eq!(s.to_uppercase(), "HÉLLO🦀日本"); // any str method through Deref
    assert_eq!(
        (s.pop(), s.pop(), s.pop()),
        (Some('本'), Some('日'), Some('🦀'))
    );
    assert_eq!(format!("[{:>8}]", s), "[   héllo]");
    assert!(MyString::from_utf8(vec![0xff, 0xfe]).is_err());
    assert_eq!(
        MyString::from_utf8("ok".as_bytes().to_vec())
            .unwrap()
            .as_str(),
        "ok"
    );
    let alpha = ['a', 'é', 'ß', '日', '🦀', ' '];
    let mut r = Rng(0x0707_7070_0707_7070);
    for _ in 0..2000 {
        let (mut mine, mut std_s) = (MyString::new(), String::new());
        for _ in 0..30 {
            match r.range(4) {
                0 => {
                    let c = alpha[r.range(alpha.len())];
                    mine.push(c);
                    std_s.push(c);
                }
                1 => {
                    let t: String = (0..r.range(4))
                        .map(|_| alpha[r.range(alpha.len())])
                        .collect();
                    mine.push_str(&t);
                    std_s.push_str(&t);
                }
                2 => assert_eq!(mine.pop(), std_s.pop()),
                _ => {
                    let boundaries: Vec<usize> = (0..=std_s.len())
                        .filter(|&i| std_s.is_char_boundary(i))
                        .collect();
                    let n = boundaries[r.range(boundaries.len())];
                    mine.truncate(n);
                    std_s.truncate(n);
                }
            }
            assert_eq!(mine.as_str(), std_s);
            assert!(std::str::from_utf8(&mine.buf).is_ok());
        }
    }
}

// ---------------------------------------------------------------------------------------------------------------------
// Q8. Implement the iterator adapters Map, Filter, Zip, Take, Chain and Peekable.
// Each adapter is a struct that WRAPS the source iterator (and maybe a closure or counter) and implements `Iterator` by calling the inner `next`.
// Nothing runs until something pulls: that is laziness, and because the adapter types are concrete generics (`MyMap<MyFilter<Iter, P>, F>`) the
// compiler monomorphizes and inlines the whole pipeline: zero-cost, no heap, no dynamic dispatch. An extension trait with a blanket impl
// (`impl<I: Iterator> MyIter for I {}`) makes the methods callable as `v.iter().my_map(..)`, exactly how std puts `map` on every Iterator.
// Rust wrinkle: bounds: `F: FnMut(I::Item) -> B`, the output type `B` becomes `Item`; Filter's predicate gets `&I::Item`; Peekable stores
// `Option<Option<Item>>` (outer: "have I peeked?", inner: the peeked value, possibly end-of-iteration) and `peek` uses `get_or_insert_with`.
// Chain keeps the first iterator in an `Option` and drops it when exhausted (fused behaviour).
// Interviewer is testing: generic struct + trait bound syntax, laziness, `Item` associated types, `FnMut` vs `Fn`, the extension-trait pattern.
// Mistakes: collecting inside `next` (breaks laziness); a `Filter` that calls the predicate by value; Peekable losing the peeked item on `next`.
// Follow-ups: `size_hint`, `DoubleEndedIterator`, `ExactSizeIterator`, `impl Iterator` returns, `Iterator::by_ref`, `fold`-based overrides.
// ---------------------------------------------------------------------------------------------------------------------
struct MyMap<I, F> {
    iter: I,
    f: F,
}
impl<B, I: Iterator, F: FnMut(I::Item) -> B> Iterator for MyMap<I, F> {
    type Item = B;
    fn next(&mut self) -> Option<B> {
        self.iter.next().map(&mut self.f)
    }
}
struct MyFilter<I, P> {
    iter: I,
    pred: P,
}
impl<I: Iterator, P: FnMut(&I::Item) -> bool> Iterator for MyFilter<I, P> {
    type Item = I::Item;
    fn next(&mut self) -> Option<I::Item> {
        while let Some(x) = self.iter.next() {
            if (self.pred)(&x) {
                return Some(x);
            }
        }
        None
    }
}
struct MyZip<A, B> {
    a: A,
    b: B,
}
impl<A: Iterator, B: Iterator> Iterator for MyZip<A, B> {
    type Item = (A::Item, B::Item);
    fn next(&mut self) -> Option<Self::Item> {
        let x = self.a.next()?; // stop as soon as either side ends
        let y = self.b.next()?;
        Some((x, y))
    }
}
struct MyTake<I> {
    iter: I,
    n: usize,
}
impl<I: Iterator> Iterator for MyTake<I> {
    type Item = I::Item;
    fn next(&mut self) -> Option<I::Item> {
        if self.n == 0 {
            return None; // do NOT pull from the source: Take must not consume an extra element
        }
        self.n -= 1;
        self.iter.next()
    }
}
struct MyChain<A, B> {
    a: Option<A>,
    b: B,
}
impl<A: Iterator, B: Iterator<Item = A::Item>> Iterator for MyChain<A, B> {
    type Item = A::Item;
    fn next(&mut self) -> Option<A::Item> {
        if let Some(a) = &mut self.a {
            match a.next() {
                Some(x) => return Some(x),
                None => self.a = None,
            }
        }
        self.b.next()
    }
}
struct MyPeekable<I: Iterator> {
    iter: I,
    peeked: Option<Option<I::Item>>,
}
impl<I: Iterator> MyPeekable<I> {
    fn peek(&mut self) -> Option<&I::Item> {
        let it = &mut self.iter;
        self.peeked.get_or_insert_with(|| it.next()).as_ref()
    }
}
impl<I: Iterator> Iterator for MyPeekable<I> {
    type Item = I::Item;
    fn next(&mut self) -> Option<I::Item> {
        match self.peeked.take() {
            Some(v) => v,
            None => self.iter.next(),
        }
    }
}
trait MyIter: Iterator + Sized {
    fn my_map<B, F: FnMut(Self::Item) -> B>(self, f: F) -> MyMap<Self, F> {
        MyMap { iter: self, f }
    }
    fn my_filter<P: FnMut(&Self::Item) -> bool>(self, pred: P) -> MyFilter<Self, P> {
        MyFilter { iter: self, pred }
    }
    fn my_zip<U: IntoIterator>(self, other: U) -> MyZip<Self, U::IntoIter> {
        MyZip {
            a: self,
            b: other.into_iter(),
        }
    }
    fn my_take(self, n: usize) -> MyTake<Self> {
        MyTake { iter: self, n }
    }
    fn my_chain<U: IntoIterator<Item = Self::Item>>(self, other: U) -> MyChain<Self, U::IntoIter> {
        MyChain {
            a: Some(self),
            b: other.into_iter(),
        }
    }
    fn my_peekable(self) -> MyPeekable<Self> {
        MyPeekable {
            iter: self,
            peeked: None,
        }
    }
}
impl<I: Iterator> MyIter for I {}
fn q8() {
    // laziness: nothing runs until pulled
    let calls = Cell::new(0);
    let it = (1..10).my_map(|x| {
        calls.set(calls.get() + 1);
        x * 2
    });
    assert_eq!(calls.get(), 0);
    assert_eq!(it.my_take(3).collect::<Vec<_>>(), vec![2, 4, 6]);
    assert_eq!(calls.get(), 3, "Take must not pull a fourth element");
    // run-length grouping, the classic Peekable use
    let mut p = "aaabccdd".chars().my_peekable();
    let mut groups = Vec::new();
    while let Some(c) = p.next() {
        let mut n = 1;
        while p.peek() == Some(&c) {
            p.next();
            n += 1;
        }
        groups.push((c, n));
    }
    assert_eq!(groups, vec![('a', 3), ('b', 1), ('c', 2), ('d', 2)]);
    let mut r = Rng(0x0808_8080_0808_8080);
    for _ in 0..2000 {
        let a: Vec<i32> = (0..r.range(12)).map(|_| r.range(50) as i32 - 25).collect();
        let b: Vec<i32> = (0..r.range(12)).map(|_| r.range(50) as i32 - 25).collect();
        let n = r.range(15);
        assert_eq!(
            a.iter().my_map(|x| x * 3).collect::<Vec<_>>(),
            a.iter().map(|x| x * 3).collect::<Vec<_>>()
        );
        assert_eq!(
            a.iter().my_filter(|x| **x % 2 == 0).collect::<Vec<_>>(),
            a.iter().filter(|x| **x % 2 == 0).collect::<Vec<_>>()
        );
        assert_eq!(
            a.iter().my_zip(&b).collect::<Vec<_>>(),
            a.iter().zip(&b).collect::<Vec<_>>()
        );
        assert_eq!(
            a.iter().my_take(n).collect::<Vec<_>>(),
            a.iter().take(n).collect::<Vec<_>>()
        );
        assert_eq!(
            a.iter().my_chain(&b).collect::<Vec<_>>(),
            a.iter().chain(&b).collect::<Vec<_>>()
        );
        let composed: Vec<i32> = a
            .iter()
            .copied()
            .my_chain(b.iter().copied())
            .my_filter(|x| x % 3 != 0)
            .my_map(|x| x + 1)
            .my_take(n)
            .collect();
        let want: Vec<i32> = a
            .iter()
            .copied()
            .chain(b.iter().copied())
            .filter(|x| x % 3 != 0)
            .map(|x| x + 1)
            .take(n)
            .collect();
        assert_eq!(composed, want);
        // peekable: interleave peek and next at random, compare with std
        let (mut mine, mut std_p) = (a.iter().my_peekable(), a.iter().peekable());
        for _ in 0..a.len() + 3 {
            if r.range(2) == 0 {
                assert_eq!(mine.peek(), std_p.peek());
            } else {
                assert_eq!(mine.next(), std_p.next());
            }
        }
    }
}

// ---------------------------------------------------------------------------------------------------------------------
// Q9. Re-implement Option's combinators on your own enum.
// `Maybe<T>` is `Just(T) | Nothing`. `map` transforms the inside; `and_then` (flat_map/bind) chains a step that can itself fail and flattens the nesting;
// `filter` keeps a value only if a predicate holds; `ok_or` converts to `Result`; `zip` pairs two values if both exist; `take` moves the value out leaving
// Nothing (via `mem::replace`). These are what the `?` operator and every `Option` pipeline desugar to.
// Rust wrinkle: methods that consume `self` move the payload; `as_ref()` (Maybe<&T>) lets you map without consuming; `take(&mut self)` needs `mem::replace`
// because you cannot move out of `&mut self`. `unwrap_or` evaluates its argument eagerly: use `unwrap_or_else` for expensive defaults.
// Interviewer is testing: match ergonomics, closure bounds (`FnOnce`), the monadic bind idea, eager vs lazy defaults.
// Mistakes: `and_then` with a closure returning `T` instead of `Maybe<U>` (that is `map`); `take` implemented with `clone`; unwrap in library code.
// Follow-ups: implement `Result::map_err`/`and_then`; `transpose`; `?` on your own type via the unstable `Try` trait (not stable); `Option::as_deref`.
// ---------------------------------------------------------------------------------------------------------------------
#[derive(Debug, PartialEq, Clone, Copy)]
enum Maybe<T> {
    Just(T),
    Nothing,
}
impl<T> Maybe<T> {
    fn map<U, F: FnOnce(T) -> U>(self, f: F) -> Maybe<U> {
        match self {
            Maybe::Just(x) => Maybe::Just(f(x)),
            Maybe::Nothing => Maybe::Nothing,
        }
    }
    fn and_then<U, F: FnOnce(T) -> Maybe<U>>(self, f: F) -> Maybe<U> {
        match self {
            Maybe::Just(x) => f(x),
            Maybe::Nothing => Maybe::Nothing,
        }
    }
    fn filter<P: FnOnce(&T) -> bool>(self, p: P) -> Maybe<T> {
        match self {
            Maybe::Just(x) if p(&x) => Maybe::Just(x),
            _ => Maybe::Nothing,
        }
    }
    fn unwrap_or(self, d: T) -> T {
        self.unwrap_or_else(|| d)
    }
    fn unwrap_or_else<F: FnOnce() -> T>(self, f: F) -> T {
        match self {
            Maybe::Just(x) => x,
            Maybe::Nothing => f(),
        }
    }
    fn ok_or<E>(self, e: E) -> Result<T, E> {
        match self {
            Maybe::Just(x) => Ok(x),
            Maybe::Nothing => Err(e),
        }
    }
    fn zip<U>(self, other: Maybe<U>) -> Maybe<(T, U)> {
        match (self, other) {
            (Maybe::Just(a), Maybe::Just(b)) => Maybe::Just((a, b)),
            _ => Maybe::Nothing,
        }
    }
    fn take(&mut self) -> Maybe<T> {
        mem::replace(self, Maybe::Nothing)
    }
    fn as_ref(&self) -> Maybe<&T> {
        match self {
            Maybe::Just(x) => Maybe::Just(x),
            Maybe::Nothing => Maybe::Nothing,
        }
    }
}
impl<T> From<Option<T>> for Maybe<T> {
    fn from(o: Option<T>) -> Self {
        o.map_or(Maybe::Nothing, Maybe::Just)
    }
}
impl<T> From<Maybe<T>> for Option<T> {
    fn from(m: Maybe<T>) -> Self {
        match m {
            Maybe::Just(x) => Some(x),
            Maybe::Nothing => None,
        }
    }
}
fn q9() {
    let mut r = Rng(0x0909_9090_0909_9090);
    let half = |x: i32| if x % 2 == 0 { Some(x / 2) } else { None };
    for _ in 0..2000 {
        let o: Option<i32> = if r.range(4) == 0 {
            None
        } else {
            Some(r.range(40) as i32 - 20)
        };
        let p: Option<i32> = if r.range(4) == 0 {
            None
        } else {
            Some(r.range(40) as i32 - 20)
        };
        let (m, n): (Maybe<i32>, Maybe<i32>) = (o.into(), p.into());
        assert_eq!(Option::from(m.map(|x| x * 2)), o.map(|x| x * 2));
        assert_eq!(
            Option::from(m.and_then(|x| half(x).into())),
            o.and_then(half)
        );
        assert_eq!(Option::from(m.filter(|x| *x > 0)), o.filter(|x| *x > 0));
        assert_eq!(m.unwrap_or(99), o.unwrap_or(99));
        assert_eq!(m.ok_or("none"), o.ok_or("none"));
        assert_eq!(Option::from(m.zip(n)), o.zip(p));
        assert_eq!(
            Option::from(m.as_ref().map(|x| *x + 1)),
            o.as_ref().map(|x| *x + 1)
        );
        let (mut mm, mut oo) = (m, o);
        assert_eq!((Option::from(mm.take()), mm), (oo.take(), Maybe::Nothing));
    }
    // lazy vs eager default
    let hits = Cell::new(0);
    assert_eq!(
        Maybe::Just(1).unwrap_or_else(|| {
            hits.set(hits.get() + 1);
            2
        }),
        1
    );
    assert_eq!(hits.get(), 0);
}

// ---------------------------------------------------------------------------------------------------------------------
// Q10. Implement a spinlock `MyMutex<T>` with an RAII guard.
// State: `AtomicBool locked` plus `UnsafeCell<T>` (the ONLY legal way to mutate through a shared reference). `lock` loops on
// `compare_exchange_weak(false, true, Acquire, Relaxed)`; success means we own the lock. The guard derefs to the data and its Drop does
// `store(false, Release)`. Orderings: ACQUIRE on lock + RELEASE on unlock make everything written by the previous holder visible to the next:
// that pair is the whole happens-before edge. The failure ordering can be Relaxed. Spin on a plain `load` (test-and-test-and-set) to avoid
// hammering the cache line with writes, and `spin_loop()` hints the CPU.
// Rust wrinkle: `unsafe impl<T: Send> Sync for MyMutex<T>`: sharing the lock across threads hands `&mut T` to one thread at a time, so T needs only
// `Send`, not `Sync`. The guard is `!Send` (a PhantomData of a raw pointer) so it cannot be unlocked from another thread. No poisoning here.
// Interviewer is testing: the Acquire/Release pairing, why `UnsafeCell`, the Sync bound reasoning, and why spinlocks are for SHORT critical sections
// (use `std::sync::Mutex` or `parking_lot` otherwise: they park the thread instead of burning CPU).
// Mistakes: `Relaxed` on both ends (data race on the protected value); `SeqCst` everywhere without understanding; guard that implements Send.
// Follow-ups: ticket lock for fairness; futex-based mutex (std's implementation); `try_lock`; poisoning on panic; loom testing.
// ---------------------------------------------------------------------------------------------------------------------
struct MyMutex<T> {
    locked: AtomicBool,
    data: UnsafeCell<T>,
}
// SAFETY: the lock serializes all access to `data`: at most one guard (hence one &mut T) exists at a time, so sharing &MyMutex<T> across threads is
// like moving the T between threads one at a time, which only requires T: Send.
unsafe impl<T: Send> Sync for MyMutex<T> {}
unsafe impl<T: Send> Send for MyMutex<T> {}
struct MyGuard<'a, T> {
    lock: &'a MyMutex<T>,
    _not_send: PhantomData<*mut ()>,
}
impl<T> MyMutex<T> {
    fn new(v: T) -> Self {
        MyMutex {
            locked: AtomicBool::new(false),
            data: UnsafeCell::new(v),
        }
    }
    fn lock(&self) -> MyGuard<'_, T> {
        while self
            .locked
            .compare_exchange_weak(false, true, Ordering::Acquire, Ordering::Relaxed)
            .is_err()
        {
            while self.locked.load(Ordering::Relaxed) {
                std::hint::spin_loop();
            }
        }
        MyGuard {
            lock: self,
            _not_send: PhantomData,
        }
    }
    fn try_lock(&self) -> Option<MyGuard<'_, T>> {
        self.locked
            .compare_exchange(false, true, Ordering::Acquire, Ordering::Relaxed)
            .ok()
            .map(|_| MyGuard {
                lock: self,
                _not_send: PhantomData,
            })
    }
    fn into_inner(self) -> T {
        self.data.into_inner()
    }
}
impl<T> Deref for MyGuard<'_, T> {
    type Target = T;
    // SAFETY (deref and deref_mut): a live guard means this thread holds the lock, so no other reference to the data exists; `&mut self` makes
    // the exclusive borrow of the guard match the exclusive borrow of the data.
    fn deref(&self) -> &T {
        unsafe { &*self.lock.data.get() }
    }
}
impl<T> DerefMut for MyGuard<'_, T> {
    fn deref_mut(&mut self) -> &mut T {
        unsafe { &mut *self.lock.data.get() }
    }
}
impl<T> Drop for MyGuard<'_, T> {
    fn drop(&mut self) {
        self.lock.locked.store(false, Ordering::Release);
    }
}
fn q10() {
    let m = MyMutex::new(0u64);
    {
        let mut g = m.lock();
        *g += 5;
        assert!(m.try_lock().is_none(), "locked: try_lock must fail");
    }
    assert_eq!(*m.try_lock().unwrap(), 5);
    // 8 threads x 5000 increments plus pushes into a shared Vec: no lost updates
    let shared = MyMutex::new((0u64, Vec::<usize>::new()));
    thread::scope(|s| {
        for t in 0..8 {
            let shared = &shared;
            s.spawn(move || {
                for i in 0..5000 {
                    let mut g = shared.lock();
                    g.0 += 1;
                    if i % 1000 == 0 {
                        g.1.push(t);
                    }
                }
            });
        }
    });
    let (count, pushes) = shared.into_inner();
    assert_eq!(count, 40_000);
    assert_eq!(pushes.len(), 8 * 5);
    let mut per_thread = [0; 8];
    pushes.iter().for_each(|&t| per_thread[t] += 1);
    assert_eq!(per_thread, [5; 8]);
}

// ---------------------------------------------------------------------------------------------------------------------
// Q11. Implement a `thread::scope`-like helper.
// Plain `thread::spawn` demands `'static` closures because the new thread may outlive the caller. A SCOPE fixes that by guaranteeing every thread it
// spawned is JOINED before the scope function returns, so borrowing stack data is sound. Implementation: `my_scope(|s| { s.spawn(...) })` runs the
// user closure, then joins all handles (also when the closure panics, via `catch_unwind`), and re-panics if any thread panicked. `spawn` has to erase
// the `'env` lifetime to hand the closure to `thread::spawn`: a `transmute` to `'static`. That transmute is the ONE unsafe step, sound only because of
// the join guarantee.
// Rust wrinkle: `'env` is a lifetime PARAMETER of `my_scope`, so it outlives the call; borrowed locals created inside the user closure are shorter and
// cannot be captured, which blocks use-after-free. The `PhantomData<&'env mut &'env ()>` makes `'env` invariant (std does the same); MyScope holds a
// `RefCell` so it is `!Sync`: a spawned closure cannot capture `&MyScope` and spawn grandchildren (std supports that with atomics: out of scope here).
// Interviewer is testing: why `spawn` needs `'static` normally, the join-before-return argument, panic propagation, and where unsafe is unavoidable.
// Mistakes: not joining on the panic path (use-after-free of stack data); forgetting to propagate a child panic; making the handle list shared across threads.
// Follow-ups: return values from scoped threads (a `ScopedJoinHandle<T>`), nested spawns, rayon's `scope`, crossbeam's scope (the original).
// ---------------------------------------------------------------------------------------------------------------------
struct MyScope<'env> {
    handles: RefCell<Vec<JoinHandle<()>>>,
    _env: PhantomData<&'env mut &'env ()>,
}
impl<'env> MyScope<'env> {
    fn spawn<F: FnOnce() + Send + 'env>(&self, f: F) {
        let boxed: Box<dyn FnOnce() + Send + 'env> = Box::new(f);
        // SAFETY: we only erase the lifetime. `my_scope` joins every handle recorded here before it returns (on both the normal and the panic path),
        // so the closure, and everything it borrows for 'env, cannot be used after those borrows end.
        let boxed: Box<dyn FnOnce() + Send + 'static> = unsafe { mem::transmute(boxed) };
        self.handles.borrow_mut().push(thread::spawn(boxed));
    }
}
fn my_scope<'env, F, R>(f: F) -> R
where
    F: FnOnce(&MyScope<'env>) -> R,
{
    let scope = MyScope {
        handles: RefCell::new(Vec::new()),
        _env: PhantomData,
    };
    let result = panic::catch_unwind(AssertUnwindSafe(|| f(&scope)));
    let mut child_panicked = false;
    for h in scope.handles.take() {
        child_panicked |= h.join().is_err(); // join EVERY thread before anything is allowed to return or unwind
    }
    match result {
        Err(p) => panic::resume_unwind(p),
        Ok(_) if child_panicked => panic!("a scoped thread panicked"),
        Ok(r) => r,
    }
}
fn q11() {
    let mut data: Vec<u64> = (0..1000).collect();
    let weights = vec![3u64; 10];
    let total = AtomicUsize::new(0);
    my_scope(|s| {
        for (i, chunk) in data.chunks_mut(100).enumerate() {
            let (weights, total) = (&weights, &total); // borrowed from the stack: impossible with thread::spawn
            s.spawn(move || {
                for x in chunk.iter_mut() {
                    *x = *x * *x + weights[i];
                }
                total.fetch_add(chunk.len(), Ordering::Relaxed);
            });
        }
    });
    assert_eq!(total.load(Ordering::Relaxed), 1000);
    assert!(
        data.iter()
            .enumerate()
            .all(|(i, &x)| x == (i * i) as u64 + 3)
    );
    // a panicking child: the scope still joins the other thread, then panics
    let finished = AtomicBool::new(false);
    let old_hook = panic::take_hook();
    panic::set_hook(Box::new(|_| {})); // silence the expected panic message
    let outcome = panic::catch_unwind(AssertUnwindSafe(|| {
        my_scope(|s| {
            s.spawn(|| panic!("boom"));
            s.spawn(|| {
                thread::sleep(std::time::Duration::from_millis(20));
                finished.store(true, Ordering::SeqCst);
            });
        })
    }));
    panic::set_hook(old_hook);
    assert!(outcome.is_err(), "child panic must propagate");
    assert!(
        finished.load(Ordering::SeqCst),
        "the sibling thread was joined before the panic left the scope"
    );
    // returns the closure's value
    assert_eq!(my_scope(|_| 42), 42);
}

// ---------------------------------------------------------------------------------------------------------------------
// Q12. Implement a `Cow`-like enum that allocates only when it must.
// `MyCow<'a, B>` is either `Borrowed(&'a B)` or `Owned(B::Owned)` (`str`/`String`, `[T]`/`Vec<T>`, `Path`/`PathBuf`). Reads go through `Deref`
// (borrowed: the reference; owned: `Borrow::borrow`). The first write calls `to_mut()`, which converts Borrowed into Owned ONCE (clone-on-write) and
// returns `&mut B::Owned`. `into_owned` yields the owned value, cloning only if still borrowed. Typical use: a function that usually returns its input
// unchanged (no allocation) and allocates only when it actually modifies it.
// Rust wrinkle: the `ToOwned` trait links the unsized borrowed type to its owned type (`<str as ToOwned>::Owned = String`), and `B: ?Sized` is needed to
// allow `str`/`[T]`. `if let Borrowed(b) = *self` copies the `&B` out (references are Copy) so the following assignment `*self = Owned(..)` is allowed.
// Interviewer is testing: ToOwned vs Clone, `?Sized`, lazy cloning, and API design (functions returning `Cow<str>`).
// Mistakes: always allocating (defeats the point); `Borrowed` returned with a lifetime tied to a temporary; assuming `Cow` is only for strings.
// Follow-ups: `Cow<'static, str>` for config keys; `Cow<[T]>`; serde's `#[serde(borrow)]`; comparing Cow with &str.
// ---------------------------------------------------------------------------------------------------------------------
enum MyCow<'a, B: ?Sized + ToOwned + 'a> {
    Borrowed(&'a B),
    Owned(<B as ToOwned>::Owned),
}
impl<B: ?Sized + ToOwned> MyCow<'_, B> {
    fn is_borrowed(&self) -> bool {
        matches!(self, MyCow::Borrowed(_))
    }
    fn to_mut(&mut self) -> &mut <B as ToOwned>::Owned {
        if let MyCow::Borrowed(b) = *self {
            *self = MyCow::Owned(b.to_owned()); // the one and only allocation
        }
        match self {
            MyCow::Owned(o) => o,
            MyCow::Borrowed(_) => unreachable!("converted above"),
        }
    }
    fn into_owned(self) -> <B as ToOwned>::Owned {
        match self {
            MyCow::Borrowed(b) => b.to_owned(),
            MyCow::Owned(o) => o,
        }
    }
}
impl<B: ?Sized + ToOwned> Deref for MyCow<'_, B>
where
    B::Owned: std::borrow::Borrow<B>,
{
    type Target = B;
    fn deref(&self) -> &B {
        match self {
            MyCow::Borrowed(b) => b,
            MyCow::Owned(o) => std::borrow::Borrow::<B>::borrow(o),
        }
    }
}
/// Collapse runs of spaces; returns the input untouched (no allocation) when there is nothing to collapse.
fn collapse_spaces(s: &str) -> MyCow<'_, str> {
    if !s.contains("  ") {
        return MyCow::Borrowed(s);
    }
    let mut out = MyCow::Borrowed(s);
    let o = out.to_mut();
    while o.contains("  ") {
        *o = o.replace("  ", " ");
    }
    out
}
fn q12() {
    let c = collapse_spaces("no change");
    assert!(c.is_borrowed() && &*c == "no change");
    let c = collapse_spaces("a   b    c");
    assert!(!c.is_borrowed());
    assert_eq!(c.into_owned(), "a b c");
    let mut v: MyCow<[i32]> = MyCow::Borrowed(&[1, 2, 3]);
    assert_eq!(v.len(), 3); // Deref to [i32]
    v.to_mut().push(4);
    assert_eq!(&*v, &[1, 2, 3, 4]);
    let mut r = Rng(0x1212_AAAA_0000_1212);
    for _ in 0..2000 {
        let s: String = (0..r.range(15))
            .map(|_| [' ', ' ', 'a', 'b'][r.range(4)])
            .collect();
        let mut want = s.clone();
        while want.contains("  ") {
            want = want.replace("  ", " ");
        }
        let got = collapse_spaces(&s);
        assert_eq!(got.is_borrowed(), want == s, "{s:?}");
        if got.is_borrowed() {
            assert_eq!(
                got.as_ptr(),
                s.as_ptr(),
                "borrowed means the very same bytes"
            );
        }
        assert_eq!(&*got, want.as_str());
    }
}

// ---------------------------------------------------------------------------------------------------------------------
// Q13. Design an error enum with `From`, `Display`, `Error::source`, and use `?`.
// One enum per module/crate lists the failure modes. `impl From<ParseIntError> for ConfigError` is what lets `?` convert automatically (`?` calls
// `From::from` on the error). `Display` is the human message; `std::error::Error` makes the type interoperable (`Box<dyn Error>`, anyhow) and its
// `source()` exposes the underlying cause so reporters can print the chain. Library crates: a typed enum (callers can match; thiserror generates
// the boilerplate). Applications: `Box<dyn Error>`/anyhow (just propagate and report).
// Rust wrinkle: `?` in a function returning `Result<T, Box<dyn Error>>` converts ANY `E: Error + 'static` via `From<E> for Box<dyn Error>`. `From` cannot
// add context (the key name): use `map_err` with a variant that carries it. Keep `source` returning `Some(&self.0)` for wrapped causes, `None` for leaves.
// Interviewer is testing: `From` + `?` desugaring, `Display` vs `Debug`, `source` chains, enum vs trait object trade-offs.
// Mistakes: stringly-typed errors (`Result<T, String>`) that callers cannot match on; `unwrap` in library code; forgetting `Error::source`; implementing
// `Display` by calling `{:?}`.
// Follow-ups: `#[non_exhaustive]`, `thiserror`/`anyhow`, `Error::downcast_ref`, error codes for FFI, backtraces.
// ---------------------------------------------------------------------------------------------------------------------
#[derive(Debug)]
enum ConfigError {
    MissingEquals,
    BadKey(String),
    Int(ParseIntError),
    OutOfRange { key: String, value: i64 },
}
impl fmt::Display for ConfigError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ConfigError::MissingEquals => write!(f, "line has no '='"),
            ConfigError::BadKey(k) => write!(f, "invalid key {k:?}"),
            ConfigError::Int(e) => write!(f, "bad integer: {e}"),
            ConfigError::OutOfRange { key, value } => {
                write!(f, "{key} = {value} is outside 0..=1000")
            }
        }
    }
}
impl Error for ConfigError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            ConfigError::Int(e) => Some(e), // the underlying cause
            _ => None,
        }
    }
}
impl From<ParseIntError> for ConfigError {
    fn from(e: ParseIntError) -> Self {
        ConfigError::Int(e)
    }
}
fn parse_line(line: &str) -> Result<(String, i64), ConfigError> {
    let (k, v) = line.split_once('=').ok_or(ConfigError::MissingEquals)?;
    let key = k.trim();
    if key.is_empty() || !key.chars().all(|c| c.is_ascii_alphanumeric() || c == '_') {
        return Err(ConfigError::BadKey(key.to_string()));
    }
    let value: i64 = v.trim().parse()?; // ParseIntError -> ConfigError via From
    if !(0..=1000).contains(&value) {
        return Err(ConfigError::OutOfRange {
            key: key.to_string(),
            value,
        });
    }
    Ok((key.to_string(), value))
}
fn sum_config(text: &str) -> Result<i64, Box<dyn Error>> {
    let mut total = 0;
    for line in text.lines().filter(|l| !l.trim().is_empty()) {
        total += parse_line(line)?.1; // ConfigError -> Box<dyn Error>
    }
    if total == 0 {
        return Err("config sums to zero".into()); // &str -> Box<dyn Error>
    }
    Ok(total)
}
fn q13() {
    assert_eq!(parse_line(" port = 80 ").unwrap(), ("port".to_string(), 80));
    assert!(matches!(
        parse_line("port 80"),
        Err(ConfigError::MissingEquals)
    ));
    assert!(matches!(parse_line("po rt=1"), Err(ConfigError::BadKey(_))));
    let e = parse_line("port=eighty").unwrap_err();
    assert!(e.to_string().starts_with("bad integer:") && e.source().is_some());
    assert_eq!(
        parse_line("n=5000").unwrap_err().to_string(),
        "n = 5000 is outside 0..=1000"
    );
    assert_eq!(sum_config("a=1\n\nb=2").unwrap(), 3);
    let err = sum_config("a=1\nb=x").unwrap_err();
    assert!(err.downcast_ref::<ConfigError>().is_some() && err.source().is_some());
    assert_eq!(
        sum_config("a=0").unwrap_err().to_string(),
        "config sums to zero"
    );
    // generated lines with a known expected category
    let mut r = Rng(0x1313_BEEF_0000_1313);
    for _ in 0..2000 {
        let (key, val, kind): (String, String, u8) = match r.range(5) {
            0 => ("k1".into(), r.range(1001).to_string(), 0),
            1 => ("k1".into(), (1001 + r.range(50)).to_string(), 1),
            2 => ("k 1".into(), "5".into(), 2),
            3 => ("k1".into(), "5x".into(), 3),
            _ => ("k1".into(), format!("-{}", 1 + r.range(9)), 1),
        };
        let res = parse_line(&format!("{key}={val}"));
        match (kind, res) {
            (0, Ok((k, v))) => assert_eq!((k, v.to_string()), ("k1".to_string(), val)),
            (1, Err(ConfigError::OutOfRange { .. }))
            | (2, Err(ConfigError::BadKey(_)))
            | (3, Err(ConfigError::Int(_))) => {}
            (k, other) => panic!("kind {k}: unexpected {other:?}"),
        }
    }
}

// ---------------------------------------------------------------------------------------------------------------------
// Q14. A registry of named functions with `Box<dyn Fn>` / `Box<dyn FnMut>`.
// Closures each have a unique anonymous type, so a collection of different closures needs type erasure: `Box<dyn Fn(&[f64]) -> f64>`. The vtable gives
// one indirect call per use; the Box owns the captured environment. `Fn` can be called through `&self` (any number of times, shared), `FnMut` needs `&mut`
// (it mutates captured state), `FnOnce` consumes. Pick the most permissive trait for the CALLER of your API: accept `FnOnce` if you call once, `FnMut` if
// repeatedly, `Fn` only if you need shared calls (threads, reentrancy).
// Rust wrinkle: `register` takes a generic `F: Fn(..) + 'static` and boxes it (`Box::new(f)` unsizes to `dyn Fn`); `'static` because the box lives in the
// map (closures that borrow locals need a lifetime parameter on the registry, or `move`/clone their captures). `move` closures own their environment.
// Interviewer is testing: Fn/FnMut/FnOnce hierarchy, `dyn` vs generics (static vs dynamic dispatch, code size), captured state, the `'static` bound.
// Mistakes: `Vec<Fn(..)>` without a Box (unsized); `Box<dyn Fn>` where the closure mutates state (needs FnMut); forgetting `move` and borrowing a local.
// Follow-ups: callbacks with `Rc<RefCell<..>>` state, `Arc<dyn Fn + Send + Sync>` for threads, function pointers `fn(..)` (no captures, Copy).
// ---------------------------------------------------------------------------------------------------------------------
struct Registry {
    pure: HashMap<String, Box<dyn Fn(&[f64]) -> f64>>,
    stateful: HashMap<String, Box<dyn FnMut() -> u32>>,
}
impl Registry {
    fn new() -> Self {
        Registry {
            pure: HashMap::new(),
            stateful: HashMap::new(),
        }
    }
    fn register<F: Fn(&[f64]) -> f64 + 'static>(&mut self, name: &str, f: F) {
        self.pure.insert(name.to_string(), Box::new(f));
    }
    fn register_counter<F: FnMut() -> u32 + 'static>(&mut self, name: &str, f: F) {
        self.stateful.insert(name.to_string(), Box::new(f));
    }
    fn call(&self, name: &str, args: &[f64]) -> Option<f64> {
        self.pure.get(name).map(|f| f(args))
    }
    fn tick(&mut self, name: &str) -> Option<u32> {
        self.stateful.get_mut(name).map(|f| f())
    }
}
fn q14() {
    let mut reg = Registry::new();
    reg.register("sum", |a| a.iter().sum());
    reg.register("max", |a| a.iter().copied().fold(f64::MIN, f64::max));
    let factor = 2.5;
    reg.register("scaled_sum", move |a| factor * a.iter().sum::<f64>()); // captures `factor` by move
    let offset = Rc::new(Cell::new(10.0));
    let o2 = offset.clone();
    // Rc is not Send but the registry is single-threaded, so this is fine; a Send registry would need Arc<Atomic..>
    reg.register("plus_offset", move |a| a.len() as f64 + o2.get());
    let mut n = 0;
    reg.register_counter("next_id", move || {
        n += 1; // FnMut: mutates its own captured state
        n
    });
    assert_eq!(reg.call("sum", &[1.0, 2.0, 3.0]), Some(6.0));
    assert_eq!(reg.call("max", &[1.0, 9.0, 3.0]), Some(9.0));
    assert_eq!(reg.call("scaled_sum", &[2.0, 2.0]), Some(10.0));
    assert_eq!(reg.call("plus_offset", &[0.0; 4]), Some(14.0));
    offset.set(100.0); // shared captured state is visible through the closure
    assert_eq!(reg.call("plus_offset", &[]), Some(100.0));
    assert_eq!(reg.call("nope", &[]), None);
    assert_eq!(
        (reg.tick("next_id"), reg.tick("next_id"), reg.tick("zzz")),
        (Some(1), Some(2), None)
    );
    let mut r = Rng(0x1414_0000_BBBB_1414);
    for _ in 0..2000 {
        let xs: Vec<f64> = (0..1 + r.range(8)).map(|_| r.range(100) as f64).collect();
        assert_eq!(reg.call("sum", &xs), Some(xs.iter().sum::<f64>()));
        assert_eq!(
            reg.call("max", &xs),
            Some(xs.iter().cloned().fold(f64::MIN, f64::max))
        );
        assert_eq!(
            reg.call("scaled_sum", &xs),
            Some(2.5 * xs.iter().sum::<f64>())
        );
    }
}

// ---------------------------------------------------------------------------------------------------------------------
// Q15. `Display` for a matrix, plus multiplication and indexing.
// `fmt::Display::fmt(&self, f)` writes into the `Formatter`; `f.precision()` and `f.width()` carry the caller's `{:.2}` / `{:8}` request, so a good impl
// honours them (format each cell with the requested precision, right-align per column). Build rows from per-column widths computed first. `Index<(usize,
// usize)>` makes `m[(i, j)]` work; multiplication is the i-k-j triple loop on a flat row-major `Vec<f64>` (cache-friendly: the inner loop walks contiguous
// memory in both the result and the right operand).
// Rust wrinkle: `write!(f, ...)` returns `fmt::Result` and `?` propagates it; use `f.pad` for string-like cells so width/fill flags work; do not
// `println!` inside Display. `impl Mul for &Matrix` avoids moving operands. Const generics (`Matrix<const R: usize, const C: usize>`) can enforce
// dimension compatibility at compile time; the runtime version panics on mismatch.
// Interviewer is testing: Formatter API knowledge, trait impls for ergonomics (Index, Mul, Display), loop order and complexity O(n^3), panics vs `Result`.
// Mistakes: trailing newline from Display; ignoring precision; recomputing widths per row (ragged columns); `&Matrix * &Matrix` impl on the owned type only.
// Follow-ups: transpose and blocked multiplication, Strassen, const-generic dimensions, `IndexMut`, `Debug` vs `Display`, `Matrix::from_rows` with `TryFrom`.
// ---------------------------------------------------------------------------------------------------------------------
#[derive(Clone, PartialEq, Debug)]
struct Matrix {
    rows: usize,
    cols: usize,
    data: Vec<f64>,
}
impl Matrix {
    fn new(rows: usize, cols: usize, data: Vec<f64>) -> Self {
        assert_eq!(
            data.len(),
            rows * cols,
            "data length must equal rows * cols"
        );
        Matrix { rows, cols, data }
    }
    fn identity(n: usize) -> Self {
        let mut m = Matrix::new(n, n, vec![0.0; n * n]);
        (0..n).for_each(|i| m[(i, i)] = 1.0);
        m
    }
}
impl Index<(usize, usize)> for Matrix {
    type Output = f64;
    fn index(&self, (i, j): (usize, usize)) -> &f64 {
        assert!(i < self.rows && j < self.cols, "matrix index out of bounds");
        &self.data[i * self.cols + j]
    }
}
impl IndexMut<(usize, usize)> for Matrix {
    fn index_mut(&mut self, (i, j): (usize, usize)) -> &mut f64 {
        assert!(i < self.rows && j < self.cols, "matrix index out of bounds");
        &mut self.data[i * self.cols + j]
    }
}
impl Mul for &Matrix {
    type Output = Matrix;
    fn mul(self, rhs: &Matrix) -> Matrix {
        assert_eq!(self.cols, rhs.rows, "dimension mismatch");
        let mut out = Matrix::new(self.rows, rhs.cols, vec![0.0; self.rows * rhs.cols]);
        for i in 0..self.rows {
            for k in 0..self.cols {
                let a = self.data[i * self.cols + k];
                for j in 0..rhs.cols {
                    out.data[i * rhs.cols + j] += a * rhs.data[k * rhs.cols + j];
                }
            }
        }
        out
    }
}
impl fmt::Display for Matrix {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let prec = f.precision().unwrap_or(1);
        let cells: Vec<String> = self.data.iter().map(|v| format!("{v:.prec$}")).collect();
        let widths: Vec<usize> = (0..self.cols)
            .map(|j| {
                (0..self.rows)
                    .map(|i| cells[i * self.cols + j].len())
                    .max()
                    .unwrap_or(0)
            })
            .collect();
        for i in 0..self.rows {
            if i > 0 {
                writeln!(f)?;
            }
            write!(f, "|")?;
            for j in 0..self.cols {
                write!(f, " {:>w$}", cells[i * self.cols + j], w = widths[j])?;
            }
            write!(f, " |")?;
        }
        Ok(())
    }
}
fn q15() {
    let m = Matrix::new(2, 3, vec![1.0, -2.5, 3.0, 10.0, 0.5, -7.25]);
    assert_eq!(m.to_string(), "|  1.0 -2.5  3.0 |\n| 10.0  0.5 -7.2 |"); // default precision 1 ("-7.25" rounds to even: -7.2)
    assert_eq!(
        format!("{m:.2}"),
        "|  1.00 -2.50  3.00 |\n| 10.00  0.50 -7.25 |"
    );
    assert_eq!(m[(1, 2)], -7.25);
    let sq = Matrix::new(2, 2, vec![1.0, 2.0, 3.0, 4.0]);
    assert_eq!((&sq * &sq).data, vec![7.0, 10.0, 15.0, 22.0]);
    let mut r = Rng(0x1515_FACE_0000_1515);
    for _ in 0..2000 {
        let (n, k, p) = (1 + r.range(4), 1 + r.range(4), 1 + r.range(4));
        let mk = |rows: usize, cols: usize, r: &mut Rng| {
            Matrix::new(
                rows,
                cols,
                (0..rows * cols).map(|_| r.range(7) as f64 - 3.0).collect(),
            )
        };
        let (a, b, c) = (mk(n, k, &mut r), mk(k, p, &mut r), mk(p, 2, &mut r));
        let prod = &a * &b;
        for i in 0..n {
            for j in 0..p {
                let want: f64 = (0..k).map(|t| a[(i, t)] * b[(t, j)]).sum(); // oracle: the textbook definition (small integers: exact in f64)
                assert_eq!(prod[(i, j)], want);
            }
        }
        assert_eq!(&(&a * &b) * &c, &a * &(&b * &c), "associativity");
        assert_eq!(&a * &Matrix::identity(k), a);
        let s = a.to_string();
        assert_eq!(s.lines().count(), n);
        assert!(!s.ends_with('\n'));
        assert!(
            s.lines()
                .all(|l| l.len() == s.lines().next().unwrap().len()),
            "columns are aligned"
        );
    }
}

// ---------------------------------------------------------------------------------------------------------------------
// Q16. Operator overloading for a Complex number, with `Sum`.
// Each operator is a trait in `std::ops` (`Add::add`, `Mul::mul`, `Neg::neg`, `AddAssign::add_assign`). Implement them on the value type for `Copy` data
// (cheap), optionally also on references (`&a + &b`) for big types. Scalars on the left need an impl for the SCALAR type (`impl Mul<Complex> for i64`),
// which the orphan rule allows because `Complex` is local. `Sum` lets `iter.sum::<Complex>()` work: implement for owned and for `&Complex` items.
// Rust wrinkle: `Output` is an associated type so `Mul<i64> for Complex` and `Mul<Complex> for Complex` can differ; `impl<'a> Sum<&'a Complex>` needs the
// lifetime; `Sum` starts from the additive identity (`Complex::ZERO`) so an empty iterator sums to zero, not a panic.
// Interviewer is testing: the operator traits and associated types, orphan-rule awareness, `Copy` derive, `Sum`/`Product`, `Display` for output.
// Mistakes: implementing `Mul` with the wrong formula (re*re - im*im, re*im + im*re); forgetting `AddAssign` when `+=` is used; moving non-Copy operands out.
// Follow-ups: `Div` (conjugate trick), `PartialOrd` (complex numbers have none: say why), generic `Complex<T>` with `T: Add + Mul + ...`, `Product`.
// ---------------------------------------------------------------------------------------------------------------------
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
struct Complex {
    re: i64,
    im: i64,
}
impl Complex {
    const ZERO: Complex = Complex { re: 0, im: 0 };
    const I: Complex = Complex { re: 0, im: 1 };
    fn new(re: i64, im: i64) -> Self {
        Complex { re, im }
    }
    fn norm_sqr(self) -> i64 {
        self.re * self.re + self.im * self.im
    }
    fn conj(self) -> Self {
        Complex {
            re: self.re,
            im: -self.im,
        }
    }
}
impl Add for Complex {
    type Output = Complex;
    fn add(self, o: Complex) -> Complex {
        Complex::new(self.re + o.re, self.im + o.im)
    }
}
impl Sub for Complex {
    type Output = Complex;
    fn sub(self, o: Complex) -> Complex {
        Complex::new(self.re - o.re, self.im - o.im)
    }
}
impl Mul for Complex {
    type Output = Complex;
    fn mul(self, o: Complex) -> Complex {
        Complex::new(
            self.re * o.re - self.im * o.im,
            self.re * o.im + self.im * o.re,
        )
    }
}
impl Mul<i64> for Complex {
    type Output = Complex;
    fn mul(self, k: i64) -> Complex {
        Complex::new(self.re * k, self.im * k)
    }
}
impl Mul<Complex> for i64 {
    type Output = Complex;
    fn mul(self, c: Complex) -> Complex {
        c * self
    } // scalar on the left: allowed because Complex is a local type
}
impl Neg for Complex {
    type Output = Complex;
    fn neg(self) -> Complex {
        Complex::new(-self.re, -self.im)
    }
}
impl AddAssign for Complex {
    fn add_assign(&mut self, o: Complex) {
        *self = *self + o;
    }
}
impl Sum for Complex {
    fn sum<I: Iterator<Item = Complex>>(iter: I) -> Complex {
        iter.fold(Complex::ZERO, |a, b| a + b)
    }
}
impl<'a> Sum<&'a Complex> for Complex {
    fn sum<I: Iterator<Item = &'a Complex>>(iter: I) -> Complex {
        iter.copied().sum()
    }
}
impl<'a> Add<&'a Complex> for &'a Complex {
    type Output = Complex;
    fn add(self, o: &Complex) -> Complex {
        *self + *o
    }
}
impl fmt::Display for Complex {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.im < 0 {
            write!(f, "{}-{}i", self.re, -self.im)
        } else {
            write!(f, "{}+{}i", self.re, self.im)
        }
    }
}
fn q16() {
    assert_eq!(Complex::I * Complex::I, Complex::new(-1, 0));
    assert_eq!(
        Complex::new(1, 2) * Complex::new(3, 4),
        Complex::new(-5, 10)
    );
    assert_eq!(3 * Complex::new(1, -1), Complex::new(3, -3));
    assert_eq!((-Complex::new(2, 3)).to_string(), "-2-3i");
    assert_eq!(Complex::new(2, 3).to_string(), "2+3i");
    assert_eq!(
        Vec::<Complex>::new().into_iter().sum::<Complex>(),
        Complex::ZERO
    );
    let mut r = Rng(0x1616_7777_0000_1616);
    let rc = |r: &mut Rng| Complex::new(r.range(41) as i64 - 20, r.range(41) as i64 - 20);
    for _ in 0..2000 {
        let (a, b, c) = (rc(&mut r), rc(&mut r), rc(&mut r));
        assert_eq!(a + b, b + a);
        assert_eq!(a * b, b * a);
        assert_eq!((a * b) * c, a * (b * c));
        assert_eq!(a * (b + c), a * b + a * c, "distributive law");
        assert_eq!(a - b, a + -b);
        assert_eq!(
            (a * b).norm_sqr(),
            a.norm_sqr() * b.norm_sqr(),
            "|ab|^2 = |a|^2 |b|^2"
        );
        assert_eq!(a * a.conj(), Complex::new(a.norm_sqr(), 0));
        let v: Vec<Complex> = (0..r.range(10)).map(|_| rc(&mut r)).collect();
        let by_fold = v.iter().fold(Complex::ZERO, |acc, x| acc + *x);
        assert_eq!(v.iter().sum::<Complex>(), by_fold); // Sum<&Complex>
        assert_eq!(v.iter().copied().sum::<Complex>(), by_fold); // Sum<Complex>
        let mut acc = Complex::ZERO;
        v.iter().for_each(|x| acc += *x);
        assert_eq!(acc, by_fold);
        assert_eq!(&a + &b, a + b);
    }
}

fn main() {
    q1();
    println!("Q1: ok");
    q2();
    println!("Q2: ok");
    q3();
    println!("Q3: ok");
    q4();
    println!("Q4: ok");
    q5();
    println!("Q5: ok");
    q6();
    println!("Q6: ok");
    q7();
    println!("Q7: ok");
    q8();
    println!("Q8: ok");
    q9();
    println!("Q9: ok");
    q10();
    println!("Q10: ok");
    q11();
    println!("Q11: ok");
    q12();
    println!("Q12: ok");
    q13();
    println!("Q13: ok");
    q14();
    println!("Q14: ok");
    q15();
    println!("Q15: ok");
    q16();
    println!("Q16: ok");
}
