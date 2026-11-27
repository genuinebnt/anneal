use std::future::Future;
use std::marker::PhantomData;
use std::pin::Pin;
use std::task::{Context, Poll};

/// Which side of a `select` finished.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Either<L, R> {
    Left(L),
    Right(R),
}

/// Waits for every child.
pub struct JoinAll<F: Future> {
    // TODO: the fields
    _todo: PhantomData<F>,
}

/// Waits for the first of two.
pub struct Select<A: Future, B: Future> {
    // TODO: the fields
    _todo: PhantomData<(A, B)>,
}

pub fn join_all<F: Future>(futures: Vec<F>) -> JoinAll<F> {
    todo!()
}

pub fn select<A: Future, B: Future>(a: A, b: B) -> Select<A, B> {
    todo!()
}

impl<F: Future> Future for JoinAll<F> {
    type Output = Vec<F::Output>;

    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        todo!()
    }
}

impl<A: Future, B: Future> Future for Select<A, B> {
    type Output = Either<A::Output, B::Output>;

    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        todo!()
    }
}
