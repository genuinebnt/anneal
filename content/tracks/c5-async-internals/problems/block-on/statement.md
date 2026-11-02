Write the smallest async runtime: `block_on(fut)` drives one future to completion on the calling thread.

Poll the future with a `Context` built from a `Waker` you create. If it returns `Pending`, the thread must **sleep** until the
waker is called (from any thread, at any time, including during the `poll` itself or before you park) and then poll
again. It must not busy-loop: the tests count polls.

Build the `Waker` from your own type with `std::task::Wake` (`Waker::from(Arc<W>)`), and pin the future with `pin!`.
