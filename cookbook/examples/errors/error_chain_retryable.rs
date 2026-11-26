//! RECIPE 61 (errors): library errors with `#[source]`, error chains, and classifying errors (retryable vs fatal).
//! LEVEL: intermediate
//! Use in: workers/executors that must decide "retry or give up?" based on WHAT failed.   Run: cargo run --release --example error_chain_retryable
//! COMMON USES: retry/dead-letter decisions in workers; error chains in logs.
#![allow(dead_code)]
//!
//! Pattern: keep a typed error enum, wrap the lower-level error with `#[source]` (so the chain is preserved), and
//! add a method like `is_retryable()` so callers never string-match messages. `Error::source()` walks the chain;
//! `Box<dyn Error + Send + Sync>` is the "any error" type that can cross threads.
//! Read: https://doc.rust-lang.org/std/error/trait.Error.html and https://blog.burntsushi.net/rust-error-handling/
use std::error::Error;
use std::io::ErrorKind;

#[derive(Debug, thiserror::Error)]
enum CallError {
    #[error("network call failed")]
    Network(#[source] std::io::Error),
    #[error("server rejected request with status {status}")]
    Rejected { status: u16 },
    #[error("bad input: {0}")]
    BadInput(String),
}

#[derive(Debug, PartialEq)]
enum Retry {
    Yes,
    No,
}

impl CallError {
    fn retry(&self) -> Retry {
        match self {
            CallError::Network(e)
                if matches!(
                    e.kind(),
                    ErrorKind::TimedOut | ErrorKind::ConnectionReset | ErrorKind::ConnectionRefused
                ) =>
            {
                Retry::Yes
            }
            CallError::Rejected { status } if *status == 429 || *status >= 500 => Retry::Yes, // throttled or server-side
            _ => Retry::No, // 4xx and bad input will fail again
        }
    }
}

// An outer error that adds context about WHICH step failed and keeps the inner one as its source.
#[derive(Debug, thiserror::Error)]
#[error("step `{step}` failed")]
struct StepError {
    step: String,
    #[source]
    cause: CallError,
}

fn print_chain(e: &dyn Error) {
    let mut cur: Option<&dyn Error> = Some(e);
    let mut depth = 0;
    while let Some(err) = cur {
        println!("  {}{err}", "  ".repeat(depth));
        cur = err.source();
        depth += 1;
    }
}

fn run_step(cause: CallError) -> Result<(), Box<dyn Error + Send + Sync>> {
    Err(Box::new(StepError {
        step: "transcode".into(),
        cause,
    }))
}

fn main() {
    let cases = [
        CallError::Network(std::io::Error::new(
            ErrorKind::TimedOut,
            "deadline exceeded",
        )),
        CallError::Network(std::io::Error::new(
            ErrorKind::PermissionDenied,
            "no access",
        )),
        CallError::Rejected { status: 503 },
        CallError::Rejected { status: 404 },
        CallError::BadInput("empty id".into()),
    ];
    for c in &cases {
        println!("{:<45} -> {:?}", c.to_string(), c.retry());
    }
    assert_eq!(cases[0].retry(), Retry::Yes);
    assert_eq!(cases[3].retry(), Retry::No);

    let boxed = run_step(CallError::Rejected { status: 503 }).unwrap_err();
    println!("full chain:");
    print_chain(boxed.as_ref());
    // recover the typed error from the box to make a decision
    if let Some(step) = boxed.downcast_ref::<StepError>() {
        println!(
            "downcast ok: step={} retry={:?}",
            step.step,
            step.cause.retry()
        );
    }
}
