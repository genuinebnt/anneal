use std::cell::UnsafeCell;
use std::mem::MaybeUninit;
use std::sync::atomic::{AtomicUsize, Ordering};

struct Slot<T> {
    /// Whose turn this slot is. For slot `i`: `i + lap * capacity` = free for the push of that lap,
    /// `i + lap * capacity + 1` = full, ready for the pop of that lap.
    seq: AtomicUsize,
    value: UnsafeCell<MaybeUninit<T>>,
}

pub struct Queue<T> {
    slots: Box<[Slot<T>]>,
    mask: usize,
    head: AtomicUsize, // next position to pop
    tail: AtomicUsize, // next position to push
}

// Given loose so the tests compile: what must `T` be for a queue shared by many threads?
unsafe impl<T> Send for Queue<T> {}
unsafe impl<T> Sync for Queue<T> {}

impl<T> Queue<T> {
    /// Capacity is rounded up to a power of two, and is at least 2.
    pub fn new(capacity: usize) -> Self {
        todo!()
    }

    pub fn capacity(&self) -> usize {
        todo!()
    }

    /// Gives the value back when the queue is full. Never blocks.
    pub fn push(&self, value: T) -> Result<(), T> {
        todo!()
    }

    /// `None` when empty. Never blocks.
    pub fn pop(&self) -> Option<T> {
        todo!()
    }
}

// TODO: values still queued when the queue is dropped must be dropped exactly once.
