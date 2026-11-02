use std::cell::UnsafeCell;
use std::ops::{Deref, DerefMut};
use std::sync::{Condvar, Mutex};

#[derive(Default)]
struct State {
    readers: usize,
    writer: bool,
    writers_waiting: usize,
}

pub struct RwLock<T> {
    state: Mutex<State>,
    cv: Condvar,
    value: UnsafeCell<T>,
}

// Given loose so the tests compile: tighten them (a `&RwLock<T>` hands `&T` to many threads at once).
unsafe impl<T> Send for RwLock<T> {}
unsafe impl<T> Sync for RwLock<T> {}

pub struct ReadGuard<'a, T> {
    lock: &'a RwLock<T>,
}

pub struct WriteGuard<'a, T> {
    lock: &'a RwLock<T>,
}

impl<T> RwLock<T> {
    pub fn new(value: T) -> Self {
        todo!()
    }

    /// Many readers at once; waits while a writer holds the lock OR is waiting for it.
    pub fn read(&self) -> ReadGuard<'_, T> {
        todo!()
    }

    pub fn try_read(&self) -> Option<ReadGuard<'_, T>> {
        todo!()
    }

    /// Exclusive. Announces itself as waiting so new readers stay out, then waits for the readers to leave.
    pub fn write(&self) -> WriteGuard<'_, T> {
        todo!()
    }

    pub fn try_write(&self) -> Option<WriteGuard<'_, T>> {
        todo!()
    }

    /// Writers currently queued inside `write`.
    pub fn pending_writers(&self) -> usize {
        todo!()
    }

    pub fn into_inner(self) -> T {
        todo!()
    }
}

impl<T> Deref for ReadGuard<'_, T> {
    type Target = T;
    fn deref(&self) -> &T {
        todo!()
    }
}

impl<T> Deref for WriteGuard<'_, T> {
    type Target = T;
    fn deref(&self) -> &T {
        todo!()
    }
}

impl<T> DerefMut for WriteGuard<'_, T> {
    fn deref_mut(&mut self) -> &mut T {
        todo!()
    }
}

// TODO: `Drop` for both guards: update the state and wake the waiters.
