//! A one-shot promise and future: one side sets a value once, the other waits for it.

// @begin 1b-02
use std::sync::{Arc, Condvar, Mutex, PoisonError};

enum State<T> {
    Pending,
    Ready(T),
    /// The promise was dropped without a value.
    Broken,
    /// The future has taken the value.
    Taken,
}

struct Shared<T> {
    state: Mutex<State<T>>,
    changed: Condvar,
}
//~ // TODO(1b-02): your imports and private types go here.
// @end

/// The promise was dropped before it set a value, so the future will never get one.
#[derive(Debug, PartialEq, Eq)]
pub struct BrokenPromise;

/// The setting side. `set` consumes it, so a value can be set at most once.
pub struct Promise<T> {
    // @begin 1b-02
    shared: Arc<Shared<T>>,
    //~ // TODO(1b-02): the fields are yours; a promise and its future share something.
    //~ _value: std::marker::PhantomData<T>, // delete this line once one of your fields mentions T
    // @end
}

/// The waiting side. `get` consumes it, so a value can be taken at most once.
pub struct Future<T> {
    // @begin 1b-02
    shared: Arc<Shared<T>>,
    //~ // TODO(1b-02): the fields are yours.
    //~ _value: std::marker::PhantomData<T>, // delete this line once one of your fields mentions T
    // @end
}

/// A connected promise and future.
pub fn promise<T>() -> (Promise<T>, Future<T>) {
    // @begin 1b-02
    let shared = Arc::new(Shared { state: Mutex::new(State::Pending), changed: Condvar::new() });
    (Promise { shared: Arc::clone(&shared) }, Future { shared })
    //~ todo!("1b-02: a promise and a future that share whatever carries the value")
    // @end
}

impl<T> Promise<T> {
    /// Gives the future its value and wakes it.
    pub fn set(self, value: T) {
        // @begin 1b-02
        *self.shared.state.lock().unwrap() = State::Ready(value);
        self.shared.changed.notify_all();
        //~ todo!("1b-02: hand the value to the future and wake it if it is waiting")
        // @end
    }
}

// @begin 1b-02
impl<T> Drop for Promise<T> {
    fn drop(&mut self) {
        let mut state = self.shared.state.lock().unwrap_or_else(PoisonError::into_inner);
        if matches!(*state, State::Pending) {
            *state = State::Broken;
            self.shared.changed.notify_all();
        }
    }
}
//~ // TODO(1b-02): a promise that is dropped without ever setting a value must wake the future, which then sees `BrokenPromise`
//~ // (a `Drop` impl is how Rust runs code when a value goes away).
// @end

impl<T> Future<T> {
    /// True once `get` would return without waiting: the value is set, or the promise is broken.
    pub fn is_ready(&self) -> bool {
        // @begin 1b-02
        !matches!(*self.shared.state.lock().unwrap(), State::Pending)
        //~ todo!("1b-02: would get return at once?")
        // @end
    }

    /// Waits for the value. `Err(BrokenPromise)` if the promise was dropped without setting one.
    pub fn get(self) -> Result<T, BrokenPromise> {
        // @begin 1b-02
        let mut state = self.shared.changed.wait_while(self.shared.state.lock().unwrap(), |s| matches!(s, State::Pending)).unwrap();
        match std::mem::replace(&mut *state, State::Taken) {
            State::Ready(value) => Ok(value),
            State::Broken => Err(BrokenPromise),
            State::Pending | State::Taken => unreachable!("the wait ended, and a future is only read once"),
        }
        //~ todo!("1b-02: wait for the value (or for the promise to be dropped), then return it")
        // @end
    }
}
