/// A handle to a slot: the slot number plus the generation it had when it was handed out.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct Key {
    pub idx: u32,
    pub gen: u32,
}

/// Values in a `Vec`, addressed by `Key`. Freed slots are reused (last freed, first reused); each reuse bumps the
/// slot's generation, so a key to a removed value can never reach the value that took its place.
pub struct Slab<T> {
    // TODO: entries with a generation each, a free list threaded through the empty ones, and a live count
    _marker: std::marker::PhantomData<T>,
}

impl<T> Slab<T> {
    pub fn new() -> Self {
        todo!()
    }

    pub fn insert(&mut self, value: T) -> Key {
        todo!()
    }

    /// `None` for a key whose value was removed, even if its slot has been reused since.
    pub fn get(&self, key: Key) -> Option<&T> {
        todo!()
    }

    pub fn get_mut(&mut self, key: Key) -> Option<&mut T> {
        todo!()
    }

    pub fn remove(&mut self, key: Key) -> Option<T> {
        todo!()
    }

    pub fn len(&self) -> usize {
        todo!()
    }

    pub fn is_empty(&self) -> bool {
        todo!()
    }

    /// Slots ever allocated (full or free). Reuse keeps this from growing under churn.
    pub fn slots(&self) -> usize {
        todo!()
    }

    /// Live entries in slot order.
    pub fn iter(&self) -> impl Iterator<Item = (Key, &T)> {
        std::iter::empty()
    }
}
