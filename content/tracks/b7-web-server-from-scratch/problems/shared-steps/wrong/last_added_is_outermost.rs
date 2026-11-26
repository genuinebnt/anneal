use std::any::Any;
use std::panic::{catch_unwind, AssertUnwindSafe};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Call {
    pub path: String,
    pub notes: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Answer {
    pub status: u16,
    pub body: String,
}

/// Everything inside the current step.
pub type Next<'a> = &'a dyn Fn(&Call) -> Answer;

type Step = Box<dyn Fn(&Call, Next<'_>) -> Answer + Send + Sync>;

pub struct Pipeline {
    handler: Box<dyn Fn(&Call) -> Answer + Send + Sync>,
    steps: Vec<Step>,
}

fn through(steps: &[Step], handler: Next<'_>, call: &Call) -> Answer {
    match steps.split_last() {
        None => handler(call),
        Some((step, inner)) => step(call, &|call: &Call| through(inner, handler, call)),
    }
}

impl Pipeline {
    pub fn new(handler: impl Fn(&Call) -> Answer + Send + Sync + 'static) -> Pipeline {
        Pipeline { handler: Box::new(handler), steps: Vec::new() }
    }

    pub fn wrap(mut self, step: impl Fn(&Call, Next<'_>) -> Answer + Send + Sync + 'static) -> Pipeline {
        self.steps.push(Box::new(step));
        self
    }

    pub fn run(&self, call: &Call) -> Answer {
        through(&self.steps, &*self.handler, call)
    }
}

/// `panic!` carries a `&str` for a plain literal and a `String` when it was formatted.
fn message(payload: &(dyn Any + Send)) -> &str {
    if let Some(text) = payload.downcast_ref::<&str>() {
        text
    } else if let Some(text) = payload.downcast_ref::<String>() {
        text
    } else {
        "unknown panic"
    }
}

pub fn shield(report: impl Fn(&str) + Send + Sync + 'static) -> impl Fn(&Call, Next<'_>) -> Answer + Send + Sync + 'static {
    move |call, next| {
        // the closure only reads `call` and `next`; no half-updated value of ours is seen after a panic
        match catch_unwind(AssertUnwindSafe(|| next(call))) {
            Ok(answer) => answer,
            Err(payload) => {
                report(message(&*payload));
                Answer { status: 500, body: "internal error".to_string() }
            }
        }
    }
}
