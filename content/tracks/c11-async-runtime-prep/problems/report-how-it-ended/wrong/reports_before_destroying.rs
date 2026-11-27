use std::any::Any;
use std::future::Future;
use std::panic::{catch_unwind, AssertUnwindSafe};
use std::pin::Pin;
use std::task::{Context, Poll};

/// How a watched future ended.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Ending<T> {
    Finished(T),
    Panicked(String),
    Dropped,
}

pub struct Watched<F: Future, R: FnOnce(Ending<F::Output>)> {
    /// Both are `Some` until the ending has been reported.
    running: Option<(Pin<Box<F>>, R)>,
}

/// Drives `future`, and tells `report` how it ended.
pub fn watched<F, R>(future: F, report: R) -> Watched<F, R>
where
    F: Future,
    R: FnOnce(Ending<F::Output>),
{
    Watched {
        running: Some((Box::pin(future), report)),
    }
}

/// `panic!` carries a `&str` for a plain literal and a `String` when it was formatted.
fn message(payload: Box<dyn Any + Send>) -> String {
    if let Some(text) = payload.downcast_ref::<&str>() {
        (*text).to_string()
    } else if let Some(text) = payload.downcast_ref::<String>() {
        text.clone()
    } else {
        "non-string panic".to_string()
    }
}

/// Drops the future behind a panic boundary.
fn destroy<F>(future: Pin<Box<F>>) -> Result<(), String> {
    // nothing of the future is looked at again, whether or not its destructor panics
    catch_unwind(AssertUnwindSafe(move || drop(future))).map_err(message)
}

// The future is boxed and the report function is never pinned, so `Watched` itself may move.
impl<F: Future, R: FnOnce(Ending<F::Output>)> Unpin for Watched<F, R> {}

impl<F: Future, R: FnOnce(Ending<F::Output>)> Future for Watched<F, R> {
    type Output = ();

    fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<()> {
        let (future, _) = self.running.as_mut().expect("polled after it finished");
        // after a panic the future is only destroyed, never polled again
        let polled = catch_unwind(AssertUnwindSafe(|| future.as_mut().poll(cx)));
        let result = match polled {
            Ok(Poll::Pending) => return Poll::Pending,
            Ok(Poll::Ready(value)) => Ok(value),
            Err(payload) => Err(message(payload)),
        };
        let (future, report) = self.running.take().expect("checked above");
        report(match result {
            Ok(value) => Ending::Finished(value),
            Err(first) => Ending::Panicked(first),
        });
        let _ = destroy(future);
        Poll::Ready(())
    }
}

impl<F: Future, R: FnOnce(Ending<F::Output>)> Drop for Watched<F, R> {
    fn drop(&mut self) {
        if let Some((future, report)) = self.running.take() {
            match destroy(future) {
                Ok(()) => report(Ending::Dropped),
                Err(from_drop) => report(Ending::Panicked(from_drop)),
            }
        }
    }
}
