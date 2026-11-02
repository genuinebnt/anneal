use std::sync::atomic::{AtomicU32, AtomicU64, Ordering};

pub const NIL: u32 = u32::MAX;

pub fn pack(tag: u32, index: u32) -> u64 {
    ((tag as u64) << 32) | index as u64
}

pub fn unpack(word: u64) -> (u32, u32) {
    ((word >> 32) as u32, word as u32)
}

/// A lock-free LIFO of `u64` values over a fixed pool of slots. Two Treiber lists share the pool: the stack itself
/// and a free list. Each head packs a 32-bit version tag next to the slot index, so a compare-exchange fails if the
/// head was popped and pushed back in between (the ABA problem).
pub struct Stack {
    values: Box<[AtomicU64]>,
    next: Box<[AtomicU32]>,
    top: AtomicU64,
    free: AtomicU64,
}

fn pop_slot(list: &AtomicU64, next: &[AtomicU32]) -> Option<u32> {
    let mut cur = list.load(Ordering::Acquire);
    loop {
        let (tag, index) = unpack(cur);
        if index == NIL {
            return None;
        }
        let after = next[index as usize].load(Ordering::Relaxed);
        match list.compare_exchange_weak(cur, pack(tag.wrapping_add(1), after), Ordering::Acquire, Ordering::Acquire) {
            Ok(_) => return Some(index),
            Err(seen) => cur = seen,
        }
    }
}

fn push_slot(list: &AtomicU64, next: &[AtomicU32], index: u32) {
    let mut cur = list.load(Ordering::Relaxed);
    loop {
        let (tag, head) = unpack(cur);
        next[index as usize].store(head, Ordering::Relaxed);
        match list.compare_exchange_weak(cur, pack(tag.wrapping_add(1), index), Ordering::Release, Ordering::Relaxed) {
            Ok(_) => return,
            Err(seen) => cur = seen,
        }
    }
}

impl Stack {
    /// Room for `capacity` values (at most `u32::MAX - 1`).
    pub fn new(capacity: usize) -> Self {
        assert!(capacity < NIL as usize);
        let values = (0..capacity).map(|_| AtomicU64::new(0)).collect();
        let next = (0..capacity).map(|i| AtomicU32::new(if i + 1 < capacity { i as u32 + 1 } else { NIL })).collect();
        let first_free = if capacity == 0 { NIL } else { 0 };
        Stack { values, next, top: AtomicU64::new(pack(0, NIL)), free: AtomicU64::new(pack(0, first_free)) }
    }

    pub fn capacity(&self) -> usize {
        self.values.len()
    }

    /// Gives the value back when every slot is in use.
    pub fn push(&self, value: u64) -> Result<(), u64> {
        let Some(slot) = pop_slot(&self.free, &self.next) else {
            return Ok(());
        };
        self.values[slot as usize].store(value, Ordering::Relaxed);
        push_slot(&self.top, &self.next, slot);
        Ok(())
    }

    pub fn pop(&self) -> Option<u64> {
        let slot = pop_slot(&self.top, &self.next)?;
        let value = self.values[slot as usize].load(Ordering::Relaxed);
        push_slot(&self.free, &self.next, slot);
        Some(value)
    }
}
