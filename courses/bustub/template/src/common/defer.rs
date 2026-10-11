//! A scope guard: runs a closure when it is dropped.

pub struct Defer<F: FnOnce()> {
    _defer: std::marker::PhantomData<F>,
}

impl<F: FnOnce()> Defer<F> {
    /// A guard that runs `f` when it is dropped.
    pub fn new(f: F) -> Defer<F> {
        todo!("1g-c1: remember the closure")
    }

    /// The closure will not run.
    pub fn cancel(&mut self) {
        todo!("1g-c1: forget the closure")
    }

    /// Runs the closure now, once.
    pub fn run_now(&mut self) {
        todo!("1g-c1: run it and make sure it is not run again")
    }
}

impl<F: FnOnce()> Drop for Defer<F> {
    fn drop(&mut self) {
        // TODO(1g-c1): run the closure unless it was cancelled or already run
    }
}
