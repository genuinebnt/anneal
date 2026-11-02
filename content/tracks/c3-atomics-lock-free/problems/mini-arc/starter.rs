use std::marker::PhantomData;
use std::ops::Deref;
use std::ptr::NonNull;
use std::sync::atomic::{fence, AtomicUsize, Ordering};

struct Inner<T> {
    strong: AtomicUsize,
    data: T,
}

pub struct MiniArc<T> {
    ptr: NonNull<Inner<T>>,
    _owns: PhantomData<Inner<T>>,
}

// Given loose so the tests compile: tighten them to what sharing a `T` across threads really requires.
unsafe impl<T> Send for MiniArc<T> {}
unsafe impl<T> Sync for MiniArc<T> {}

impl<T> MiniArc<T> {
    pub fn new(data: T) -> Self {
        todo!()
    }

    /// Number of live owners.
    pub fn strong_count(this: &Self) -> usize {
        todo!()
    }

    pub fn ptr_eq(a: &Self, b: &Self) -> bool {
        todo!()
    }

    /// `Some` only while `this` is the sole owner.
    pub fn get_mut(this: &mut Self) -> Option<&mut T> {
        todo!()
    }

    /// Moves the value out when `this` is the only owner, otherwise gives `this` back untouched.
    pub fn try_unwrap(this: Self) -> Result<T, Self> {
        todo!()
    }

    /// Clone-on-write: unique owners get `&mut T` directly; a shared value is cloned into a fresh allocation first.
    pub fn make_mut(this: &mut Self) -> &mut T
    where
        T: Clone,
    {
        todo!()
    }
}

impl<T> Deref for MiniArc<T> {
    type Target = T;
    fn deref(&self) -> &T {
        todo!()
    }
}

// TODO: `Clone` (what ordering does a count increment need?) and `Drop` (which ordering, and what fence, before freeing?).
impl<T> Clone for MiniArc<T> {
    fn clone(&self) -> Self {
        todo!()
    }
}

impl<T> Drop for MiniArc<T> {
    fn drop(&mut self) {}
}
