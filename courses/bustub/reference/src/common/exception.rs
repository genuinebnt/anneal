//! The database's error type. A failing SQL operation (an overflow, a division by zero, an impossible cast) is an *expected* error,
//! returned as `Result<_, Exception>`, not a bug.

use std::fmt;

/// The kinds of failure.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ExceptionType {
    Invalid,
    OutOfRange,
    Conversion,
    UnknownType,
    Decimal,
    MismatchType,
    DivideByZero,
    IncompatibleType,
    NotImplemented,
    Execution,
    Optimizer,
    Io,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Exception {
    pub kind: ExceptionType,
    pub message: String,
    /// The SQLSTATE code a client sees for this failure, when the place that raised it knows a more precise one than its kind
    /// implies (a unique violation is `23505`, not the `22023` of any `Invalid` error). `None` for most errors.
    pub sqlstate: Option<&'static str>,
}

impl Exception {
    pub fn new(kind: ExceptionType, message: impl Into<String>) -> Exception {
        Exception { kind, message: message.into(), sqlstate: None }
    }

    /// The same failure, reported to clients under the SQLSTATE code `code` (five digits or capital letters).
    pub fn with_sqlstate(self, code: &'static str) -> Exception {
        Exception { sqlstate: Some(code), ..self }
    }
}

impl fmt::Display for Exception {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Exception: [{:?}] {}", self.kind, self.message)
    }
}

impl std::error::Error for Exception {}

pub type Result<T> = std::result::Result<T, Exception>;
