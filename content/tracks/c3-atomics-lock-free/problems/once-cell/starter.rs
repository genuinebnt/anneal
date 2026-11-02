use std::cell::UnsafeCell;
use std::mem::MaybeUninit;
use std::sync::atomic::{AtomicU8, Ordering};

const EMPTY: u8 = 0;
const RUNNING: u8 = 1;
const READY: u8 = 2;

pub struct Once<T> {
    state: AtomicU8,
    slot: UnsafeCell<MaybeUninit<T>>,
}

// These compile as given so the tests can share the cell across threads. Decide what bound `T` really needs for
// `Sync` and `Send` to be sound, and tighten them.
unsafe impl<T> Sync for Once<T> {}
unsafe impl<T> Send for Once<T> {}

impl<T> Once<T> {
    pub const fn new() -> Self {
        todo!()
    }

    /// `Some` once a value has been stored.
    pub fn get(&self) -> Option<&T> {
        todo!()
    }

    /// Runs `f` in exactly one caller, even when many threads race here; the others wait and then all of them get
    /// the same reference. If `f` panics, the cell stays empty so a later caller can try again.
    pub fn get_or_init<F: FnOnce() -> T>(&self, f: F) -> &T {
        todo!()
    }

    pub fn into_inner(self) -> Option<T> {
        todo!()
    }
}

// TODO: `Drop`, so a stored value is dropped exactly once.
