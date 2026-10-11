//! Shared state between threads: a blocking queue made of a `Mutex` and two `Condvar`s, a counter of atomics, and a sum computed by
//! threads that borrow the input.

use std::collections::VecDeque;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Condvar, Mutex};
use std::thread;

/// A queue shared by threads: at most `capacity` values wait in it. `push` blocks while it is full, `pop` blocks while it is empty,
/// and `close` ends it: after `close`, `push` is refused, and `pop` returns what is left and then `None` (instead of blocking forever).
pub struct BoundedQueue<T> {
    // @begin r-04
    state: Mutex<QueueState<T>>,
    not_empty: Condvar,
    not_full: Condvar,
    capacity: usize,
    //~ _queue: std::marker::PhantomData<T>,
    //~ // TODO(r-04): your fields: a Mutex around the queued values and a closed flag, a Condvar for 'not empty' and one for 'not full', the capacity
    // @end
}

// @begin r-04
struct QueueState<T> {
    items: VecDeque<T>,
    closed: bool,
}
//~ // TODO(r-04): the state behind the mutex, of your own
// @end

impl<T> BoundedQueue<T> {
    /// A queue for `capacity` values (at least 1).
    pub fn new(capacity: usize) -> BoundedQueue<T> {
        // @begin r-04
        BoundedQueue {
            state: Mutex::new(QueueState { items: VecDeque::new(), closed: false }),
            not_empty: Condvar::new(),
            not_full: Condvar::new(),
            capacity: capacity.max(1),
        }
        //~ todo!("r-04: an empty open queue")
        // @end
    }

    /// Adds a value, waiting while the queue is full. `Err(value)` if the queue is (or becomes) closed.
    pub fn push(&self, value: T) -> Result<(), T> {
        // @begin r-04
        let mut state = self.state.lock().unwrap();
        loop {
            if state.closed {
                return Err(value);
            }
            if state.items.len() < self.capacity {
                state.items.push_back(value);
                self.not_empty.notify_one();
                return Ok(());
            }
            state = self.not_full.wait(state).unwrap();
        }
        //~ todo!("r-04: closed gives Err(value); a full queue makes the caller wait; otherwise the value is added and a waiting pop is woken")
        // @end
    }

    /// Takes the oldest value, waiting while the queue is empty. `None` once the queue is closed and empty.
    pub fn pop(&self) -> Option<T> {
        // @begin r-04
        let mut state = self.state.lock().unwrap();
        loop {
            if let Some(value) = state.items.pop_front() {
                self.not_full.notify_one();
                return Some(value);
            }
            if state.closed {
                return None;
            }
            state = self.not_empty.wait(state).unwrap();
        }
        //~ todo!("r-04: lock; in a loop: a value -> take it, wake a waiting push, Some; closed -> None; else wait on `not_empty`")
        // @end
    }

    /// Takes the oldest value if there is one now; never waits.
    pub fn try_pop(&self) -> Option<T> {
        // @begin r-04
        let mut state = self.state.lock().unwrap();
        let value = state.items.pop_front();
        if value.is_some() {
            self.not_full.notify_one();
        }
        value
        //~ todo!("r-04: like pop without the waiting")
        // @end
    }

    /// Ends the queue: every waiting `push` and `pop` wakes up and sees it.
    pub fn close(&self) {
        // @begin r-04
        self.state.lock().unwrap().closed = true;
        self.not_empty.notify_all();
        self.not_full.notify_all();
        //~ todo!("r-04: set the flag under the lock, then wake everybody (notify_all) on both condition variables")
        // @end
    }

    /// The number of values waiting.
    pub fn len(&self) -> usize {
        // @begin r-04
        self.state.lock().unwrap().items.len()
        //~ todo!("r-04: the number of waiting values")
        // @end
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
}

/// Hands out numbers 0, 1, 2, … to any number of threads, each number once: a counter shared by `&self`, with no lock.
pub struct IdGenerator {
    // @begin r-04
    next: AtomicU64,
    //~ _ids: (),
    //~ // TODO(r-04): your field: an atomic number
    // @end
}

impl IdGenerator {
    /// A generator whose first number is 0.
    pub fn new() -> IdGenerator {
        // @begin r-04
        IdGenerator { next: AtomicU64::new(0) }
        //~ todo!("r-04: start at 0")
        // @end
    }

    /// The next number. Two threads never get the same one.
    pub fn next(&self) -> u64 {
        // @begin r-04
        self.next.fetch_add(1, Ordering::Relaxed)
        //~ todo!("r-04: one atomic read-and-add (fetch_add), not a load followed by a store")
        // @end
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
    // @begin r-04
    let threads = threads.max(1);
    let chunk = values.len().div_ceil(threads).max(1);
    thread::scope(|scope| {
        let handles: Vec<_> = values
            .chunks(chunk)
            .map(|part| scope.spawn(move || part.iter().fold(0u64, |a, &b| a.wrapping_add(b))))
            .collect();
        handles.into_iter().fold(0u64, |a, h| a.wrapping_add(h.join().unwrap()))
    })
    //~ todo!("r-04: chunks of the input, one scoped thread each (thread::scope lets them borrow `values`), their sums added up")
    // @end
}
