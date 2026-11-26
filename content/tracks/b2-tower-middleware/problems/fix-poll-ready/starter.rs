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
        let mut inner = self.inner.clone();
        let stats = self.stats.clone();
        Box::pin(async move {
            stats.in_flight.fetch_add(1, Ordering::SeqCst);
            let result = inner.call(request).await;
            stats.in_flight.fetch_sub(1, Ordering::SeqCst);
            match &result {
                Ok(_) => stats.ok.fetch_add(1, Ordering::SeqCst),
                Err(_) => stats.err.fetch_add(1, Ordering::SeqCst),
            };
            result
        })
    }
}
