//! A scope guard: runs a closure when it is dropped.

pub struct Defer<F: FnOnce()> {
    // @begin 1g-c1
    f: Option<F>,
    //~ _defer: std::marker::PhantomData<F>,
    // @end
}

impl<F: FnOnce()> Defer<F> {
    /// A guard that runs `f` when it is dropped.
    pub fn new(f: F) -> Defer<F> {
        // @begin 1g-c1
        Defer { f: Some(f) }
        //~ todo!("1g-c1: remember the closure")
        // @end
    }

    /// The closure will not run.
    pub fn cancel(&mut self) {
        // @begin 1g-c1
        self.f = None;
        //~ todo!("1g-c1: forget the closure")
        // @end
    }

    /// Runs the closure now, once.
    pub fn run_now(&mut self) {
        // @begin 1g-c1
        if let Some(f) = self.f.take() {
            f();
        }
        //~ todo!("1g-c1: run it and make sure it is not run again")
        // @end
    }
}

impl<F: FnOnce()> Drop for Defer<F> {
    fn drop(&mut self) {
        // @begin 1g-c1
        if let Some(f) = self.f.take() {
            f();
        }
        //~ // TODO(1g-c1): run the closure unless it was cancelled or already run
        // @end
    }
}
