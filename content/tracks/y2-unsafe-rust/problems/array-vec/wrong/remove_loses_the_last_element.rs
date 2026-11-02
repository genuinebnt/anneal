use std::mem::{self, MaybeUninit};
use std::ops::{Deref, DerefMut};
use std::ptr;

/// A vector with room for `N` elements stored inline (no heap). Slots `buf[..len]` are initialised, the rest are not.
pub struct ArrayVec<T, const N: usize> {
    buf: [MaybeUninit<T>; N],
    len: usize,
}

impl<T, const N: usize> ArrayVec<T, N> {
    pub fn new() -> Self {
        // SAFETY: an array of `MaybeUninit` needs no initialisation.
        ArrayVec { buf: unsafe { MaybeUninit::<[MaybeUninit<T>; N]>::uninit().assume_init() }, len: 0 }
    }

    pub fn len(&self) -> usize {
        self.len
    }

    pub fn is_empty(&self) -> bool {
        self.len == 0
    }

    pub fn capacity(&self) -> usize {
        N
    }

    pub fn is_full(&self) -> bool {
        self.len == N
    }

    /// Returns the value back if the vector is full.
    pub fn try_push(&mut self, value: T) -> Result<(), T> {
        if self.len == N {
            return Err(value);
        }
        self.buf[self.len].write(value);
        self.len += 1;
        Ok(())
    }

    pub fn pop(&mut self) -> Option<T> {
        if self.len == 0 {
            return None;
        }
        self.len -= 1;
        // SAFETY: slot len (the old last) was initialised and is no longer counted, so it is read out once.
        Some(unsafe { self.buf[self.len].assume_init_read() })
    }

    /// Inserts at `index` (0..=len); returns the value if full. Panics if `index > len`.
    pub fn try_insert(&mut self, index: usize, value: T) -> Result<(), T> {
        assert!(index <= self.len, "index out of bounds");
        if self.len == N {
            return Err(value);
        }
        // SAFETY: there is a free slot at len; `copy` handles the overlap while shifting the tail right by one.
        unsafe {
            let p = self.buf.as_mut_ptr().add(index);
            ptr::copy(p, p.add(1), self.len - index);
            (*p).write(value);
        }
        self.len += 1;
        Ok(())
    }

    /// Removes and returns the element at `index`, keeping order. Panics if `index >= len`.
    pub fn remove(&mut self, index: usize) -> T {
        assert!(index < self.len, "index out of bounds");
        self.len -= 1;
        // SAFETY: index was live; move it out, then slide the tail over the hole.
        unsafe {
            let p = self.buf.as_mut_ptr().add(index);
            let out = (*p).assume_init_read();
            ptr::copy(p.add(1), p, (self.len - index).saturating_sub(1));
            out
        }
    }

    /// Removes the element at `index` by swapping the last element into its place (O(1), order not kept).
    pub fn swap_remove(&mut self, index: usize) -> T {
        assert!(index < self.len, "index out of bounds");
        let last = self.len - 1;
        self.as_mut_slice().swap(index, last);
        self.pop().unwrap()
    }

    pub fn as_slice(&self) -> &[T] {
        // SAFETY: the first len slots are initialised and MaybeUninit<T> has the layout of T.
        unsafe { std::slice::from_raw_parts(self.buf.as_ptr() as *const T, self.len) }
    }

    pub fn as_mut_slice(&mut self) -> &mut [T] {
        // SAFETY: as in `as_slice`, with exclusive access.
        unsafe { std::slice::from_raw_parts_mut(self.buf.as_mut_ptr() as *mut T, self.len) }
    }

    /// The elements as a real `[T; N]` if exactly `N` are present, else `self` back.
    pub fn into_array(self) -> Result<[T; N], Self> {
        if self.len != N {
            return Err(self);
        }
        // SAFETY: all N slots are initialised; we read them out as an array and forget `self` so they are not dropped twice.
        let arr = unsafe { ptr::read(self.buf.as_ptr() as *const [T; N]) };
        mem::forget(self);
        Ok(arr)
    }
}

impl<T, const N: usize> Deref for ArrayVec<T, N> {
    type Target = [T];
    fn deref(&self) -> &[T] {
        self.as_slice()
    }
}

impl<T, const N: usize> DerefMut for ArrayVec<T, N> {
    fn deref_mut(&mut self) -> &mut [T] {
        self.as_mut_slice()
    }
}

impl<T, const N: usize> Drop for ArrayVec<T, N> {
    fn drop(&mut self) {
        // SAFETY: exactly the first len slots are live, and each is dropped once here.
        unsafe { ptr::drop_in_place(self.as_mut_slice()) };
    }
}
