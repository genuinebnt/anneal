use std::any::Any;
use std::future::Future;
use std::time::Duration;
use tokio::sync::watch;
use tokio::task::JoinSet;

#[derive(Debug, PartialEq, Eq)]
pub enum Ending<T> {
    Done(T),
    Panicked(String),
    Cancelled,
    TimedOut,
}

fn message(payload: Box<dyn Any + Send>) -> String {
    match payload.downcast::<String>() {
        Ok(s) => *s,
        Err(other) => match other.downcast::<&str>() {
            Ok(s) => s.to_string(),
            Err(_) => "unknown panic".to_string(),
        },
    }
}

/// Resolves when somebody sends `true`. A sender that is gone can never do so.
async fn cancelled(mut cancel: watch::Receiver<bool>) {
    while cancel.changed().await.is_ok() {
        if *cancel.borrow() {
            return;
        }
    }
    std::future::pending::<()>().await;
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
    // A JoinSet aborts its tasks when dropped, so giving up on `supervise` stops the work too.
    let mut task = JoinSet::new();
    task.spawn(work);
    let early = tokio::select! {
        biased;
        () = cancelled(cancel) => Ending::Cancelled,
        joined = task.join_next() => {
            return match joined.expect("one task was spawned") {
                Ok(value) => Ending::Done(value),
                Err(e) => Ending::Panicked(message(e.into_panic())),
            };
        }
        () = tokio::time::sleep(limit) => Ending::TimedOut,
    };
    // Aborting only asks. This also waits until the work's future is gone.
    task.shutdown().await;
    early
}
