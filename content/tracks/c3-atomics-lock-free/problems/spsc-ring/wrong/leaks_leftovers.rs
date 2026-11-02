use std::cell::UnsafeCell;
use std::mem::MaybeUninit;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;

struct Shared<T> {
    buf: Box<[UnsafeCell<MaybeUninit<T>>]>,
    head: AtomicUsize, // next slot to pop: written only by the consumer
    tail: AtomicUsize, // next slot to push: written only by the producer
}

// SAFETY: each slot is touched by only one side at a time; the head/tail handshake hands it over.
unsafe impl<T: Send> Sync for Shared<T> {}

pub struct Producer<T> {
    shared: Arc<Shared<T>>,
}

pub struct Consumer<T> {
    shared: Arc<Shared<T>>,
}

pub fn channel<T>(capacity: usize) -> (Producer<T>, Consumer<T>) {
    assert!(capacity > 0, "capacity must be at least 1");
    let buf: Box<[UnsafeCell<MaybeUninit<T>>]> = (0..capacity).map(|_| UnsafeCell::new(MaybeUninit::uninit())).collect();
    let shared = Arc::new(Shared { buf, head: AtomicUsize::new(0), tail: AtomicUsize::new(0) });
    (Producer { shared: shared.clone() }, Consumer { shared })
}

impl<T> Producer<T> {
    /// Gives `value` back when the buffer is full.
    pub fn push(&mut self, value: T) -> Result<(), T> {
        let s = &*self.shared;
        let tail = s.tail.load(Ordering::Relaxed); // only we write it
        let head = s.head.load(Ordering::Acquire);
        if tail.wrapping_sub(head) == s.buf.len() {
            return Err(value);
        }
        unsafe { (*s.buf[tail % s.buf.len()].get()).write(value) };
        s.tail.store(tail.wrapping_add(1), Ordering::Release);
        Ok(())
    }

    /// True once the consumer has been dropped.
    pub fn is_abandoned(&self) -> bool {
        Arc::strong_count(&self.shared) == 1
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
        let value = unsafe { (*s.buf[head % s.buf.len()].get()).assume_init_read() };
        s.head.store(head.wrapping_add(1), Ordering::Release);
        Some(value)
    }

    /// True once the producer has been dropped. Items may still be waiting: pop until `None` after seeing this.
    pub fn is_abandoned(&self) -> bool {
        Arc::strong_count(&self.shared) == 1
    }
}

impl<T> Drop for Shared<T> {
    fn drop(&mut self) {
        let (mut i, tail) = (*self.head.get_mut(), *self.tail.get_mut());
        let _ = (&mut i, tail);
    }
}
