use std::sync::atomic::{AtomicU32, AtomicU64, Ordering};

pub const NIL: u32 = u32::MAX;

/// Version tag in the high 32 bits, slot index in the low 32.
pub fn pack(tag: u32, index: u32) -> u64 {
    todo!()
}

pub fn unpack(word: u64) -> (u32, u32) {
    todo!()
}

/// A lock-free LIFO of `u64` values over a fixed pool of slots. Two lists share the pool: the stack (`top`) and the
/// free slots (`free`). `next[i]` links slot `i` to the slot below it in whichever list it is on.
pub struct Stack {
    values: Box<[AtomicU64]>,
    next: Box<[AtomicU32]>,
    top: AtomicU64,
    free: AtomicU64,
}

impl Stack {
    /// A stack with room for `capacity` values; every slot starts on the free list.
    pub fn new(capacity: usize) -> Self {
        todo!()
    }

    pub fn capacity(&self) -> usize {
        todo!()
    }

    /// Gives the value back when every slot is in use. Lock-free: never waits on another thread.
    pub fn push(&self, value: u64) -> Result<(), u64> {
        todo!()
    }

    pub fn pop(&self) -> Option<u64> {
        todo!()
    }
}
