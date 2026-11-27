use std::future::Future;
use std::time::Duration;
use tokio::sync::watch;

#[derive(Debug, PartialEq, Eq)]
pub enum Ending<T> {
    Done(T),
    Panicked(String),
    Cancelled,
    TimedOut,
}

pub async fn supervise<T, Fut>(
    cancel: watch::Receiver<bool>,
    limit: Duration,
    work: Fut,
) -> Ending<T>
where
    Fut: Future<Output = T> + Send + 'static,
    T: Send + 'static,
{
    todo!()
}
