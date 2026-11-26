//! Fixed-capacity stack: storage lives inline (`[MaybeUninit<T>; N]`), so the
//! whole struct can sit on the stack or in a `static`. No heap, no panics on push.
use core::mem::MaybeUninit;

pub struct Stack<T, const N: usize> {
    buf: [MaybeUninit<T>; N],
    len: usize,
}

impl<T, const N: usize> Stack<T, N> {
    pub const fn new() -> Self {
        Self {
            buf: [const { MaybeUninit::uninit() }; N],
            len: 0,
        }
    }
    pub const fn len(&self) -> usize {
        self.len
    }
    pub const fn is_empty(&self) -> bool {
        self.len == 0
    }
    pub const fn capacity(&self) -> usize {
        N
    }
    /// Hands the value back on overflow (the `heapless` convention).
    pub fn push(&mut self, v: T) -> Result<(), T> {
        if self.len == N {
            return Err(v);
        }
        self.buf[self.len].write(v);
        self.len += 1;
        Ok(())
    }
    pub fn pop(&mut self) -> Option<T> {
        if self.len == 0 {
            return None;
        }
        self.len -= 1;
        // SAFETY: slot `len` was initialised and is now logically removed.
        Some(unsafe { self.buf[self.len].assume_init_read() })
    }
    pub fn as_slice(&self) -> &[T] {
        // SAFETY: the first `len` slots are initialised.
        unsafe { core::slice::from_raw_parts(self.buf.as_ptr().cast::<T>(), self.len) }
    }
    #[cfg(feature = "alloc")]
    pub fn to_vec(&self) -> alloc::vec::Vec<T>
    where
        T: Clone,
    {
        self.as_slice().to_vec()
    }
}
impl<T, const N: usize> Default for Stack<T, N> {
    fn default() -> Self {
        Self::new()
    }
}
impl<T, const N: usize> Drop for Stack<T, N> {
    fn drop(&mut self) {
        while self.pop().is_some() {}
    }
}
