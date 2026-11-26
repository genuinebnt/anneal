//! Ring (circular) buffer: FIFO queue of fixed capacity.
use core::mem::MaybeUninit;

pub struct Ring<T, const N: usize> {
    buf: [MaybeUninit<T>; N],
    head: usize, // index of the oldest element
    len: usize,
}

impl<T, const N: usize> Ring<T, N> {
    pub const fn new() -> Self {
        Self {
            buf: [const { MaybeUninit::uninit() }; N],
            head: 0,
            len: 0,
        }
    }
    pub const fn len(&self) -> usize {
        self.len
    }
    pub const fn is_empty(&self) -> bool {
        self.len == 0
    }
    pub const fn is_full(&self) -> bool {
        self.len == N
    }
    /// Reject when full (hands value back).
    pub fn push_back(&mut self, v: T) -> Result<(), T> {
        if self.len == N {
            return Err(v);
        }
        let tail = (self.head + self.len) % N; // wraps around
        self.buf[tail].write(v);
        self.len += 1;
        Ok(())
    }
    /// Overwrite the oldest element when full (typical for logging/telemetry).
    pub fn push_overwrite(&mut self, v: T) -> Option<T> {
        let old = if self.len == N {
            self.pop_front()
        } else {
            None
        };
        let _ = self.push_back(v);
        old
    }
    pub fn pop_front(&mut self) -> Option<T> {
        if self.len == 0 {
            return None;
        }
        // SAFETY: `head` is initialised while len > 0.
        let v = unsafe { self.buf[self.head].assume_init_read() };
        self.head = (self.head + 1) % N;
        self.len -= 1;
        Some(v)
    }
    pub fn get(&self, i: usize) -> Option<&T> {
        if i >= self.len {
            return None;
        }
        // SAFETY: logical index i < len maps to an initialised slot.
        Some(unsafe { self.buf[(self.head + i) % N].assume_init_ref() })
    }
    pub fn iter(&self) -> impl Iterator<Item = &T> + '_ {
        (0..self.len).filter_map(|i| self.get(i))
    }
}
impl<T, const N: usize> Default for Ring<T, N> {
    fn default() -> Self {
        Self::new()
    }
}
impl<T, const N: usize> Drop for Ring<T, N> {
    fn drop(&mut self) {
        while self.pop_front().is_some() {}
    }
}
