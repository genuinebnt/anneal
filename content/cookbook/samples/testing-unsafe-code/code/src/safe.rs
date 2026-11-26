//! The same append-only list with no `unsafe` at all, built on `std::cell::OnceCell`. It is the baseline for the benchmark,
//! and a second oracle: two implementations that must agree.

use std::cell::{Cell, OnceCell};

pub struct SafeShelf<T, const N: usize> {
    slots: [OnceCell<T>; N],
    len: Cell<usize>,
}

impl<T, const N: usize> SafeShelf<T, N> {
    pub fn new() -> Self {
        SafeShelf { slots: [const { OnceCell::new() }; N], len: Cell::new(0) }
    }

    pub fn len(&self) -> usize {
        self.len.get()
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    pub fn push(&self, value: T) -> Result<&T, T> {
        let index = self.len.get();
        let Some(slot) = self.slots.get(index) else { return Err(value) };
        self.len.set(index + 1);
        // The slot is empty, so the closure runs. If that reasoning were wrong we would get a wrong value, not UB.
        Ok(slot.get_or_init(move || value))
    }

    pub fn get(&self, index: usize) -> Option<&T> {
        self.slots.get(index)?.get()
    }

    pub fn clear(&mut self) {
        while self.len.get() > 0 {
            let last = self.len.get() - 1;
            self.len.set(last);
            drop(self.slots[last].take());
        }
    }
}

impl<T, const N: usize> Default for SafeShelf<T, N> {
    fn default() -> Self {
        Self::new()
    }
}
