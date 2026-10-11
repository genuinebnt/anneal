//! A reader-writer latch that owns what it protects: lock and unlock are one guard.

use std::sync::{RwLockReadGuard, RwLockWriteGuard};

// @begin 1b-05
use std::sync::{PoisonError, RwLock};
//~ // TODO(1b-05): your imports go here.
// @end

/// Many readers or one writer.
pub struct ReaderWriterLatch<T> {
    // @begin 1b-05
    inner: RwLock<T>,
    //~ // TODO(1b-05): the field is yours (the guards below are std's, so wrapping a `std::sync::RwLock` is the natural design).
    //~ _value: std::marker::PhantomData<T>, // delete this line once a field mentions T
    // @end
}

impl<T> ReaderWriterLatch<T> {
    /// A latch protecting `value`.
    pub fn new(value: T) -> ReaderWriterLatch<T> {
        // @begin 1b-05
        ReaderWriterLatch { inner: RwLock::new(value) }
        //~ todo!("1b-05: a latch around the value")
        // @end
    }

    /// Takes a read latch, released when the guard drops. Any number of readers may hold it together, and none while a writer does.
    ///
    /// A latch whose holder panicked still works.
    pub fn read(&self) -> RwLockReadGuard<'_, T> {
        // @begin 1b-05
        self.inner.read().unwrap_or_else(PoisonError::into_inner)
        //~ todo!("1b-05: take the read latch; a poisoned lock still gives its guard (see the notes)")
        // @end
    }

    /// Takes the write latch, released when the guard drops. One writer at a time, and no readers meanwhile.
    pub fn write(&self) -> RwLockWriteGuard<'_, T> {
        // @begin 1b-05
        self.inner.write().unwrap_or_else(PoisonError::into_inner)
        //~ todo!("1b-05: take the write latch; a poisoned lock still gives its guard")
        // @end
    }
}
