//! A queue that returns the most urgent item first, and the oldest among equals.

pub struct RequestQueue<T> {
    _q: std::marker::PhantomData<T>,
}

// TODO(1b-c3): anything else your design needs

impl<T> RequestQueue<T> {
    /// An empty queue.
    pub fn new() -> RequestQueue<T> {
        todo!("1b-c3: an empty queue")
    }

    /// Adds `item` with `priority`; a larger priority is more urgent.
    pub fn push(&mut self, priority: u8, item: T) {
        todo!("1b-c3: add the item with its priority and arrival")
    }

    /// Removes and returns the most urgent item, the oldest among equals.
    pub fn pop(&mut self) -> Option<T> {
        todo!("1b-c3: the most urgent, oldest item")
    }

    /// The priority of the item `pop` would return; `None` when the queue is empty.
    pub fn peek_priority(&self) -> Option<u8> {
        todo!("1b-c3: the priority pop would return")
    }

    /// The number of items waiting.
    pub fn len(&self) -> usize {
        todo!("1b-c3: how many items wait")
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
}

impl<T> Default for RequestQueue<T> {
    fn default() -> Self {
        RequestQueue::new()
    }
}
