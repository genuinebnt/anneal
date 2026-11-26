use std::alloc::{self, Layout};
use std::marker::PhantomData;
use std::ptr::{self, NonNull};

/// A growable array on a raw allocation. Zero-sized `T` isn't supported.
pub struct MiniVec<T> {
    ptr: NonNull<T>,
    cap: usize,
    len: usize,
    _owns: PhantomData<T>,
}

impl<T> MiniVec<T> {
    pub fn new() -> Self {
        assert!(
            std::mem::size_of::<T>() != 0,
            "zero-sized types aren't supported"
        );
        MiniVec {
            ptr: NonNull::dangling(),
            cap: 0,
            len: 0,
            _owns: PhantomData,
        }
    }

    pub fn len(&self) -> usize {
        self.len
    }

    pub fn is_empty(&self) -> bool {
        self.len == 0
    }

    pub fn capacity(&self) -> usize {
        self.cap
    }

    pub fn push(&mut self, value: T) {
        if self.len == self.cap {
            self.grow();
        }
        // SAFETY: len < cap, so the slot is inside the allocation and not yet initialised.
        unsafe { ptr::write(self.ptr.as_ptr().add(self.len), value) };
        self.len += 1;
    }

    pub fn pop(&mut self) -> Option<T> {
        if self.len == 0 {
            return None;
        }
        self.len -= 1;
        // SAFETY: the slot was initialised and is now past len, so it's read exactly once.
        Some(unsafe { ptr::read(self.ptr.as_ptr().add(self.len)) })
    }

    pub fn get(&self, i: usize) -> Option<&T> {
        // SAFETY: i < len, so the slot is initialised and inside the allocation.
        (i < self.len).then(|| unsafe { &*self.ptr.as_ptr().add(i) })
    }

    fn grow(&mut self) {
        let new_cap = if self.cap == 0 { 4 } else { self.cap * 2 };
        let new_layout = Layout::array::<T>(new_cap).expect("capacity overflow");
        let raw = if self.cap == 0 {
            // SAFETY: new_layout has a non-zero size because T isn't zero-sized.
            unsafe { alloc::alloc(new_layout) }
        } else {
            let old_layout = Layout::array::<T>(self.cap).expect("existing layout");
            // SAFETY: ptr was allocated with old_layout by this allocator.
            unsafe { alloc::realloc(self.ptr.as_ptr().cast(), old_layout, new_layout.size()) }
        };
        self.ptr =
            NonNull::new(raw.cast()).unwrap_or_else(|| alloc::handle_alloc_error(new_layout));
        self.cap = new_cap;
    }
}

impl<T> Default for MiniVec<T> {
    fn default() -> Self {
        Self::new()
    }
}

impl<T> Drop for MiniVec<T> {
    fn drop(&mut self) {
        while self.pop().is_some() {}
        if self.cap != 0 {
            // SAFETY: ptr was allocated with this layout and every element has been dropped.
            unsafe {
                alloc::dealloc(
                    self.ptr.as_ptr().cast(),
                    Layout::array::<T>(self.cap).expect("layout"),
                )
            };
        }
    }
}
