use std::future::poll_fn;
use std::sync::{Mutex, MutexGuard, PoisonError};
use std::task::{Context, Poll, Waker};

fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(PoisonError::into_inner)
}

/// The wakers of the futures that are waiting. All are woken together, so a cancelled one can't keep a wake-up to itself.
#[derive(Default)]
struct Waiters(Vec<Waker>);

impl Waiters {
    fn register(&mut self, cx: &Context<'_>) {
        self.0.push(cx.waker().clone());
    }

    fn take(&mut self) -> Vec<Waker> {
        std::mem::take(&mut self.0)
    }
}

fn wake_all(wakers: Vec<Waker>) {
    wakers.into_iter().for_each(Waker::wake);
}

/// Lets at most `n` holders through at a time.
pub struct Semaphore {
    total: usize,
    /// Free permits, and who waits for one.
    state: Mutex<(usize, Waiters)>,
}

/// One of the semaphore's permits. Dropping it gives it back.
pub struct Permit<'a> {
    semaphore: &'a Semaphore,
}

/// Remembers at most one unused "something happened".
pub struct Signal {
    /// Is a signal unused, and who waits for one.
    state: Mutex<(bool, Waiters)>,
}

impl Semaphore {
    pub fn new(permits: usize) -> Semaphore {
        Semaphore {
            total: permits,
            state: Mutex::new((permits, Waiters::default())),
        }
    }

    pub async fn acquire(&self) -> Permit<'_> {
        poll_fn(|cx| {
            let mut state = lock(&self.state);
            if state.0 == 0 {
                state.1.register(cx);
                return Poll::Pending;
            }
            state.0 -= 1;
            Poll::Ready(Permit { semaphore: self })
        })
        .await
    }

    pub fn try_acquire(&self) -> Option<Permit<'_>> {
        let mut state = lock(&self.state);
        if state.0 == 0 {
            return None;
        }
        state.0 -= 1;
        Some(Permit { semaphore: self })
    }

    pub fn available(&self) -> usize {
        let free = lock(&self.state).0;
        debug_assert!(free <= self.total);
        free
    }
}

impl Drop for Permit<'_> {
    fn drop(&mut self) {
        let waiting = {
            let mut state = lock(&self.semaphore.state);
            state.0 += 1;
            state.1.take()
        };
        wake_all(waiting);
    }
}

impl Signal {
    pub fn new() -> Signal {
        Signal {
            state: Mutex::new((false, Waiters::default())),
        }
    }

    pub fn signal(&self) {
        let waiting = {
            let mut state = lock(&self.state);
            state.0 = true;
            state.1.take()
        };
        wake_all(waiting);
    }

    pub async fn wait(&self) {
        poll_fn(|cx| {
            let mut state = lock(&self.state);
            if state.0 {
                state.0 = false;
                return Poll::Ready(());
            }
            state.1.register(cx);
            Poll::Pending
        })
        .await
    }
}

impl Default for Signal {
    fn default() -> Self {
        Signal::new()
    }
}
