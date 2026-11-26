use std::future::Future;
use std::pin::Pin;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use std::task::{Context, Poll};
use tower::Service;

#[derive(Debug, Default)]
pub struct Stats {
    pub in_flight: AtomicUsize,
    pub ok: AtomicUsize,
    pub err: AtomicUsize,
    pub cancelled: AtomicUsize,
}

#[derive(Clone)]
pub struct Observe<S> {
    inner: S,
    stats: Arc<Stats>,
}

impl<S> Observe<S> {
    pub fn new(inner: S, stats: Arc<Stats>) -> Self {
        Observe { inner, stats }
    }
}

/// Counts one call from its start to whichever way it ends.
struct InFlight {
    stats: Arc<Stats>,
    /// `None` until the inner future resolves: dropped in that state means cancelled.
    succeeded: Option<bool>,
}

impl InFlight {
    fn start(stats: Arc<Stats>) -> Self {
        stats.in_flight.fetch_add(1, Ordering::SeqCst);
        InFlight {
            stats,
            succeeded: None,
        }
    }
}

impl Drop for InFlight {
    fn drop(&mut self) {
        let counter = match self.succeeded {
            Some(true) => &self.stats.ok,
            Some(false) => &self.stats.err,
            None => &self.stats.cancelled,
        };
        counter.fetch_add(1, Ordering::SeqCst);
        self.stats.in_flight.fetch_sub(1, Ordering::SeqCst);
    }
}

impl<S, Request> Service<Request> for Observe<S>
where
    S: Service<Request> + Clone + Send + 'static,
    S::Future: Send,
    Request: Send + 'static,
{
    type Response = S::Response;
    type Error = S::Error;
    type Future = Pin<Box<dyn Future<Output = Result<S::Response, S::Error>> + Send>>;

    fn poll_ready(&mut self, cx: &mut Context<'_>) -> Poll<Result<(), S::Error>> {
        self.inner.poll_ready(cx)
    }

    fn call(&mut self, request: Request) -> Self::Future {
        // Call the value that was polled ready, and keep a fresh clone for the next round.
        let clone = self.inner.clone();
        let mut ready = std::mem::replace(&mut self.inner, clone);
        let response = ready.call(request);
        let guard = InFlight::start(self.stats.clone());
        Box::pin(async move {
            // Take the whole guard: a `move` block would otherwise copy just the field it writes.
            let mut guard = guard;
            let result = response.await;
            guard.succeeded = Some(result.is_ok());
            // The ready service may hold a permit: it lives as long as the call.
            drop(ready);
            result
        })
    }
}
