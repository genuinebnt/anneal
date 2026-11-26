//! RECIPE 101 (unsafe): UnsafeCell: build your own `Cell` and `RefCell` (interior mutability from first principles).
//! LEVEL: advanced
//! Use in: understanding Cell/RefCell/Mutex/atomics, writing custom synchronisation primitives or caches.   Run: cargo run --release --example unsafecell_cells
#![allow(dead_code)]
//!
//! The compiler assumes data behind `&T` is immutable (and may cache reads / mark it `noalias readonly`).
//! Mutating it anyway, e.g. `*(r as *const T as *mut T) = v`, is UB. `UnsafeCell<T>` is the ONE language primitive
//! that opts out of that assumption: `UnsafeCell::get(&self) -> *mut T` is how mutation through `&` is allowed.
//! `Cell`, `RefCell`, `OnceCell`, `Mutex`, `RwLock` and every atomic are all `UnsafeCell` plus a rule that keeps
//! accesses from overlapping: Cell never hands out references (copy in/out), RefCell counts borrows at runtime,
//! Mutex uses an OS lock, atomics use hardware instructions.
//! `UnsafeCell` is already `!Sync`; we also add `PhantomData<*const ()>` to make that intent explicit (raw pointers
//! are neither Send nor Sync). Without !Sync, two threads could call `set` at once: a data race.
//!   fn assert_sync<T: Sync>() {}  assert_sync::<MyCell<i32>>();   // error[E0277]: `*const ()` cannot be shared between threads
//! Our RefCell invariant: `state == 0` free, `n > 0` means n live `Ref`s, `-1` means one live `RefMut`.
//! A guard exists only while it holds its slice of that state, so `&mut T` is never aliased.
//! COMMON USES: shared mutable state in single-threaded graphs/trees (Rc<RefCell<_>>), caches behind `&self`,
//! lazy initialisation, test doubles that record calls, GUI widget trees.
//! Read: https://doc.rust-lang.org/std/cell/struct.UnsafeCell.html and https://doc.rust-lang.org/nomicon/
use std::cell::{Cell, RefCell, UnsafeCell};
use std::marker::PhantomData;
use std::ops::{Deref, DerefMut};
use std::panic::{AssertUnwindSafe, catch_unwind};

struct MyCell<T> {
    value: UnsafeCell<T>,
    _not_sync: PhantomData<*const ()>,
}
// SAFETY: moving the whole cell to another thread moves sole ownership of T, which is fine when T: Send.
unsafe impl<T: Send> Send for MyCell<T> {}

impl<T> MyCell<T> {
    fn new(v: T) -> Self {
        Self {
            value: UnsafeCell::new(v),
            _not_sync: PhantomData,
        }
    }
    fn replace(&self, new: T) -> T {
        // SAFETY: MyCell is !Sync so no other thread can be here, and we never hand out references into the cell,
        // so no reference to the old value exists. `mem::replace` completes before any user code (Drop) runs.
        std::mem::replace(unsafe { &mut *self.value.get() }, new)
    }
    fn set(&self, v: T) {
        drop(self.replace(v)); // old value is dropped AFTER the swap, so a reentrant Drop sees consistent state
    }
    fn take(&self) -> T
    where
        T: Default,
    {
        self.replace(T::default())
    }
    fn get(&self) -> T
    where
        T: Copy,
    {
        // SAFETY: !Sync (no concurrent access); T: Copy means the read runs no user code and no reference escapes.
        unsafe { *self.value.get() }
    }
}

#[derive(Debug, PartialEq)]
struct BorrowError;
#[derive(Debug, PartialEq)]
struct BorrowMutError;

struct MyRefCell<T> {
    value: UnsafeCell<T>,
    state: Cell<isize>, // 0 free, >0 reader count, -1 writer
    _not_sync: PhantomData<*const ()>,
}
struct Ref<'a, T> {
    cell: &'a MyRefCell<T>,
}
struct RefMut<'a, T> {
    cell: &'a MyRefCell<T>,
}

impl<T> MyRefCell<T> {
    fn new(v: T) -> Self {
        Self {
            value: UnsafeCell::new(v),
            state: Cell::new(0),
            _not_sync: PhantomData,
        }
    }
    fn try_borrow(&self) -> Result<Ref<'_, T>, BorrowError> {
        let s = self.state.get();
        if s < 0 {
            return Err(BorrowError);
        }
        self.state.set(s + 1);
        Ok(Ref { cell: self })
    }
    fn try_borrow_mut(&self) -> Result<RefMut<'_, T>, BorrowMutError> {
        if self.state.get() != 0 {
            return Err(BorrowMutError);
        }
        self.state.set(-1);
        Ok(RefMut { cell: self })
    }
    fn borrow(&self) -> Ref<'_, T> {
        self.try_borrow().expect("already mutably borrowed")
    }
    fn borrow_mut(&self) -> RefMut<'_, T> {
        self.try_borrow_mut().expect("already borrowed")
    }
}
impl<T> Deref for Ref<'_, T> {
    type Target = T;
    fn deref(&self) -> &T {
        // SAFETY: a live Ref means state > 0, so no RefMut exists; only shared references to the value are made.
        unsafe { &*self.cell.value.get() }
    }
}
impl<T> Drop for Ref<'_, T> {
    fn drop(&mut self) {
        self.cell.state.set(self.cell.state.get() - 1);
    }
}
impl<T> Deref for RefMut<'_, T> {
    type Target = T;
    fn deref(&self) -> &T {
        // SAFETY: a live RefMut means state == -1: it is the only access path to the value.
        unsafe { &*self.cell.value.get() }
    }
}
impl<T> DerefMut for RefMut<'_, T> {
    fn deref_mut(&mut self) -> &mut T {
        // SAFETY: as above, and `&mut self` makes this the only reference derived from this guard.
        unsafe { &mut *self.cell.value.get() }
    }
}
impl<T> Drop for RefMut<'_, T> {
    fn drop(&mut self) {
        self.cell.state.set(0);
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
    let c = MyCell::new(String::from("one"));
    c.set("two".into());
    assert_eq!(c.replace("three".into()), "two");
    assert_eq!(c.take(), "three");
    assert_eq!(c.take(), "");
    let n = MyCell::new(5);
    n.set(n.get() + 1);
    assert_eq!(n.get(), 6);

    let rc = MyRefCell::new(vec![1, 2]);
    {
        let a = rc.borrow();
        let b = rc.borrow();
        assert_eq!((a.len(), b.len()), (2, 2));
        assert_eq!(rc.try_borrow_mut().err(), Some(BorrowMutError));
    }
    rc.borrow_mut().push(3);
    {
        let _w = rc.borrow_mut();
        assert_eq!(rc.try_borrow().err(), Some(BorrowError));
    }
    assert_eq!(*rc.borrow(), vec![1, 2, 3]);

    // Model test: random borrow/release sequences must agree with std::cell::RefCell at every step.
    let (mine, std_c) = (MyRefCell::new(0i64), RefCell::new(0i64));
    let mut rng = Rng(0x1234_5678_9ABC_DEF1);
    let mut readers: Vec<(std::cell::Ref<i64>, Ref<i64>)> = Vec::new();
    let mut writer: Option<(std::cell::RefMut<i64>, RefMut<i64>)> = None;
    for _ in 0..20_000 {
        match rng.next() % 5 {
            0 => {
                let (a, b) = (std_c.try_borrow(), mine.try_borrow());
                assert_eq!(a.is_ok(), b.is_ok());
                if let (Ok(a), Ok(b)) = (a, b) {
                    assert_eq!(*a, *b);
                    readers.push((a, b));
                }
            }
            1 => {
                let (a, b) = (std_c.try_borrow_mut(), mine.try_borrow_mut());
                assert_eq!(a.is_ok(), b.is_ok());
                if let (Ok(a), Ok(b)) = (a, b) {
                    writer = Some((a, b));
                }
            }
            2 if !readers.is_empty() => {
                let i = (rng.next() % readers.len() as u64) as usize;
                readers.swap_remove(i);
            }
            3 => writer = None,
            _ => {
                if let Some((a, b)) = writer.as_mut() {
                    let k = (rng.next() % 100) as i64;
                    **a += k;
                    **b += k;
                    assert_eq!(**a, **b);
                }
            }
        }
    }
    drop(readers);
    drop(writer);
    assert_eq!(*std_c.borrow(), *mine.borrow());

    // The double-borrow panic is catchable, and guards released during unwinding leave the cell usable.
    std::panic::set_hook(Box::new(|_| {}));
    let r = catch_unwind(AssertUnwindSafe(|| {
        let _a = rc.borrow_mut();
        let _b = rc.borrow_mut(); // panics: "already borrowed"
    }));
    let _ = std::panic::take_hook();
    assert!(r.is_err());
    assert!(rc.try_borrow_mut().is_ok());
    println!(
        "MyCell/MyRefCell agree with std over 20000 random steps; final value {}",
        *mine.borrow()
    );
}
