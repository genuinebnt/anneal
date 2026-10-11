//! A reader-writer latch that owns what it protects: lock and unlock are one guard.

use std::sync::{RwLockReadGuard, RwLockWriteGuard};

// TODO(1b-05): your imports go here.

/// Many readers or one writer.
pub struct ReaderWriterLatch<T> {
    // TODO(1b-05): the field is yours (the guards below are std's, so wrapping a `std::sync::RwLock` is the natural design).
    _value: std::marker::PhantomData<T>, // delete this line once a field mentions T
}

impl<T> ReaderWriterLatch<T> {
    /// A latch protecting `value`.
    pub fn new(value: T) -> ReaderWriterLatch<T> {
        todo!("1b-05: a latch around the value")
    }

    /// Takes a read latch, released when the guard drops. Any number of readers may hold it together, and none while a writer does.
    ///
    /// A latch whose holder panicked still works.
    pub fn read(&self) -> RwLockReadGuard<'_, T> {
        todo!("1b-05: take the read latch; a poisoned lock still gives its guard (see the notes)")
    }

    /// Takes the write latch, released when the guard drops. One writer at a time, and no readers meanwhile.
    pub fn write(&self) -> RwLockWriteGuard<'_, T> {
        todo!("1b-05: take the write latch; a poisoned lock still gives its guard")
    }
}
