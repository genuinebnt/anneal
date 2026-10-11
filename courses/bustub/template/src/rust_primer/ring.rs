//! A ring buffer: a queue of fixed capacity stored in one array that wraps around. A classic to test against a model: a `VecDeque` does
//! the same job in a way nobody doubts, so any sequence of operations must give both the same answers.

/// A queue holding at most `capacity` values, first in first out.
pub struct RingBuffer<T> {
    _ring: std::marker::PhantomData<T>,
    // TODO(r-05): your fields: the slots, where the oldest value is, how many values there are
}

impl<T> RingBuffer<T> {
    /// An empty buffer for `capacity` values. A capacity of 0 is allowed: every push is refused.
    pub fn new(capacity: usize) -> RingBuffer<T> {
        todo!("r-05: capacity empty slots; the oldest value is at slot 0")
    }

    /// The number of slots, which is the most values the buffer can hold.
    pub fn capacity(&self) -> usize {
        todo!("r-05: the number of slots")
    }

    /// The number of values held.
    pub fn len(&self) -> usize {
        todo!("r-05: the number of values held")
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    pub fn is_full(&self) -> bool {
        self.len() == self.capacity()
    }

    /// Adds a value at the back. A full buffer refuses it and gives it back: `Err(value)`.
    pub fn push(&mut self, value: T) -> Result<(), T> {
        todo!("r-05: Err(value) when full; else the value goes to slot (head + len) % capacity")
    }

    /// Adds a value at the back; a full buffer first drops its oldest value, which is returned. A buffer of capacity 0 returns the value itself.
    pub fn push_overwriting(&mut self, value: T) -> Option<T> {
        todo!("r-05: when full, pop the oldest first; then push; return what was dropped (capacity 0: the value itself)")
    }

    /// Removes and returns the oldest value.
    pub fn pop(&mut self) -> Option<T> {
        todo!("r-05: take the value out of slot `head` (Option::take), move head on by one, wrapping")
    }

    /// The oldest value, without removing it.
    pub fn front(&self) -> Option<&T> {
        todo!("r-05: a reference to the value in slot `head`")
    }

    /// The newest value, without removing it.
    pub fn back(&self) -> Option<&T> {
        todo!("r-05: a reference to the value in slot (head + len - 1) % capacity")
    }

    /// The values from the oldest to the newest.
    pub fn iter(&self) -> Box<dyn Iterator<Item = &T> + '_> {
        todo!("r-05: for i in 0..len, the value in slot (head + i) % capacity (a boxed iterator, no Vec)")
    }

    /// Removes everything.
    pub fn clear(&mut self) {
        todo!("r-05: empty every slot (so the values are dropped now) and start again at 0")
    }
}
