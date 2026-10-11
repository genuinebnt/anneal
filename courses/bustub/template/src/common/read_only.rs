//! A session that cannot change anything.

use crate::common::exception::{Exception, ExceptionType, Result};
use crate::common::session::Session;
use crate::sql::ast::Statement;
use crate::sql::parser;

pub struct ReadOnlySession<'a> {
    _ro: std::marker::PhantomData<&'a ()>,
}

impl<'a> ReadOnlySession<'a> {
    /// Wraps `session`.
    pub fn new(session: Session<'a>) -> ReadOnlySession<'a> {
        todo!("4e-c6: wrap the session")
    }

    /// The wrapped session. Statements run through it are not checked.
    pub fn inner(&mut self) -> &mut Session<'a> {
        todo!("4e-c6: the wrapped session")
    }

    /// Runs `sql` if none of its statements changes data or schema.
    ///
    /// # Errors
    ///
    /// Refuses the whole text, running nothing, with an `Invalid` exception naming the first kind of statement that would change something.
    pub fn execute(&mut self, sql: &str) -> Result<Vec<String>> {
        let _ = (sql, parser::parse as fn(&str) -> Result<Vec<Statement>>);
        todo!("4e-c6: parse; refuse the whole text if any statement changes data or schema; else run it")
    }

    // TODO(4e-c6): a helper of yours
}
