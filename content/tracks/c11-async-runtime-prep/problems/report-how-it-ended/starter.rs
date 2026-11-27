use std::future::Future;
use std::marker::PhantomData;
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
    // TODO: the fields
    _todo: PhantomData<(F, R)>,
}

/// Drives `future`, and tells `report` how it ended.
pub fn watched<F, R>(future: F, report: R) -> Watched<F, R>
where
    F: Future,
    R: FnOnce(Ending<F::Output>),
{
    todo!()
}

impl<F: Future, R: FnOnce(Ending<F::Output>)> Future for Watched<F, R> {
    type Output = ();

    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<()> {
        todo!()
    }
}
