use std::cell::UnsafeCell;
use std::mem::MaybeUninit;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::Arc;

struct Shared<T> {
    /// A power-of-two number of slots, so that `count & (len - 1)` stays correct when a counter wraps past `usize::MAX`.
    buf: Box<[UnsafeCell<MaybeUninit<T>>]>,
    capacity: usize,   // how many items may wait; `buf` may be larger
    head: AtomicUsize, // next slot to pop: written only by the consumer
    tail: AtomicUsize, // next slot to push: written only by the producer
    one_end_gone: AtomicBool, // set (Release) by whichever end is dropped first
}

// SAFETY: each slot is touched by only one side at a time; the head/tail handshake hands it over.
unsafe impl<T: Send> Sync for Shared<T> {}

pub struct Producer<T> {
    shared: Arc<Shared<T>>,
}

pub struct Consumer<T> {
    shared: Arc<Shared<T>>,
}

impl<T> Shared<T> {
    fn slot(&self, count: usize) -> *mut MaybeUninit<T> {
        self.buf[count & (self.buf.len() - 1)].get()
    }
}

pub fn channel<T>(capacity: usize) -> (Producer<T>, Consumer<T>) {
    assert!(capacity > 0, "capacity must be at least 1");
    let slots = capacity
        .checked_next_power_of_two()
        .expect("capacity too large");
    let buf: Box<[UnsafeCell<MaybeUninit<T>>]> = (0..slots)
        .map(|_| UnsafeCell::new(MaybeUninit::uninit()))
        .collect();
    let shared = Arc::new(Shared {
        buf,
        capacity,
        head: AtomicUsize::new(0),
        tail: AtomicUsize::new(0),
        one_end_gone: AtomicBool::new(false),
    });
    (
        Producer {
            shared: shared.clone(),
        },
        Consumer { shared },
    )
}

impl<T> Producer<T> {
    /// Gives `value` back when the buffer is full.
    pub fn push(&mut self, value: T) -> Result<(), T> {
        let s = &*self.shared;
        let tail = s.tail.load(Ordering::Relaxed); // only we write it
        let head = s.head.load(Ordering::Acquire);
        if tail.wrapping_sub(head) == s.capacity {
            return Err(value);
        }
        unsafe { (*s.slot(tail)).write(value) };
        s.tail.store(tail.wrapping_add(1), Ordering::Release);
        Ok(())
    }

    /// True once the consumer has been dropped.
    pub fn is_abandoned(&self) -> bool {
        // Acquire pairs with the Release store in the other end's `drop`: after `true`, everything it did is visible.
        self.shared.one_end_gone.load(Ordering::Acquire)
    }
}

impl<T> Consumer<T> {
    pub fn pop(&mut self) -> Option<T> {
        let s = &*self.shared;
        let head = s.head.load(Ordering::Relaxed); // only we write it
        let tail = s.tail.load(Ordering::Acquire);
        if head == tail {
            return None;
        }
        let value = unsafe { (*s.slot(head)).assume_init_read() };
        s.head.store(head.wrapping_add(1), Ordering::Release);
        Some(value)
    }

    /// True once the producer has been dropped. Items may still be waiting: pop until `None` after seeing this.
    pub fn is_abandoned(&self) -> bool {
        // Acquire pairs with the Release store in the other end's `drop`: after `true`, everything it did is visible.
        self.shared.one_end_gone.load(Ordering::Acquire)
    }
}

impl<T> Drop for Producer<T> {
    fn drop(&mut self) {
        self.shared.one_end_gone.store(true, Ordering::Release);
    }
}

impl<T> Drop for Consumer<T> {
    fn drop(&mut self) {
        self.shared.one_end_gone.store(true, Ordering::Release);
    }
}

impl<T> Drop for Shared<T> {
    fn drop(&mut self) {
        let (mut i, tail) = (*self.head.get_mut(), *self.tail.get_mut());
        let _ = (&mut i, tail);
    }
}
