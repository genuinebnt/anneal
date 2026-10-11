//! A queue that returns the most urgent item first, and the oldest among equals.

pub struct RequestQueue<T> {
    // @begin 1b-c3
    /// (priority, arrival number, item); the arrival number makes the order of equals stable.
    items: std::collections::BinaryHeap<(u8, std::cmp::Reverse<u64>, Entry<T>)>,
    arrivals: u64,
    //~ _q: std::marker::PhantomData<T>,
    // @end
}

// @begin 1b-c3
/// Wraps the payload so that it needs no ordering of its own.
struct Entry<T>(T);

impl<T> PartialEq for Entry<T> {
    fn eq(&self, _: &Self) -> bool {
        true
    }
}
impl<T> Eq for Entry<T> {}
impl<T> PartialOrd for Entry<T> {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}
impl<T> Ord for Entry<T> {
    fn cmp(&self, _: &Self) -> std::cmp::Ordering {
        std::cmp::Ordering::Equal
    }
}
//~ // TODO(1b-c3): anything else your design needs
// @end

impl<T> RequestQueue<T> {
    /// An empty queue.
    pub fn new() -> RequestQueue<T> {
        // @begin 1b-c3
        RequestQueue { items: std::collections::BinaryHeap::new(), arrivals: 0 }
        //~ todo!("1b-c3: an empty queue")
        // @end
    }

    /// Adds `item` with `priority`; a larger priority is more urgent.
    pub fn push(&mut self, priority: u8, item: T) {
        // @begin 1b-c3
        self.items.push((priority, std::cmp::Reverse(self.arrivals), Entry(item)));
        self.arrivals += 1;
        //~ todo!("1b-c3: add the item with its priority and arrival")
        // @end
    }

    /// Removes and returns the most urgent item, the oldest among equals.
    pub fn pop(&mut self) -> Option<T> {
        // @begin 1b-c3
        self.items.pop().map(|(_, _, Entry(item))| item)
        //~ todo!("1b-c3: the most urgent, oldest item")
        // @end
    }

    /// The priority of the item `pop` would return; `None` when the queue is empty.
    pub fn peek_priority(&self) -> Option<u8> {
        // @begin 1b-c3
        self.items.peek().map(|(p, _, _)| *p)
        //~ todo!("1b-c3: the priority pop would return")
        // @end
    }

    /// The number of items waiting.
    pub fn len(&self) -> usize {
        // @begin 1b-c3
        self.items.len()
        //~ todo!("1b-c3: how many items wait")
        // @end
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
