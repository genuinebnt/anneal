//! One error type for the whole database (module 3k). Every layer below this one has its own failure type; a client of the database
//! needs one: something it can print, walk the causes of, group by a stable code, and send across threads. **The design of `DbError` is
//! yours**: the tests only use the traits and methods named on the stage pages. The imports, the alias and the derive are given.

use std::error::Error;
use std::fmt;
use std::io;

use crate::catalog::name_catalog::NameError;
use crate::common::exception::{Exception, ExceptionType};
use crate::sql::mini_lexer::{LexError, LexErrorKind};
use crate::sql::resolve::ResolveError;
use crate::storage::disk::page_bitmap::FreeError;
use crate::storage::table::checksum::SealError;
use crate::types::int_ops::IntOpError;
use crate::types::int_parse::IntError;
use crate::types::like_match::LikeError;

/// A result whose error is a `DbError`.
pub type DbResult<T> = Result<T, DbError>;

/// A failure of the database, with the chain of causes that led to it.
#[derive(Debug)]
pub struct DbError {
    // TODO(3k-01): the fields are yours: what the error says, and the cause it keeps.
    _yours: (),
}

// TODO(3k-01): a private helper of your own may go here: every `From` below builds an error from a cause.

impl fmt::Display for DbError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        todo!("3k-01: what this error says; for an error made by `From`, exactly what its cause says")
    }
}

impl Error for DbError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        todo!("3k-01: the cause this error keeps, if any")
    }
}

impl From<io::Error> for DbError {
    fn from(e: io::Error) -> DbError {
        todo!("3k-01: keep the I/O error as the cause")
    }
}

impl From<Exception> for DbError {
    fn from(e: Exception) -> DbError {
        todo!("3k-01: keep the engine's exception as the cause")
    }
}

// The lower layers' errors are plain enums with only `Debug`. Making them real errors is what lets `DbError` keep them as causes.
impl Error for IntError {}
impl Error for IntOpError {}
impl Error for LikeError {}
impl Error for NameError {}
impl Error for LexError {}
impl Error for ResolveError {}
impl Error for FreeError {}
impl Error for SealError {}

impl fmt::Display for IntError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        todo!("3k-02: empty / invalid / out of range, in words (see the table on the stage page)")
    }
}

impl fmt::Display for IntOpError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        todo!("3k-02: overflow / division by zero")
    }
}

impl fmt::Display for LikeError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        todo!("3k-02: a trailing escape character")
    }
}

impl fmt::Display for NameError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        todo!("3k-02: already exists / does not exist / invalid")
    }
}

impl fmt::Display for LexError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        todo!("3k-02: what went wrong and the position, in words")
    }
}

impl fmt::Display for ResolveError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        todo!("3k-02: unknown table / unknown column / ambiguous")
    }
}

impl fmt::Display for FreeError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        todo!("3k-02: already free / out of range")
    }
}

impl fmt::Display for SealError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        todo!("3k-02: too short / checksum mismatch")
    }
}

// TODO(3k-02): the eight conversions below are stubs. Each keeps its cause as a value, like the two above. Write them by hand, or
// replace all eight with one macro; delete these lines either way (two impls for one type do not compile).
impl From<IntError> for DbError { fn from(_: IntError) -> DbError { todo!("3k-02: keep the IntError as the cause") } }
impl From<IntOpError> for DbError { fn from(_: IntOpError) -> DbError { todo!("3k-02: keep the IntOpError as the cause") } }
impl From<LikeError> for DbError { fn from(_: LikeError) -> DbError { todo!("3k-02: keep the LikeError as the cause") } }
impl From<NameError> for DbError { fn from(_: NameError) -> DbError { todo!("3k-02: keep the NameError as the cause") } }
impl From<LexError> for DbError { fn from(_: LexError) -> DbError { todo!("3k-02: keep the LexError as the cause") } }
impl From<ResolveError> for DbError { fn from(_: ResolveError) -> DbError { todo!("3k-02: keep the ResolveError as the cause") } }
impl From<FreeError> for DbError { fn from(_: FreeError) -> DbError { todo!("3k-02: keep the FreeError as the cause") } }
impl From<SealError> for DbError { fn from(_: SealError) -> DbError { todo!("3k-02: keep the SealError as the cause") } }

impl DbError {
    /// Adds a layer: an error that says `message` and has this one as its cause.
    pub fn context(self, message: impl Into<String>) -> DbError {
        todo!("3k-03: a new error whose cause is this one")
    }

    /// This error, then its cause, then the cause's cause, and so on to the innermost.
    pub fn chain(&self) -> Box<dyn Iterator<Item = &(dyn Error + 'static)> + '_> {
        todo!("3k-03: walk `source()` until it runs out")
    }

    /// The innermost error of the chain: what really went wrong.
    pub fn root_cause(&self) -> &(dyn Error + 'static) {
        todo!("3k-03: the last element of the chain")
    }

    /// Every layer's message, outermost first, joined with ": ". A layer that says exactly what the one below it says is shown once.
    pub fn report(&self) -> String {
        todo!("3k-03: the messages of the chain, outermost first, a repeated message once")
    }
}

/// `.context(..)` on a result: convert the error to a `DbError` and add a layer in one step.
pub trait Context<T> {
    fn context(self, message: impl Into<String>) -> DbResult<T>;
    /// Like `context`, but the message is only built if there is an error.
    fn with_context<F: FnOnce() -> String>(self, message: F) -> DbResult<T>;
}

impl<T, E> Context<T> for Result<T, E>
where
    DbError: From<E>,
{
    fn context(self, message: impl Into<String>) -> DbResult<T> {
        todo!("3k-03: convert the error, then add the layer")
    }

    fn with_context<F: FnOnce() -> String>(self, message: F) -> DbResult<T> {
        todo!("3k-03: the same, building the message only on failure")
    }
}

impl DbError {
    /// A five-character code for the kind of failure, in the style of SQLSTATE (see the table on the stage page). It comes from the
    /// innermost cause, so adding context never changes it.
    pub fn code(&self) -> &'static str {
        todo!("3k-04: look at the innermost cause and answer from the table on the stage page; anything not listed is XX000")
    }

    /// What a client of the database may be shown: the code and, for the user's own mistakes, this error's message. Failures of the
    /// system itself (class 58, class XX) say only that something went wrong inside: their messages can hold paths and internals.
    pub fn client_message(&self) -> String {
        todo!("3k-04: `ERROR <code>: <message>`, but `ERROR <code>: internal error` for codes of class 58 and XX")
    }
}
