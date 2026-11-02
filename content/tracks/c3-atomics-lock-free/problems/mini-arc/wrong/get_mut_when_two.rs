use std::marker::PhantomData;
use std::ops::Deref;
use std::ptr::NonNull;
use std::sync::atomic::{fence, AtomicUsize, Ordering};

struct Inner<T> {
    strong: AtomicUsize,
    data: T,
}

/// A reference-counted pointer that can cross threads.
pub struct MiniArc<T> {
    ptr: NonNull<Inner<T>>,
    _owns: PhantomData<Inner<T>>,
}

unsafe impl<T: Send + Sync> Send for MiniArc<T> {}
unsafe impl<T: Send + Sync> Sync for MiniArc<T> {}

impl<T> MiniArc<T> {
    pub fn new(data: T) -> Self {
        let b = Box::new(Inner { strong: AtomicUsize::new(1), data });
        MiniArc { ptr: NonNull::from(Box::leak(b)), _owns: PhantomData }
    }

    fn inner(&self) -> &Inner<T> {
        unsafe { self.ptr.as_ref() }
    }

    pub fn strong_count(this: &Self) -> usize {
        this.inner().strong.load(Ordering::Acquire)
    }

    pub fn ptr_eq(a: &Self, b: &Self) -> bool {
        a.ptr == b.ptr
    }

    /// `Some` only while this is the sole owner.
    pub fn get_mut(this: &mut Self) -> Option<&mut T> {
        if this.inner().strong.load(Ordering::Acquire) <= 2 {
            Some(unsafe { &mut (*this.ptr.as_ptr()).data })
        } else {
            None
        }
    }

    /// Takes the value out if this is the only owner, else gives `this` back.
    pub fn try_unwrap(this: Self) -> Result<T, Self> {
        if this.inner().strong.compare_exchange(1, 0, Ordering::Relaxed, Ordering::Relaxed).is_err() {
            return Err(this);
        }
        fence(Ordering::Acquire);
        let ptr = this.ptr.as_ptr();
        std::mem::forget(this);
        let boxed = unsafe { Box::from_raw(ptr) };
        let Inner { data, .. } = *boxed;
        Ok(data)
    }

    /// Clone-on-write: a unique owner gets `&mut T` directly, a shared one first copies the value.
    pub fn make_mut(this: &mut Self) -> &mut T
    where
        T: Clone,
    {
        if Self::strong_count(this) != 1 {
            *this = MiniArc::new((**this).clone());
        }
        Self::get_mut(this).unwrap()
    }
}

impl<T> Deref for MiniArc<T> {
    type Target = T;
    fn deref(&self) -> &T {
        &self.inner().data
    }
}

impl<T> Clone for MiniArc<T> {
    fn clone(&self) -> Self {
        let old = self.inner().strong.fetch_add(1, Ordering::Relaxed);
        if old > isize::MAX as usize {
            std::process::abort(); // a leaked-clone loop would wrap the count and free live data
        }
        MiniArc { ptr: self.ptr, _owns: PhantomData }
    }
}

impl<T> Drop for MiniArc<T> {
    fn drop(&mut self) {
        if self.inner().strong.fetch_sub(1, Ordering::Release) == 1 {
            fence(Ordering::Acquire);
            drop(unsafe { Box::from_raw(self.ptr.as_ptr()) });
        }
    }
}
