//! Shared state between threads: a blocking queue made of a `Mutex` and two `Condvar`s, a counter of atomics, and a sum computed by
//! threads that borrow the input.

use std::collections::VecDeque;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Condvar, Mutex};
use std::thread;

/// A queue shared by threads: at most `capacity` values wait in it. `push` blocks while it is full, `pop` blocks while it is empty,
/// and `close` ends it: after `close`, `push` is refused, and `pop` returns what is left and then `None` (instead of blocking forever).
pub struct BoundedQueue<T> {
    _queue: std::marker::PhantomData<T>,
    // TODO(r-04): your fields: a Mutex around the queued values and a closed flag, a Condvar for 'not empty' and one for 'not full', the capacity
}

// TODO(r-04): the state behind the mutex, of your own

impl<T> BoundedQueue<T> {
    /// A queue for `capacity` values (at least 1).
    pub fn new(capacity: usize) -> BoundedQueue<T> {
        todo!("r-04: an empty open queue")
    }

    /// Adds a value, waiting while the queue is full. `Err(value)` if the queue is (or becomes) closed.
    pub fn push(&self, value: T) -> Result<(), T> {
        todo!("r-04: closed gives Err(value); a full queue makes the caller wait; otherwise the value is added and a waiting pop is woken")
    }

    /// Takes the oldest value, waiting while the queue is empty. `None` once the queue is closed and empty.
    pub fn pop(&self) -> Option<T> {
        todo!("r-04: lock; in a loop: a value -> take it, wake a waiting push, Some; closed -> None; else wait on `not_empty`")
    }

    /// Takes the oldest value if there is one now; never waits.
    pub fn try_pop(&self) -> Option<T> {
        todo!("r-04: like pop without the waiting")
    }

    /// Ends the queue: every waiting `push` and `pop` wakes up and sees it.
    pub fn close(&self) {
        todo!("r-04: set the flag under the lock, then wake everybody (notify_all) on both condition variables")
    }

    /// The number of values waiting.
    pub fn len(&self) -> usize {
        todo!("r-04: the number of waiting values")
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
}

/// Hands out numbers 0, 1, 2, … to any number of threads, each number once: a counter shared by `&self`, with no lock.
pub struct IdGenerator {
    _ids: (),
    // TODO(r-04): your field: an atomic number
}

impl IdGenerator {
    /// A generator whose first number is 0.
    pub fn new() -> IdGenerator {
        todo!("r-04: start at 0")
    }

    /// The next number. Two threads never get the same one.
    pub fn next(&self) -> u64 {
        todo!("r-04: one atomic read-and-add (fetch_add), not a load followed by a store")
    }
}

impl Default for IdGenerator {
    fn default() -> Self {
        IdGenerator::new()
    }
}

/// The sum of `values`, computed by `threads` threads that each add one slice of the input (borrowed, not copied: scoped threads).
/// `threads == 0` counts as 1. The sum wraps on overflow, like `u64::wrapping_add`.
pub fn parallel_sum(values: &[u64], threads: usize) -> u64 {
    todo!("r-04: chunks of the input, one scoped thread each (thread::scope lets them borrow `values`), their sums added up")
}
