use std::cell::UnsafeCell;
use std::mem::MaybeUninit;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;

struct Shared<T> {
    buf: Box<[UnsafeCell<MaybeUninit<T>>]>,
    head: AtomicUsize, // next slot to pop
    tail: AtomicUsize, // next slot to push
}

// Given loose so the tests compile: what does `Shared<T>` need from `T` to be shared by two threads?
unsafe impl<T> Sync for Shared<T> {}

pub struct Producer<T> {
    shared: Arc<Shared<T>>,
}

pub struct Consumer<T> {
    shared: Arc<Shared<T>>,
}

/// Panics if `capacity` is 0. Exactly `capacity` items fit.
pub fn channel<T>(capacity: usize) -> (Producer<T>, Consumer<T>) {
    todo!()
}

impl<T> Producer<T> {
    /// Gives `value` back when the buffer is full. Never blocks.
    pub fn push(&mut self, value: T) -> Result<(), T> {
        todo!()
    }

    /// True once the consumer has been dropped.
    pub fn is_abandoned(&self) -> bool {
        todo!()
    }
}

impl<T> Consumer<T> {
    /// `None` when empty. Never blocks.
    pub fn pop(&mut self) -> Option<T> {
        todo!()
    }

    /// True once the producer has been dropped (items may still be waiting).
    pub fn is_abandoned(&self) -> bool {
        todo!()
    }
}

// TODO: items still in the buffer when everything is dropped must be dropped exactly once.
