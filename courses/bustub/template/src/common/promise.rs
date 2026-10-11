//! A one-shot promise and future: one side sets a value once, the other waits for it.

// TODO(1b-02): your imports and private types go here.

/// The promise was dropped before it set a value, so the future will never get one.
#[derive(Debug, PartialEq, Eq)]
pub struct BrokenPromise;

/// The setting side. `set` consumes it, so a value can be set at most once.
pub struct Promise<T> {
    // TODO(1b-02): the fields are yours; a promise and its future share something.
    _value: std::marker::PhantomData<T>, // delete this line once one of your fields mentions T
}

/// The waiting side. `get` consumes it, so a value can be taken at most once.
pub struct Future<T> {
    // TODO(1b-02): the fields are yours.
    _value: std::marker::PhantomData<T>, // delete this line once one of your fields mentions T
}

/// A connected promise and future.
pub fn promise<T>() -> (Promise<T>, Future<T>) {
    todo!("1b-02: a promise and a future that share whatever carries the value")
}

impl<T> Promise<T> {
    /// Gives the future its value and wakes it.
    pub fn set(self, value: T) {
        todo!("1b-02: hand the value to the future and wake it if it is waiting")
    }
}

// TODO(1b-02): a promise that is dropped without ever setting a value must wake the future, which then sees `BrokenPromise`
// (a `Drop` impl is how Rust runs code when a value goes away).

impl<T> Future<T> {
    /// True once `get` would return without waiting: the value is set, or the promise is broken.
    pub fn is_ready(&self) -> bool {
        todo!("1b-02: would get return at once?")
    }

    /// Waits for the value. `Err(BrokenPromise)` if the promise was dropped without setting one.
    pub fn get(self) -> Result<T, BrokenPromise> {
        todo!("1b-02: wait for the value (or for the promise to be dropped), then return it")
    }
}
