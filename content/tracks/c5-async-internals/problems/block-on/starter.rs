use std::future::Future;

/// Runs `fut` to completion on the current thread. While the future is pending the thread must sleep
/// (`thread::park`), and a `Waker` you build must wake it (`Thread::unpark`); it must not spin.
pub fn block_on<F: Future>(fut: F) -> F::Output {
    todo!()
}
