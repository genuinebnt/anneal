use std::io;
use std::task::{Context, Poll};

/// Where an event source and the task waiting for it meet.
pub struct Readiness {
    // TODO: the fields
}

impl Readiness {
    pub fn new() -> Readiness {
        todo!()
    }

    /// Called by the source: something new has happened.
    pub fn ready(&self) {
        todo!()
    }
}

/// Tries a non-blocking operation, and arranges to be woken if there is nothing yet.
pub fn poll_io<T>(
    readiness: &Readiness,
    cx: &mut Context<'_>,
    op: impl FnMut() -> io::Result<T>,
) -> Poll<io::Result<T>> {
    todo!()
}
