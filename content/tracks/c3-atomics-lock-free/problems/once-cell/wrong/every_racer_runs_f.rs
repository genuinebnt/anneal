use std::cell::UnsafeCell;
use std::mem::MaybeUninit;
use std::sync::atomic::{AtomicU8, Ordering};

const EMPTY: u8 = 0;
const RUNNING: u8 = 1;
const READY: u8 = 2;

/// A cell written at most once, from any thread, through `&self`.
pub struct Once<T> {
    state: AtomicU8,
    slot: UnsafeCell<MaybeUninit<T>>,
}

// SAFETY: the value is written by exactly one thread (the one that moved EMPTY -> RUNNING) and only read after
// READY was observed with Acquire, which pairs with the writer's Release store.
unsafe impl<T: Send + Sync> Sync for Once<T> {}
unsafe impl<T: Send> Send for Once<T> {}

/// Puts the state back to EMPTY if the initializer unwinds.
struct Reset<'a>(&'a AtomicU8);

impl Drop for Reset<'_> {
    fn drop(&mut self) {
        self.0.store(EMPTY, Ordering::Release);
    }
}

impl<T> Once<T> {
    pub const fn new() -> Self {
        Once { state: AtomicU8::new(EMPTY), slot: UnsafeCell::new(MaybeUninit::uninit()) }
    }

    pub fn get(&self) -> Option<&T> {
        if self.state.load(Ordering::Acquire) == READY {
            // SAFETY: READY means the slot is initialized and never written again.
            Some(unsafe { (*self.slot.get()).assume_init_ref() })
        } else {
            None
        }
    }

    pub fn get_or_init<F: FnOnce() -> T>(&self, f: F) -> &T {
        loop {
            match (if self.state.load(Ordering::Acquire) == EMPTY { Ok(0) } else { Err(self.state.load(Ordering::Acquire)) }) {
                Ok(_) => {
                    let reset = Reset(&self.state);
                    let value = f();
                    // SAFETY: we own the RUNNING state, so nobody else touches the slot.
                    unsafe { (*self.slot.get()).write(value) };
                    std::mem::forget(reset);
                    self.state.store(READY, Ordering::Release);
                    break;
                }
                Err(READY) => break,
                Err(_) => std::thread::yield_now(), // someone else is initializing: wait for READY or a reset
            }
        }
        self.get().unwrap()
    }

    pub fn into_inner(mut self) -> Option<T> {
        if *self.state.get_mut() == READY {
            *self.state.get_mut() = EMPTY; // so Drop doesn't drop it again
            Some(unsafe { self.slot.get_mut().assume_init_read() })
        } else {
            None
        }
    }
}

impl<T> Drop for Once<T> {
    fn drop(&mut self) {
        if *self.state.get_mut() == READY {
            unsafe { self.slot.get_mut().assume_init_drop() };
        }
    }
}
