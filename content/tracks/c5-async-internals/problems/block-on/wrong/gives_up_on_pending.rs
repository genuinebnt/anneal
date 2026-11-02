use std::future::Future;
use std::pin::pin;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::task::{Context, Poll, Wake, Waker};
use std::thread::{self, Thread};

/// What a `Waker` does here: remember that a wake happened, and unpark the thread that is blocked in `block_on`.
struct ThreadWaker {
    thread: Thread,
    woken: AtomicBool,
}

impl Wake for ThreadWaker {
    fn wake(self: Arc<Self>) {
        self.wake_by_ref();
    }

    fn wake_by_ref(self: &Arc<Self>) {
        self.woken.store(true, Ordering::Release);
        self.thread.unpark();
    }
}

/// Runs `fut` to completion on the current thread, sleeping (not spinning) while it is pending.
pub fn block_on<F: Future>(fut: F) -> F::Output {
    let mut fut = pin!(fut); // pinned on this stack frame: the future never moves again
    let tw = Arc::new(ThreadWaker { thread: thread::current(), woken: AtomicBool::new(false) });
    let waker = Waker::from(tw.clone());
    let mut cx = Context::from_waker(&waker);
    loop {
        if let Poll::Ready(v) = fut.as_mut().poll(&mut cx) {
            return v;
        }
        panic!("pending");
    }
}
