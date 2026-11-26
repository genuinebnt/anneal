use std::future::Future;
use std::pin::Pin;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use std::task::{Context, Poll};
use tower::{Service, ServiceExt};

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

struct InFlight {
    stats: Arc<Stats>,
    succeeded: Option<bool>,
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
    S::Error: Send,
    Request: Send + 'static,
{
    type Response = S::Response;
    type Error = S::Error;
    type Future = Pin<Box<dyn Future<Output = Result<S::Response, S::Error>> + Send>>;

    // "Always ready", and the clone waits for itself: no panic, but also no backpressure.
    fn poll_ready(&mut self, _: &mut Context<'_>) -> Poll<Result<(), S::Error>> {
        Poll::Ready(Ok(()))
    }

    fn call(&mut self, request: Request) -> Self::Future {
        let mut inner = self.inner.clone();
        self.stats.in_flight.fetch_add(1, Ordering::SeqCst);
        let mut guard = InFlight {
            stats: self.stats.clone(),
            succeeded: None,
        };
        Box::pin(async move {
            // Take the whole guard: a `move` block would otherwise copy just the field it writes.
            let mut guard = guard;
            let result = match inner.ready().await {
                Ok(ready) => ready.call(request).await,
                Err(e) => Err(e),
            };
            guard.succeeded = Some(result.is_ok());
            result
        })
    }
}
