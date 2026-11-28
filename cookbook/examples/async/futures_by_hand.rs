//! RECIPE 52 (async): what a Future really is: poll it by hand with a 10-line executor.
//! LEVEL: advanced
//! Use in: understanding tokio, writing cancel-safe code, debugging "why is my task stuck?".   Run: cargo run --release --example futures_by_hand
//! COMMON USES: debugging stuck async tasks, writing executors/combinators, understanding cancellation.
#![allow(dead_code)]
//!
//! A `Future` is a state machine with one method: `poll(self: Pin<&mut Self>, cx) -> Poll<Output>`.
//! `Ready(v)` means finished. `Pending` means not yet, and then the future must arrange to be woken through `cx.waker()`.
//! `async fn` and `async {}` compile into such state machines. Nothing runs until something polls them. `.await` means
//! "poll the inner future; if it is Pending, return Pending myself". An executor such as Tokio polls a task again when
//! its waker says it may be ready. Dropping a future cancels it at the `.await` where it is waiting.
//! Read: https://rust-lang.github.io/async-book/ and Jon Gjengset's "Crust of Rust: async/await" (YouTube).
use std::future::Future;
use std::pin::{Pin, pin};
use std::task::{Context, Poll, Waker};

/// Returns Pending `left` times, then Ready(number of polls).
struct CountDown {
    left: u32,
    polls: u32,
}
impl Future for CountDown {
    type Output = u32;
    fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<u32> {
        self.polls += 1;
        if self.left == 0 {
            Poll::Ready(self.polls)
        } else {
            self.left -= 1;
            cx.waker().wake_by_ref(); // "poll me again": without a wake, a real executor would never come back
            Poll::Pending
        }
    }
}

/// The smallest possible executor: poll in a loop. (Real ones sleep until the waker fires.)
fn block_on<F: Future>(fut: F) -> F::Output {
    let mut fut = pin!(fut); // pin on the stack: the state machine must not move once polled
    let mut cx = Context::from_waker(Waker::noop());
    loop {
        if let Poll::Ready(v) = fut.as_mut().poll(&mut cx) {
            return v;
        }
    }
}

async fn add(a: u32, b: u32) -> u32 {
    println!("  [add body runs now]");
    a + b
}
async fn pipeline() -> u32 {
    let x = add(1, 2).await;
    let polls = CountDown { left: 2, polls: 0 }.await; // suspends twice inside this async fn
    x + polls
}

fn main() {
    let fut = add(1, 2);
    println!("future created: body has NOT run");
    println!(
        "size of that future: {} bytes (a state machine, no heap)",
        std::mem::size_of_val(&fut)
    );
    println!("block_on -> {}", block_on(fut));

    println!(
        "CountDown polled {} times",
        block_on(CountDown { left: 3, polls: 0 })
    );
    println!("pipeline = {}", block_on(pipeline()));

    // Different futures behind one type: Pin<Box<dyn Future>>. (`tokio::spawn` also puts a future on the heap and hides
    // its type, but with its own vtable, not as this trait object.)
    let tasks: Vec<Pin<Box<dyn Future<Output = u32>>>> = vec![
        Box::pin(add(2, 2)),
        Box::pin(async { 7 }),
        Box::pin(CountDown { left: 1, polls: 0 }),
    ];
    let results: Vec<u32> = tasks.into_iter().map(block_on).collect();
    println!("{results:?}");

    // Dropping before completion = cancellation: the code after the pending .await never runs.
    let mut cancelled = Box::pin(async {
        CountDown { left: 5, polls: 0 }.await;
        println!("never printed");
    });
    let mut cx = Context::from_waker(Waker::noop());
    let first = cancelled.as_mut().poll(&mut cx);
    println!("first poll pending: {}", first.is_pending());
    drop(cancelled);
    println!("dropped: cancelled");
}
