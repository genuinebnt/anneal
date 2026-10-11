//! A ring buffer: a queue of fixed capacity stored in one array that wraps around. A classic to test against a model: a `VecDeque` does
//! the same job in a way nobody doubts, so any sequence of operations must give both the same answers.

/// A queue holding at most `capacity` values, first in first out.
pub struct RingBuffer<T> {
    // @begin r-05
    slots: Vec<Option<T>>,
    head: usize,
    len: usize,
    //~ _ring: std::marker::PhantomData<T>,
    //~ // TODO(r-05): your fields: the slots, where the oldest value is, how many values there are
    // @end
}

impl<T> RingBuffer<T> {
    /// An empty buffer for `capacity` values. A capacity of 0 is allowed: every push is refused.
    pub fn new(capacity: usize) -> RingBuffer<T> {
        // @begin r-05
        RingBuffer { slots: (0..capacity).map(|_| None).collect(), head: 0, len: 0 }
        //~ todo!("r-05: capacity empty slots; the oldest value is at slot 0")
        // @end
    }

    /// The number of slots, which is the most values the buffer can hold.
    pub fn capacity(&self) -> usize {
        // @begin r-05
        self.slots.len()
        //~ todo!("r-05: the number of slots")
        // @end
    }

    /// The number of values held.
    pub fn len(&self) -> usize {
        // @begin r-05
        self.len
        //~ todo!("r-05: the number of values held")
        // @end
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    pub fn is_full(&self) -> bool {
        self.len() == self.capacity()
    }

    /// Adds a value at the back. A full buffer refuses it and gives it back: `Err(value)`.
    pub fn push(&mut self, value: T) -> Result<(), T> {
        // @begin r-05
        if self.is_full() {
            return Err(value);
        }
        let tail = (self.head + self.len) % self.slots.len();
        self.slots[tail] = Some(value);
        self.len += 1;
        Ok(())
        //~ todo!("r-05: Err(value) when full; else the value goes to slot (head + len) % capacity")
        // @end
    }

    /// Adds a value at the back; a full buffer first drops its oldest value, which is returned. A buffer of capacity 0 returns the value itself.
    pub fn push_overwriting(&mut self, value: T) -> Option<T> {
        // @begin r-05
        if self.capacity() == 0 {
            return Some(value);
        }
        let evicted = if self.is_full() { self.pop() } else { None };
        let _ = self.push(value);
        evicted
        //~ todo!("r-05: when full, pop the oldest first; then push; return what was dropped (capacity 0: the value itself)")
        // @end
    }

    /// Removes and returns the oldest value.
    pub fn pop(&mut self) -> Option<T> {
        // @begin r-05
        if self.len == 0 {
            return None;
        }
        let value = self.slots[self.head].take();
        self.head = (self.head + 1) % self.slots.len();
        self.len -= 1;
        value
        //~ todo!("r-05: take the value out of slot `head` (Option::take), move head on by one, wrapping")
        // @end
    }

    /// The oldest value, without removing it.
    pub fn front(&self) -> Option<&T> {
        // @begin r-05
        if self.len == 0 {
            return None;
        }
        self.slots[self.head].as_ref()
        //~ todo!("r-05: a reference to the value in slot `head`")
        // @end
    }

    /// The newest value, without removing it.
    pub fn back(&self) -> Option<&T> {
        // @begin r-05
        if self.len == 0 {
            return None;
        }
        self.slots[(self.head + self.len - 1) % self.slots.len()].as_ref()
        //~ todo!("r-05: a reference to the value in slot (head + len - 1) % capacity")
        // @end
    }

    /// The values from the oldest to the newest.
    pub fn iter(&self) -> Box<dyn Iterator<Item = &T> + '_> {
        // @begin r-05
        Box::new((0..self.len).filter_map(move |i| self.slots[(self.head + i) % self.slots.len()].as_ref()))
        //~ todo!("r-05: for i in 0..len, the value in slot (head + i) % capacity (a boxed iterator, no Vec)")
        // @end
    }

    /// Removes everything.
    pub fn clear(&mut self) {
        // @begin r-05
        for slot in &mut self.slots {
            *slot = None;
        }
        self.head = 0;
        self.len = 0;
        //~ todo!("r-05: empty every slot (so the values are dropped now) and start again at 0")
        // @end
    }
}
