//! Tiny bump arena over an inline buffer: allocation is a pointer bump and all
//! memory is released at once (`reset`). Values placed here are never dropped.
use core::cell::{Cell, UnsafeCell};
use core::mem::{MaybeUninit, align_of, size_of};

pub struct Bump<const N: usize> {
    // The struct itself might not be aligned for T, so we align the *address*.
    buf: UnsafeCell<[MaybeUninit<u8>; N]>,
    used: Cell<usize>,
}

impl<const N: usize> Bump<N> {
    pub const fn new() -> Self {
        Self {
            buf: UnsafeCell::new([MaybeUninit::uninit(); N]),
            used: Cell::new(0),
        }
    }
    pub fn used(&self) -> usize {
        self.used.get()
    }

    /// Allocate and move `v` in. `&self` + `Cell` lets many live `&mut T`s coexist:
    /// each points at a disjoint region.
    #[allow(clippy::mut_from_ref)]
    pub fn alloc<T>(&self, v: T) -> Option<&mut T> {
        let base = self.buf.get() as *mut u8;
        let start = base as usize + self.used.get();
        let aligned = start.next_multiple_of(align_of::<T>());
        let offset = aligned - base as usize;
        let end = offset.checked_add(size_of::<T>())?;
        if end > N {
            return None;
        }
        self.used.set(end);
        // SAFETY: [offset, end) is in bounds, aligned, and never handed out again
        // until `reset(&mut self)` proves no references remain.
        unsafe {
            let p = base.add(offset) as *mut T;
            p.write(v);
            Some(&mut *p)
        }
    }
    /// Needs `&mut self`: the borrow checker guarantees no outstanding refs.
    pub fn reset(&mut self) {
        self.used.set(0);
    }
}
impl<const N: usize> Default for Bump<N> {
    fn default() -> Self {
        Self::new()
    }
}
