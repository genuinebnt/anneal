//! A multi-producer multi-consumer queue: `put` never blocks, `get` waits until there is something to get.
//!
//! The shape behind a request queue and every thread pool.

// TODO(1b-01): your imports go here.

/// A queue shared by many threads: any of them may `put`, any may `get`.
pub struct Channel<T> {
    // TODO(1b-01): the fields are yours. A `Channel` is shared through `&self`, so what changes must be protected.
    _elements: std::marker::PhantomData<T>, // delete this line once one of your fields mentions T
}

impl<T> Channel<T> {
    /// An empty channel.
    pub fn new() -> Channel<T> {
        todo!("1b-01: an empty channel")
    }

    /// How many elements are waiting.
    pub fn len(&self) -> usize {
        todo!("1b-01: the number of elements waiting")
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// Adds an element at the back and wakes a getter that may be waiting. Never blocks.
    pub fn put(&self, element: T) {
        todo!("1b-01: add the element at the back, and make sure a sleeping getter notices")
    }

    /// Takes the element at the front, waiting for one if the channel is empty. Several threads may wait at once; each
    /// element goes to exactly one of them.
    pub fn get(&self) -> T {
        todo!("1b-01: wait until there is an element, then take the front one")
    }
}

impl<T> Default for Channel<T> {
    fn default() -> Self {
        Self::new()
    }
}

/// The worker loop: elements are `Option<T>`, and `None` means stop.
///
/// Calls `f` on each `Some` element, in order, until it receives a `None`, which it consumes.
pub fn consume<T>(channel: &Channel<Option<T>>, mut f: impl FnMut(T)) {
    todo!("1b-01: get elements and call f on each Some, until a None arrives")
}
