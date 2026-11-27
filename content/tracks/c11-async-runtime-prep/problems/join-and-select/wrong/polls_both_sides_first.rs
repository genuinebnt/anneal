use std::future::Future;
use std::pin::Pin;
use std::task::{Context, Poll};

/// Which side of a `select` finished.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Either<L, R> {
    Left(L),
    Right(R),
}

/// A child that is still running (pinned on the heap), or what it left behind.
enum Slot<F: Future> {
    Running(Pin<Box<F>>),
    Done(F::Output),
}

/// Waits for every child.
pub struct JoinAll<F: Future> {
    /// `None` once the result has been handed out.
    slots: Option<Vec<Slot<F>>>,
}

/// Waits for the first of two.
pub struct Select<A: Future, B: Future> {
    /// `None` once a side has won.
    both: Option<(Pin<Box<A>>, Pin<Box<B>>)>,
}

pub fn join_all<F: Future>(futures: Vec<F>) -> JoinAll<F> {
    JoinAll {
        slots: Some(
            futures
                .into_iter()
                .map(|f| Slot::Running(Box::pin(f)))
                .collect(),
        ),
    }
}

pub fn select<A: Future, B: Future>(a: A, b: B) -> Select<A, B> {
    Select {
        both: Some((Box::pin(a), Box::pin(b))),
    }
}

// Every child is behind its own `Box`, and the outputs are never pinned, so moving the combinator moves nothing that was
// pinned. That makes it safe to say it is `Unpin`, which lets `poll` use `get_mut`.
impl<F: Future> Unpin for JoinAll<F> {}

impl<F: Future> Future for JoinAll<F> {
    type Output = Vec<F::Output>;

    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        let this = self.get_mut();
        let slots = this
            .slots
            .as_mut()
            .expect("join_all polled after it finished");
        let mut all_done = true;
        for slot in slots.iter_mut() {
            if let Slot::Running(child) = slot {
                match child.as_mut().poll(cx) {
                    // assigning drops the boxed child here, in this poll
                    Poll::Ready(output) => *slot = Slot::Done(output),
                    Poll::Pending => all_done = false,
                }
            }
        }
        if !all_done {
            return Poll::Pending;
        }
        let slots = this.slots.take().expect("checked above");
        Poll::Ready(
            slots
                .into_iter()
                .map(|slot| match slot {
                    Slot::Done(output) => output,
                    Slot::Running(_) => unreachable!("every child is done"),
                })
                .collect(),
        )
    }
}

impl<A: Future, B: Future> Future for Select<A, B> {
    type Output = Either<A::Output, B::Output>;

    fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        let (a, b) = self.both.as_mut().expect("select polled after it finished");
        let (ra, rb) = (a.as_mut().poll(cx), b.as_mut().poll(cx));
        let won = match (ra, rb) {
            (Poll::Ready(v), _) => Either::Left(v),
            (_, Poll::Ready(v)) => Either::Right(v),
            _ => return Poll::Pending,
        };
        // the loser is cancelled now: cancelling a future is dropping it
        self.both = None;
        Poll::Ready(won)
    }
}
