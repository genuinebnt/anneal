//! One small failing task, written with four kinds of error type.
//!
//! The task: read a file, find its `port=` line and return the port. It can fail
//! in four ways: the file cannot be read, there is no `port=` line, the value is
//! not a `u16`, or the value is 0.

pub mod app;
pub mod boxed;
pub mod derived;
pub mod hand;
pub mod report;
pub mod status;
