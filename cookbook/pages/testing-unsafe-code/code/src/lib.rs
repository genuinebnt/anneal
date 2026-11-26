//! `Shelf<T, N>`: a fixed-capacity, append-only list that you can push to through `&self`.
//!
//! `push` returns a reference that stays valid while you keep pushing. A `Vec` cannot offer that: `Vec::push` needs `&mut self`.
//! The price is a few lines of `unsafe`, and this crate is about testing them.
//!
//! # What must not compile
//!
//! Each `compile_fail` block has a twin that differs only by the offending line, and the twin must compile. Without the twin, a
//! typo would also make the block "fail to compile" and the test would pass for the wrong reason. The error codes are only
//! enforced by nightly rustdoc.
//!
//! A shelf cannot be shared between threads (its length is a `Cell`):
//!
//! ```compile_fail,E0277
//! fn needs_sync<T: Sync>() {}
//! needs_sync::<shelf::Shelf<u8, 4>>();
//! ```
//! ```
//! fn needs_send<T: Send>() {}
//! needs_send::<shelf::Shelf<u8, 4>>();
//! ```
//!
//! A shelf of non-`Send` values cannot move to another thread:
//!
//! ```compile_fail,E0277
//! fn needs_send<T: Send>() {}
//! needs_send::<shelf::Shelf<std::rc::Rc<u8>, 4>>();
//! ```
//!
//! `clear` cannot run while a reference into the shelf is alive:
//!
//! ```compile_fail,E0502
//! let mut s = shelf::Shelf::<String, 4>::new();
//! let first = s.push("a".to_string()).unwrap();
//! s.clear();
//! println!("{first}");
//! ```
//! ```
//! let mut s = shelf::Shelf::<String, 4>::new();
//! let first = s.push("a".to_string()).unwrap();
//! println!("{first}");
//! s.clear();
//! ```
//!
//! The shelf cannot be moved while a reference into it is alive (its elements are stored inline, so they would move too):
//!
//! ```compile_fail,E0505
//! let s = shelf::Shelf::<String, 4>::new();
//! let first = s.push("a".to_string()).unwrap();
//! let moved = s;
//! println!("{first}");
//! ```
//! ```
//! let s = shelf::Shelf::<String, 4>::new();
//! let first = s.push("a".to_string()).unwrap();
//! println!("{first}");
//! let moved = s;
//! ```
//!
//! `Shelf<T>` is invariant in `T`. If it were covariant, safe code could view a shelf of long-lived references as a shelf of
//! short-lived ones, push a short-lived one through `&self`, and read it after it died:
//!
//! ```compile_fail
//! fn shorten<'s, 'a>(s: &'s shelf::Shelf<&'static str, 4>) -> &'s shelf::Shelf<&'a str, 4> {
//!     s
//! }
//! ```
//! ```
//! fn same<'s>(s: &'s shelf::Shelf<&'static str, 4>) -> &'s shelf::Shelf<&'static str, 4> {
//!     s
//! }
//! ```

#[cfg(all(
    any(feature = "planted-lying-len", feature = "planted-stale-index", feature = "planted-early-len"),
    not(miri),
    not(planted_native)
))]
compile_error!(
    "the planted-* unsound variants execute undefined behaviour: build them only with `cargo +nightly miri test`"
);

pub mod safe;
pub mod script;

use std::cell::{Cell, UnsafeCell};
use std::mem::MaybeUninit;

/// The sequence handed to [`Shelf::extend_exact`] says it is longer than the free space.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct NoRoom;

/// A fixed-capacity, append-only list that can be pushed to through `&self`.
///
/// # Invariants
///
/// Every `unsafe` block in this file relies on these. Only this module can break them, because both fields are private.
///
/// 1. `len <= N`.
/// 2. Slots `0..len` each hold an initialised `T` that the shelf owns. Slots `len..N` hold nothing it owns.
/// 3. While any `&self` borrow is alive, a slot below `len` is not written, moved or dropped. Only `&mut self` methods and
///    `Drop` may do that.
/// 4. Only one thread can use `&self` at a time. `Cell` and `UnsafeCell` make the type `!Sync`, so the compiler enforces this.
///
/// Invariants 1 to 3 must hold **whenever code we did not write runs** (a closure, an iterator, a destructor), because that
/// code can call back into the shelf, or panic and never return.
pub struct Shelf<T, const N: usize> {
    // One `UnsafeCell` per slot, not one around the whole array: writing slot `i` must not touch the other slots.
    slots: [UnsafeCell<MaybeUninit<T>>; N],
    len: Cell<usize>,
}

impl<T, const N: usize> Shelf<T, N> {
    pub fn new() -> Self {
        Shelf { slots: [const { UnsafeCell::new(MaybeUninit::uninit()) }; N], len: Cell::new(0) }
    }

    pub fn len(&self) -> usize {
        self.len.get()
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    pub fn capacity(&self) -> usize {
        N
    }

    /// Stores `value` and returns a reference to it, or gives it back if the shelf is full.
    pub fn push(&self, value: T) -> Result<&T, T> {
        let index = self.len.get();
        let Some(slot) = self.slots.get(index) else { return Err(value) };
        // SAFETY: `index == len`, so by invariant 2 the slot holds nothing and no reference into it exists. By invariant 4 no
        // other thread is here. No user code runs between reading `len` and updating it.
        let stored: &T = unsafe { (*slot.get()).write(value) };
        self.len.set(index + 1);
        Ok(stored)
    }

    #[cfg(not(feature = "planted-wrong-get"))]
    pub fn get(&self, index: usize) -> Option<&T> {
        if index >= self.len.get() {
            return None;
        }
        // SAFETY: `index < len`, so the slot is initialised (invariant 2) and nobody writes it while the returned borrow of
        // `self` is alive (invariant 3).
        Some(unsafe { (*self.slots[index].get()).assume_init_ref() })
    }

    /// PLANTED BUG, SOUND: hides the newest element. Wrong answers, no undefined behaviour.
    #[cfg(feature = "planted-wrong-get")]
    pub fn get(&self, index: usize) -> Option<&T> {
        if index + 1 >= self.len.get() {
            return None;
        }
        // SAFETY: `index + 1 < len` implies `index < len`; same argument as the correct version.
        Some(unsafe { (*self.slots[index].get()).assume_init_ref() })
    }

    /// The elements present when `iter` is called. Pushing while iterating is allowed; the new items are not visited.
    pub fn iter(&self) -> impl Iterator<Item = &T> {
        (0..self.len()).map_while(|i| self.get(i))
    }

    /// Appends a sequence that knows its length. Refuses up front if that length exceeds the free space; returns how many
    /// elements were stored. A wrong `len()` can make the count surprising, but never makes this method unsound.
    #[cfg(not(feature = "planted-lying-len"))]
    pub fn extend_exact<I>(&self, items: I) -> Result<usize, NoRoom>
    where
        I: IntoIterator<Item = T>,
        I::IntoIter: ExactSizeIterator,
    {
        let items = items.into_iter();
        if items.len() > N - self.len() {
            return Err(NoRoom); // a hint used only to refuse early: nothing unsafe depends on it
        }
        let mut stored = 0;
        for value in items {
            if self.push(value).is_err() {
                break; // `push` checks the real bound every time
            }
            stored += 1;
        }
        Ok(stored)
    }

    /// PLANTED, UNSOUND: "we already checked `len()`, so skip the bounds check". `ExactSizeIterator` is a safe trait.
    #[cfg(feature = "planted-lying-len")]
    pub fn extend_exact<I>(&self, items: I) -> Result<usize, NoRoom>
    where
        I: IntoIterator<Item = T>,
        I::IntoIter: ExactSizeIterator,
    {
        let items = items.into_iter();
        if items.len() > N - self.len() {
            return Err(NoRoom);
        }
        let mut stored = 0;
        for value in items {
            let index = self.len.get();
            // SAFETY: WRONG. Assumes the iterator yields no more than `len()` items.
            unsafe { (*self.slots.get_unchecked(index).get()).write(value) };
            self.len.set(index + 1);
            stored += 1;
        }
        Ok(stored)
    }

    /// Pushes up to `count` values made by `make(0)`, `make(1)`, ... and returns how many of them were stored.
    /// `make` may use the shelf, including pushing to it, and may panic. Values stored before a panic stay stored.
    #[cfg(not(any(feature = "planted-stale-index", feature = "planted-early-len")))]
    pub fn fill_with(&self, count: usize, mut make: impl FnMut(usize) -> T) -> usize {
        let mut stored = 0;
        for k in 0..count {
            if self.len() == N {
                break;
            }
            let value = make(k); // user code: it may push (so `len` may change) or panic (so we may never get past this line)
            if self.push(value).is_err() {
                break; // `make` filled the shelf itself
            }
            stored += 1;
        }
        stored
    }

    /// PLANTED, UNSOUND (re-entrancy): "read `len` once, before the loop".
    #[cfg(feature = "planted-stale-index")]
    pub fn fill_with(&self, count: usize, mut make: impl FnMut(usize) -> T) -> usize {
        let start = self.len.get();
        let count = count.min(N - start);
        for k in 0..count {
            let value = make(k);
            // SAFETY: WRONG. If `make` pushed, slot `start + k` is below `len` and someone may hold a reference to it.
            unsafe { (*self.slots[start + k].get()).write(value) };
            self.len.set(start + k + 1);
        }
        count
    }

    /// PLANTED, UNSOUND (panic safety and re-entrancy): "reserve the whole range, so `len` is written once".
    #[cfg(feature = "planted-early-len")]
    pub fn fill_with(&self, count: usize, mut make: impl FnMut(usize) -> T) -> usize {
        let start = self.len.get();
        let count = count.min(N - start);
        self.len.set(start + count); // invariant 2 is false until the loop ends, and `make` runs meanwhile
        for k in 0..count {
            let value = make(k);
            // SAFETY: WRONG, for the reason above.
            unsafe { (*self.slots[start + k].get()).write(value) };
        }
        count
    }

    /// Drops every element. If a destructor panics, the elements not yet dropped stay in the shelf and no element is dropped twice.
    pub fn clear(&mut self) {
        while self.len.get() > 0 {
            let last = self.len.get() - 1;
            // Shrink FIRST: if the destructor below panics, the shelf no longer claims to own this element.
            self.len.set(last);
            // SAFETY: slot `last` was initialised (invariant 2, before the line above), `&mut self` proves no references into
            // the shelf are alive, and `len` no longer covers the slot, so it cannot be dropped again.
            unsafe { self.slots[last].get_mut().assume_init_drop() };
        }
    }
}

impl<T, const N: usize> Default for Shelf<T, N> {
    fn default() -> Self {
        Self::new()
    }
}

impl<T, const N: usize> Drop for Shelf<T, N> {
    fn drop(&mut self) {
        self.clear();
    }
}
