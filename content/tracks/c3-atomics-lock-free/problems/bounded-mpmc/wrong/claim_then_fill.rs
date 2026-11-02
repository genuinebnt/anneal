use std::cell::UnsafeCell;
use std::sync::atomic::{AtomicUsize, Ordering};

struct Slot<T> {
    value: UnsafeCell<Option<T>>,
}

/// Claims a position first, fills it afterwards: a consumer can pop a claimed-but-empty slot.
pub struct Queue<T> {
    slots: Box<[Slot<T>]>,
    head: AtomicUsize,
    tail: AtomicUsize,
}

unsafe impl<T: Send> Send for Queue<T> {}
unsafe impl<T: Send> Sync for Queue<T> {}

impl<T> Queue<T> {
    pub fn new(capacity: usize) -> Self {
        let cap = capacity.max(2).next_power_of_two();
        Queue { slots: (0..cap).map(|_| Slot { value: UnsafeCell::new(None) }).collect(), head: AtomicUsize::new(0), tail: AtomicUsize::new(0) }
    }

    pub fn capacity(&self) -> usize {
        self.slots.len()
    }

    pub fn push(&self, value: T) -> Result<(), T> {
        loop {
            let tail = self.tail.load(Ordering::Acquire);
            let head = self.head.load(Ordering::Acquire);
            if tail - head >= self.slots.len() {
                return Err(value);
            }
            if self.tail.compare_exchange(tail, tail + 1, Ordering::AcqRel, Ordering::Relaxed).is_ok() {
                std::thread::yield_now();
                unsafe { *self.slots[tail % self.slots.len()].value.get() = Some(value) };
                return Ok(());
            }
        }
    }

    pub fn pop(&self) -> Option<T> {
        loop {
            let head = self.head.load(Ordering::Acquire);
            let tail = self.tail.load(Ordering::Acquire);
            if head >= tail {
                return None;
            }
            if self.head.compare_exchange(head, head + 1, Ordering::AcqRel, Ordering::Relaxed).is_ok() {
                return unsafe { (*self.slots[head % self.slots.len()].value.get()).take() };
            }
        }
    }
}
