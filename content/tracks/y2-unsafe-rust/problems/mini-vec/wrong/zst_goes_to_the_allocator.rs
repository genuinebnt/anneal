use std::alloc::{self, Layout};
use std::mem;
use std::ops::{Deref, DerefMut};
use std::ptr::{self, NonNull};

/// A growable array with its own allocation: pointer, capacity, length.
/// Invariant: `ptr[0..len]` are initialised, `ptr[len..cap]` are not, and `ptr` came from the global allocator
/// with `Layout::array::<T>(cap)` (or is dangling when `cap == 0` or `T` is zero-sized).
pub struct MyVec<T> {
    ptr: NonNull<T>,
    cap: usize,
    len: usize,
}

// SAFETY: MyVec owns its elements exactly like Vec<T> does.
unsafe impl<T: Send> Send for MyVec<T> {}
unsafe impl<T: Sync> Sync for MyVec<T> {}

impl<T> MyVec<T> {
    pub fn new() -> Self {
        // A zero-sized type never needs memory, so its capacity is "infinite".
        let cap = 0;
        MyVec { ptr: NonNull::dangling(), cap, len: 0 }
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

    fn grow(&mut self) {
        assert!(mem::size_of::<T>() != 0, "capacity overflow");
        let new_cap = if self.cap == 0 { 4 } else { self.cap * 2 };
        let new_layout = Layout::array::<T>(new_cap).expect("capacity overflow");
        let raw = if self.cap == 0 {
            // SAFETY: new_layout has a non-zero size because T is not zero-sized and new_cap > 0.
            unsafe { alloc::alloc(new_layout) }
        } else {
            let old_layout = Layout::array::<T>(self.cap).unwrap();
            // SAFETY: ptr was allocated by this allocator with old_layout, and new_layout.size() > 0.
            unsafe { alloc::realloc(self.ptr.as_ptr() as *mut u8, old_layout, new_layout.size()) }
        };
        self.ptr = match NonNull::new(raw as *mut T) {
            Some(p) => p,
            None => alloc::handle_alloc_error(new_layout),
        };
        self.cap = new_cap;
    }

    pub fn push(&mut self, value: T) {
        if self.len == self.cap {
            self.grow();
        }
        // SAFETY: len < cap, so the slot is inside the allocation and holds no live value to overwrite.
        unsafe { ptr::write(self.ptr.as_ptr().add(self.len), value) };
        self.len += 1;
    }

    pub fn pop(&mut self) -> Option<T> {
        if self.len == 0 {
            return None;
        }
        self.len -= 1;
        // SAFETY: the slot at the old len - 1 was initialised, and len no longer covers it, so it is read out once.
        Some(unsafe { ptr::read(self.ptr.as_ptr().add(self.len)) })
    }

    /// Inserts at `index`, shifting later elements right. Panics if `index > len`.
    pub fn insert(&mut self, index: usize, value: T) {
        assert!(index <= self.len, "index out of bounds");
        if self.len == self.cap {
            self.grow();
        }
        // SAFETY: after the grow check there is room for one more element; `copy` handles the overlap.
        unsafe {
            let p = self.ptr.as_ptr().add(index);
            ptr::copy(p, p.add(1), self.len - index);
            ptr::write(p, value);
        }
        self.len += 1;
    }

    /// Removes and returns the element at `index`, shifting later elements left. Panics if `index >= len`.
    pub fn remove(&mut self, index: usize) -> T {
        assert!(index < self.len, "index out of bounds");
        self.len -= 1;
        // SAFETY: index was in bounds; we move the value out, then slide the tail over the hole.
        unsafe {
            let p = self.ptr.as_ptr().add(index);
            let out = ptr::read(p);
            ptr::copy(p.add(1), p, self.len - index);
            out
        }
    }

    /// Drops the elements past `new_len`.
    pub fn truncate(&mut self, new_len: usize) {
        while self.len > new_len {
            self.len -= 1;
            // SAFETY: that slot was initialised and is no longer counted in len, so it is dropped once.
            unsafe { ptr::drop_in_place(self.ptr.as_ptr().add(self.len)) };
        }
    }

    pub fn clear(&mut self) {
        self.truncate(0);
    }
}

impl<T> Deref for MyVec<T> {
    type Target = [T];
    fn deref(&self) -> &[T] {
        // SAFETY: the first len slots are initialised; a dangling pointer is fine for an empty slice.
        unsafe { std::slice::from_raw_parts(self.ptr.as_ptr(), self.len) }
    }
}

impl<T> DerefMut for MyVec<T> {
    fn deref_mut(&mut self) -> &mut [T] {
        // SAFETY: as in `deref`, and `&mut self` guarantees exclusive access.
        unsafe { std::slice::from_raw_parts_mut(self.ptr.as_ptr(), self.len) }
    }
}

impl<T> Drop for MyVec<T> {
    fn drop(&mut self) {
        self.clear();
        if self.cap != 0 && mem::size_of::<T>() != 0 {
            // SAFETY: ptr was allocated with exactly this layout and is freed once.
            unsafe { alloc::dealloc(self.ptr.as_ptr() as *mut u8, Layout::array::<T>(self.cap).unwrap()) };
        }
    }
}
