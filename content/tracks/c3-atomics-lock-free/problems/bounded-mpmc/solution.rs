use std::cell::UnsafeCell;
use std::mem::MaybeUninit;
use std::sync::atomic::{AtomicUsize, Ordering};

/// Keeps `head` and `tail` on different cache lines so producers and consumers do not invalidate each other.
#[repr(align(64))]
struct Padded<T>(T);

struct Slot<T> {
    seq: AtomicUsize,
    value: UnsafeCell<MaybeUninit<T>>,
}

/// Bounded multi-producer multi-consumer FIFO (Dmitry Vyukov's design): every slot carries a sequence number that
/// says whose turn it is. For slot `i` at lap `k`: `seq == i + k*cap` means ready for a push, `seq == i + k*cap + 1`
/// means ready for a pop.
pub struct Queue<T> {
    slots: Box<[Slot<T>]>,
    mask: usize,
    head: Padded<AtomicUsize>,
    tail: Padded<AtomicUsize>,
}

unsafe impl<T: Send> Send for Queue<T> {}
unsafe impl<T: Send> Sync for Queue<T> {}

impl<T> Queue<T> {
    /// Capacity is rounded up to a power of two, at least 2.
    pub fn new(capacity: usize) -> Self {
        let cap = capacity.max(2).next_power_of_two();
        let slots = (0..cap).map(|i| Slot { seq: AtomicUsize::new(i), value: UnsafeCell::new(MaybeUninit::uninit()) }).collect();
        Queue { slots, mask: cap - 1, head: Padded(AtomicUsize::new(0)), tail: Padded(AtomicUsize::new(0)) }
    }

    pub fn capacity(&self) -> usize {
        self.mask + 1
    }

    pub fn push(&self, value: T) -> Result<(), T> {
        let mut pos = self.tail.0.load(Ordering::Relaxed);
        loop {
            let slot = &self.slots[pos & self.mask];
            let seq = slot.seq.load(Ordering::Acquire);
            let diff = seq as isize - pos as isize;
            if diff == 0 {
                match self.tail.0.compare_exchange_weak(pos, pos.wrapping_add(1), Ordering::Relaxed, Ordering::Relaxed) {
                    Ok(_) => {
                        unsafe { (*slot.value.get()).write(value) };
                        slot.seq.store(pos.wrapping_add(1), Ordering::Release);
                        return Ok(());
                    }
                    Err(now) => pos = now,
                }
            } else if diff < 0 {
                return Err(value); // the slot still holds last lap's value: full
            } else {
                pos = self.tail.0.load(Ordering::Relaxed); // someone else claimed it: reload
            }
        }
    }

    pub fn pop(&self) -> Option<T> {
        let mut pos = self.head.0.load(Ordering::Relaxed);
        loop {
            let slot = &self.slots[pos & self.mask];
            let seq = slot.seq.load(Ordering::Acquire);
            let diff = seq as isize - pos.wrapping_add(1) as isize;
            if diff == 0 {
                match self.head.0.compare_exchange_weak(pos, pos.wrapping_add(1), Ordering::Relaxed, Ordering::Relaxed) {
                    Ok(_) => {
                        let value = unsafe { (*slot.value.get()).assume_init_read() };
                        slot.seq.store(pos.wrapping_add(self.mask + 1), Ordering::Release);
                        return Some(value);
                    }
                    Err(now) => pos = now,
                }
            } else if diff < 0 {
                return None; // not written yet: empty
            } else {
                pos = self.head.0.load(Ordering::Relaxed);
            }
        }
    }
}

impl<T> Drop for Queue<T> {
    fn drop(&mut self) {
        while self.pop().is_some() {}
    }
}
