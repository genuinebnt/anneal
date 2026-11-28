// WRONG: keeps the starter's loose bounds, so a `SpinLock<Rc<u8>>` can be shared between threads and two
// threads end up bumping the same non-atomic reference count.

use std::cell::UnsafeCell;
use std::ops::{Deref, DerefMut};
use std::sync::atomic::{AtomicBool, Ordering};

pub struct SpinLock<T> {
    locked: AtomicBool,
    value: UnsafeCell<T>,
}

unsafe impl<T> Sync for SpinLock<T> {}
unsafe impl<T> Send for SpinLock<T> {}

pub struct Guard<'a, T> {
    lock: &'a SpinLock<T>,
}

// Without this line the compiler derives `Guard: Sync` from `&SpinLock<T>: Sync`, i.e. from `T: Send`. But `&Guard`
// derefs to `&T`, so sharing a guard between threads shares `&T`: that needs `T: Sync`. An explicit impl replaces the
// derived one (`std::sync::MutexGuard` does the same).
unsafe impl<T: Sync> Sync for Guard<'_, T> {}

impl<T> SpinLock<T> {
    pub const fn new(value: T) -> Self {
        SpinLock { locked: AtomicBool::new(false), value: UnsafeCell::new(value) }
    }

    pub fn lock(&self) -> Guard<'_, T> {
        loop {
            if let Some(g) = self.try_lock() {
                return g;
            }
            // test-and-test-and-set: wait on a plain load so the cache line stays shared while it is held
            while self.locked.load(Ordering::Relaxed) {
                std::hint::spin_loop();
            }
        }
    }

    pub fn try_lock(&self) -> Option<Guard<'_, T>> {
        if self.locked.compare_exchange(false, true, Ordering::Acquire, Ordering::Relaxed).is_ok() {
            Some(Guard { lock: self })
        } else {
            None
        }
    }

    pub fn into_inner(self) -> T {
        self.value.into_inner()
    }

    pub fn get_mut(&mut self) -> &mut T {
        self.value.get_mut()
    }
}

impl<T> Deref for Guard<'_, T> {
    type Target = T;
    fn deref(&self) -> &T {
        unsafe { &*self.lock.value.get() }
    }
}

impl<T> DerefMut for Guard<'_, T> {
    fn deref_mut(&mut self) -> &mut T {
        unsafe { &mut *self.lock.value.get() }
    }
}

impl<T> Drop for Guard<'_, T> {
    fn drop(&mut self) {
        self.lock.locked.store(false, Ordering::Release);
    }
}
