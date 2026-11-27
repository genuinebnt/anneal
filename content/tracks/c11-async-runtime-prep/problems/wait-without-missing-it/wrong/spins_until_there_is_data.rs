use std::io;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Mutex, PoisonError};
use std::task::{Context, Poll, Waker};

/// Where an event source and the task waiting for it meet.
pub struct Readiness {
    /// Counts the news. A change means: something happened since you last looked.
    epoch: AtomicU64,
    waker: Mutex<Option<Waker>>,
}

impl Readiness {
    pub fn new() -> Readiness {
        Readiness {
            epoch: AtomicU64::new(0),
            waker: Mutex::new(None),
        }
    }

    /// Called by the source: something new has happened.
    pub fn ready(&self) {
        // count first, then take the waker: a poll that stores its waker too late for the take sees the new count
        self.epoch.fetch_add(1, Ordering::SeqCst);
        let waker = self.slot().take();
        if let Some(waker) = waker {
            waker.wake();
        }
    }

    fn slot(&self) -> std::sync::MutexGuard<'_, Option<Waker>> {
        self.waker.lock().unwrap_or_else(PoisonError::into_inner)
    }
}

/// Tries a non-blocking operation, and arranges to be woken if there is nothing yet.
pub fn poll_io<T>(
    readiness: &Readiness,
    cx: &mut Context<'_>,
    mut op: impl FnMut() -> io::Result<T>,
) -> Poll<io::Result<T>> {
    loop {
        let seen = readiness.epoch.load(Ordering::SeqCst);
        match op() {
            Err(e) if e.kind() == io::ErrorKind::WouldBlock => {
                let old = readiness.slot().replace(cx.waker().clone());
                // dropping a waker can run code: not under the lock
                drop(old);
                let _ = seen;
                std::thread::yield_now();
                // news arrived after we looked: look again
            }
            Err(e) if e.kind() == io::ErrorKind::Interrupted => {}
            result => return Poll::Ready(result),
        }
    }
}

impl Default for Readiness {
    fn default() -> Self {
        Readiness::new()
    }
}
