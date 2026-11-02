use std::cell::UnsafeCell;
use std::ops::{Deref, DerefMut};
use std::sync::atomic::{AtomicBool, Ordering};

pub struct SpinLock<T> {
    locked: AtomicBool,
    value: UnsafeCell<T>,
}

// Given loose so the tests compile: tighten the bounds to what is actually sound.
unsafe impl<T> Sync for SpinLock<T> {}
unsafe impl<T> Send for SpinLock<T> {}

pub struct Guard<'a, T> {
    lock: &'a SpinLock<T>,
}

impl<T> SpinLock<T> {
    pub const fn new(value: T) -> Self {
        todo!()
    }

    /// Spins until the lock is free.
    pub fn lock(&self) -> Guard<'_, T> {
        todo!()
    }

    /// `None` immediately if somebody holds it.
    pub fn try_lock(&self) -> Option<Guard<'_, T>> {
        todo!()
    }

    pub fn into_inner(self) -> T {
        todo!()
    }

    /// No locking needed: `&mut self` proves nobody else has it.
    pub fn get_mut(&mut self) -> &mut T {
        todo!()
    }
}

impl<T> Deref for Guard<'_, T> {
    type Target = T;
    fn deref(&self) -> &T {
        todo!()
    }
}

impl<T> DerefMut for Guard<'_, T> {
    fn deref_mut(&mut self) -> &mut T {
        todo!()
    }
}

// TODO: `Drop` for `Guard`.
