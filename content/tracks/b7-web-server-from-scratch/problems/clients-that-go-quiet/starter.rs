use std::io::{self, ErrorKind, Read, Write};
use std::time::Duration;

#[derive(Debug, Clone, Copy)]
pub struct Limits {
    /// How long a connection may wait for the first byte of its next line.
    pub idle: Duration,
    /// How long a line may take, from its first byte to its end.
    pub line: Duration,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Ending {
    ClientLeft,
    Idle,
    TooSlow,
}

pub fn serve<S: Read + Write>(
    stream: &mut S,
    now: impl Fn() -> Duration,
    limits: Limits,
) -> io::Result<Ending> {
    todo!()
}
