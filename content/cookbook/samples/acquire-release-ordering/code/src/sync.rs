//! One place that picks std or loom. loom only sees operations on its own types.

#[cfg(loom)]
pub use loom::{
    cell::UnsafeCell,
    sync::Arc,
    sync::atomic::{AtomicBool, AtomicU32, AtomicU64, AtomicUsize, fence},
};

#[cfg(not(loom))]
pub use std::sync::{
    Arc,
    atomic::{AtomicBool, AtomicU32, AtomicU64, AtomicUsize, fence},
};

pub use std::sync::atomic::Ordering;

/// loom's `UnsafeCell` takes a closure, so that it can see where each access starts
/// and ends. This gives the std cell the same shape.
#[cfg(not(loom))]
#[derive(Debug, Default)]
pub struct UnsafeCell<T>(std::cell::UnsafeCell<T>);

#[cfg(not(loom))]
impl<T> UnsafeCell<T> {
    pub fn new(value: T) -> Self {
        Self(std::cell::UnsafeCell::new(value))
    }

    pub fn with<R>(&self, f: impl FnOnce(*const T) -> R) -> R {
        f(self.0.get())
    }

    pub fn with_mut<R>(&self, f: impl FnOnce(*mut T) -> R) -> R {
        f(self.0.get())
    }
}
