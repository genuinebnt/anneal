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
    // @begin 3k-01
    /// What this layer says about the failure.
    message: String,
    /// The error this one wraps, kept as a value so that callers can still ask what it was.
    source: Option<Box<dyn Error + Send + Sync + 'static>>,
    //~ // TODO(3k-01): the fields are yours: what the error says, and the cause it keeps.
    //~ _yours: (),
    // @end
}

// @begin 3k-01
impl DbError {
    /// An error that says what its cause says, and keeps the cause.
    fn wrap<E: Error + Send + Sync + 'static>(cause: E) -> DbError {
        DbError { message: cause.to_string(), source: Some(Box::new(cause)) }
    }
}
//~ // TODO(3k-01): a private helper of your own may go here: every `From` below builds an error from a cause.
// @end

impl fmt::Display for DbError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        // @begin 3k-01
        f.write_str(&self.message)
        //~ todo!("3k-01: what this error says; for an error made by `From`, exactly what its cause says")
        // @end
    }
}

impl Error for DbError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        // @begin 3k-01
        self.source.as_deref().map(|e| e as &(dyn Error + 'static))
        //~ todo!("3k-01: the cause this error keeps, if any")
        // @end
    }
}

impl From<io::Error> for DbError {
    fn from(e: io::Error) -> DbError {
        // @begin 3k-01
        DbError::wrap(e)
        //~ todo!("3k-01: keep the I/O error as the cause")
        // @end
    }
}

impl From<Exception> for DbError {
    fn from(e: Exception) -> DbError {
        // @begin 3k-01
        DbError::wrap(e)
        //~ todo!("3k-01: keep the engine's exception as the cause")
        // @end
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
        // @begin 3k-02
        f.write_str(match self {
            IntError::Empty => "the text is empty, not an integer",
            IntError::Invalid => "invalid integer",
            IntError::OutOfRange => "integer out of range for 64 bits",
        })
        //~ todo!("3k-02: empty / invalid / out of range, in words (see the table on the stage page)")
        // @end
    }
}

impl fmt::Display for IntOpError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        // @begin 3k-02
        f.write_str(match self {
            IntOpError::Overflow => "integer overflow",
            IntOpError::DivisionByZero => "division by zero",
        })
        //~ todo!("3k-02: overflow / division by zero")
        // @end
    }
}

impl fmt::Display for LikeError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        // @begin 3k-02
        f.write_str("LIKE pattern ends with an escape character")
        //~ todo!("3k-02: a trailing escape character")
        // @end
    }
}

impl fmt::Display for NameError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        // @begin 3k-02
        f.write_str(match self {
            NameError::Exists => "name already exists",
            NameError::Missing => "name does not exist",
            NameError::Invalid => "invalid name",
        })
        //~ todo!("3k-02: already exists / does not exist / invalid")
        // @end
    }
}

impl fmt::Display for LexError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        // @begin 3k-02
        let what = match self.kind {
            LexErrorKind::UnterminatedString => "unterminated string literal",
            LexErrorKind::UnterminatedIdent => "unterminated quoted identifier",
            LexErrorKind::UnterminatedComment => "unterminated comment",
            LexErrorKind::BadChar => "unexpected character",
            LexErrorKind::IntegerOverflow => "integer literal out of range",
        };
        write!(f, "{what} at position {}", self.pos)
        //~ todo!("3k-02: what went wrong and the position, in words")
        // @end
    }
}

impl fmt::Display for ResolveError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        // @begin 3k-02
        match self {
            ResolveError::UnknownTable => f.write_str("unknown table"),
            ResolveError::UnknownColumn => f.write_str("unknown column"),
            ResolveError::Ambiguous(candidates) => write!(f, "ambiguous column reference ({} candidates)", candidates.len()),
        }
        //~ todo!("3k-02: unknown table / unknown column / ambiguous")
        // @end
    }
}

impl fmt::Display for FreeError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        // @begin 3k-02
        f.write_str(match self {
            FreeError::DoubleFree => "page is already free",
            FreeError::OutOfRange => "page id out of range",
        })
        //~ todo!("3k-02: already free / out of range")
        // @end
    }
}

impl fmt::Display for SealError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        // @begin 3k-02
        f.write_str(match self {
            SealError::TooShort => "sealed data is too short",
            SealError::Mismatch => "checksum mismatch",
        })
        //~ todo!("3k-02: too short / checksum mismatch")
        // @end
    }
}

// @begin 3k-02
macro_rules! convert_from {
    ($($t:ty),* $(,)?) => {
        $(impl From<$t> for DbError {
            fn from(e: $t) -> DbError {
                DbError::wrap(e)
            }
        })*
    };
}
convert_from!(IntError, IntOpError, LikeError, NameError, LexError, ResolveError, FreeError, SealError);
//~ // TODO(3k-02): the eight conversions below are stubs. Each keeps its cause as a value, like the two above. Write them by hand, or
//~ // replace all eight with one macro; delete these lines either way (two impls for one type do not compile).
//~ impl From<IntError> for DbError { fn from(_: IntError) -> DbError { todo!("3k-02: keep the IntError as the cause") } }
//~ impl From<IntOpError> for DbError { fn from(_: IntOpError) -> DbError { todo!("3k-02: keep the IntOpError as the cause") } }
//~ impl From<LikeError> for DbError { fn from(_: LikeError) -> DbError { todo!("3k-02: keep the LikeError as the cause") } }
//~ impl From<NameError> for DbError { fn from(_: NameError) -> DbError { todo!("3k-02: keep the NameError as the cause") } }
//~ impl From<LexError> for DbError { fn from(_: LexError) -> DbError { todo!("3k-02: keep the LexError as the cause") } }
//~ impl From<ResolveError> for DbError { fn from(_: ResolveError) -> DbError { todo!("3k-02: keep the ResolveError as the cause") } }
//~ impl From<FreeError> for DbError { fn from(_: FreeError) -> DbError { todo!("3k-02: keep the FreeError as the cause") } }
//~ impl From<SealError> for DbError { fn from(_: SealError) -> DbError { todo!("3k-02: keep the SealError as the cause") } }
// @end

impl DbError {
    /// Adds a layer: an error that says `message` and has this one as its cause.
    pub fn context(self, message: impl Into<String>) -> DbError {
        // @begin 3k-03
        DbError { message: message.into(), source: Some(Box::new(self)) }
        //~ todo!("3k-03: a new error whose cause is this one")
        // @end
    }

    /// This error, then its cause, then the cause's cause, and so on to the innermost.
    pub fn chain(&self) -> Box<dyn Iterator<Item = &(dyn Error + 'static)> + '_> {
        // @begin 3k-03
        Box::new(std::iter::successors(Some(self as &(dyn Error + 'static)), |&e| e.source()))
        //~ todo!("3k-03: walk `source()` until it runs out")
        // @end
    }

    /// The innermost error of the chain: what really went wrong.
    pub fn root_cause(&self) -> &(dyn Error + 'static) {
        // @begin 3k-03
        self.chain().last().expect("a chain has at least this error")
        //~ todo!("3k-03: the last element of the chain")
        // @end
    }

    /// Every layer's message, outermost first, joined with ": ". A layer that says exactly what the one below it says is shown once.
    pub fn report(&self) -> String {
        // @begin 3k-03
        let mut parts: Vec<String> = Vec::new();
        for e in self.chain() {
            let text = e.to_string();
            if parts.last() != Some(&text) {
                parts.push(text);
            }
        }
        parts.join(": ")
        //~ todo!("3k-03: the messages of the chain, outermost first, a repeated message once")
        // @end
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
        // @begin 3k-03
        self.map_err(|e| DbError::from(e).context(message))
        //~ todo!("3k-03: convert the error, then add the layer")
        // @end
    }

    fn with_context<F: FnOnce() -> String>(self, message: F) -> DbResult<T> {
        // @begin 3k-03
        self.map_err(|e| DbError::from(e).context(message()))
        //~ todo!("3k-03: the same, building the message only on failure")
        // @end
    }
}

impl DbError {
    /// A five-character code for the kind of failure, in the style of SQLSTATE (see the table on the stage page). It comes from the
    /// innermost cause, so adding context never changes it.
    pub fn code(&self) -> &'static str {
        // @begin 3k-04
        let root = self.root_cause();
        if let Some(e) = root.downcast_ref::<Exception>() {
            return match e.kind {
                ExceptionType::Invalid => "22023",
                ExceptionType::OutOfRange | ExceptionType::Decimal => "22003",
                ExceptionType::Conversion => "22018",
                ExceptionType::DivideByZero => "22012",
                ExceptionType::UnknownType => "42704",
                ExceptionType::MismatchType | ExceptionType::IncompatibleType => "42804",
                ExceptionType::NotImplemented => "0A000",
                ExceptionType::Io => "58030",
                ExceptionType::Execution | ExceptionType::Optimizer => "XX000",
            };
        }
        if root.is::<io::Error>() {
            return "58030";
        }
        if let Some(e) = root.downcast_ref::<IntError>() {
            return if *e == IntError::OutOfRange { "22003" } else { "22P02" };
        }
        if let Some(e) = root.downcast_ref::<IntOpError>() {
            return if *e == IntOpError::Overflow { "22003" } else { "22012" };
        }
        if root.is::<LikeError>() {
            return "22025";
        }
        if let Some(e) = root.downcast_ref::<NameError>() {
            return match e {
                NameError::Exists => "42P07",
                NameError::Missing => "42P01",
                NameError::Invalid => "42602",
            };
        }
        if root.is::<LexError>() {
            return "42601";
        }
        if let Some(e) = root.downcast_ref::<ResolveError>() {
            return match e {
                ResolveError::UnknownTable => "42P01",
                ResolveError::UnknownColumn => "42703",
                ResolveError::Ambiguous(_) => "42702",
            };
        }
        if root.is::<SealError>() {
            return "XX001";
        }
        "XX000"
        //~ todo!("3k-04: look at the innermost cause and answer from the table on the stage page; anything not listed is XX000")
        // @end
    }

    /// What a client of the database may be shown: the code and, for the user's own mistakes, this error's message. Failures of the
    /// system itself (class 58, class XX) say only that something went wrong inside: their messages can hold paths and internals.
    pub fn client_message(&self) -> String {
        // @begin 3k-04
        let code = self.code();
        if code.starts_with("58") || code.starts_with("XX") {
            format!("ERROR {code}: internal error")
        } else {
            format!("ERROR {code}: {self}")
        }
        //~ todo!("3k-04: `ERROR <code>: <message>`, but `ERROR <code>: internal error` for codes of class 58 and XX")
        // @end
    }
}
