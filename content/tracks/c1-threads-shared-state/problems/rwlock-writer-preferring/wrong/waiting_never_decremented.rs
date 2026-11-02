use std::cell::UnsafeCell;
use std::ops::{Deref, DerefMut};
use std::sync::{Condvar, Mutex};

#[derive(Default)]
struct State {
    readers: usize,
    writer: bool,
    writers_waiting: usize,
}

/// A readers-writer lock that prefers writers: once a writer is waiting, no new reader gets in.
pub struct RwLock<T> {
    state: Mutex<State>,
    cv: Condvar,
    value: UnsafeCell<T>,
}

unsafe impl<T: Send> Send for RwLock<T> {}
unsafe impl<T: Send + Sync> Sync for RwLock<T> {}

pub struct ReadGuard<'a, T> {
    lock: &'a RwLock<T>,
}

pub struct WriteGuard<'a, T> {
    lock: &'a RwLock<T>,
}

impl<T> RwLock<T> {
    pub fn new(value: T) -> Self {
        RwLock { state: Mutex::new(State::default()), cv: Condvar::new(), value: UnsafeCell::new(value) }
    }

    pub fn read(&self) -> ReadGuard<'_, T> {
        let mut s = self.state.lock().unwrap();
        while s.writer || s.writers_waiting > 0 {
            s = self.cv.wait(s).unwrap();
        }
        s.readers += 1;
        ReadGuard { lock: self }
    }

    pub fn try_read(&self) -> Option<ReadGuard<'_, T>> {
        let mut s = self.state.lock().unwrap();
        if s.writer || s.writers_waiting > 0 {
            return None;
        }
        s.readers += 1;
        Some(ReadGuard { lock: self })
    }

    pub fn write(&self) -> WriteGuard<'_, T> {
        let mut s = self.state.lock().unwrap();
        s.writers_waiting += 1;
        while s.writer || s.readers > 0 {
            s = self.cv.wait(s).unwrap();
        }
        s.writer = true;
        WriteGuard { lock: self }
    }

    pub fn try_write(&self) -> Option<WriteGuard<'_, T>> {
        let mut s = self.state.lock().unwrap();
        if s.writer || s.readers > 0 {
            return None;
        }
        s.writer = true;
        Some(WriteGuard { lock: self })
    }

    /// Writers queued in `write` (not counting one that holds the lock).
    pub fn pending_writers(&self) -> usize {
        self.state.lock().unwrap().writers_waiting
    }

    pub fn into_inner(self) -> T {
        self.value.into_inner()
    }
}

impl<T> Deref for ReadGuard<'_, T> {
    type Target = T;
    fn deref(&self) -> &T {
        unsafe { &*self.lock.value.get() }
    }
}

impl<T> Drop for ReadGuard<'_, T> {
    fn drop(&mut self) {
        let mut s = self.lock.state.lock().unwrap();
        s.readers -= 1;
        if s.readers == 0 {
            self.lock.cv.notify_all();
        }
    }
}

impl<T> Deref for WriteGuard<'_, T> {
    type Target = T;
    fn deref(&self) -> &T {
        unsafe { &*self.lock.value.get() }
    }
}

impl<T> DerefMut for WriteGuard<'_, T> {
    fn deref_mut(&mut self) -> &mut T {
        unsafe { &mut *self.lock.value.get() }
    }
}

impl<T> Drop for WriteGuard<'_, T> {
    fn drop(&mut self) {
        self.lock.state.lock().unwrap().writer = false;
        self.lock.cv.notify_all();
    }
}
