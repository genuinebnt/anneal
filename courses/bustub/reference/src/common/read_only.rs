//! A session that cannot change anything.

use crate::common::exception::{Exception, ExceptionType, Result};
use crate::common::session::Session;
use crate::sql::ast::Statement;
use crate::sql::parser;

pub struct ReadOnlySession<'a> {
    // @begin 4e-c6
    inner: Session<'a>,
    //~ _ro: std::marker::PhantomData<&'a ()>,
    // @end
}

impl<'a> ReadOnlySession<'a> {
    /// Wraps `session`.
    pub fn new(session: Session<'a>) -> ReadOnlySession<'a> {
        // @begin 4e-c6
        ReadOnlySession { inner: session }
        //~ todo!("4e-c6: wrap the session")
        // @end
    }

    /// The wrapped session. Statements run through it are not checked.
    pub fn inner(&mut self) -> &mut Session<'a> {
        // @begin 4e-c6
        &mut self.inner
        //~ todo!("4e-c6: the wrapped session")
        // @end
    }

    /// Runs `sql` if none of its statements changes data or schema.
    ///
    /// # Errors
    ///
    /// Refuses the whole text, running nothing, with an `Invalid` exception naming the first kind of statement that would change something.
    pub fn execute(&mut self, sql: &str) -> Result<Vec<String>> {
        // @begin 4e-c6
        for statement in parser::parse(sql)? {
            if let Some(kind) = Self::change_kind(&statement) {
                return Err(Exception::new(ExceptionType::Invalid, format!("cannot execute {kind} in a read-only session")));
            }
        }
        self.inner.execute(sql)
        //~ let _ = (sql, parser::parse as fn(&str) -> Result<Vec<Statement>>);
        //~ todo!("4e-c6: parse; refuse the whole text if any statement changes data or schema; else run it")
        // @end
    }

    // @begin 4e-c6
    /// What kind of change `statement` is, if it is one (also when hidden inside EXPLAIN).
    fn change_kind(statement: &Statement) -> Option<&'static str> {
        match statement {
            Statement::Insert { .. } => Some("INSERT"),
            Statement::Update { .. } => Some("UPDATE"),
            Statement::Delete { .. } => Some("DELETE"),
            Statement::CreateTable { .. } => Some("CREATE TABLE"),
            Statement::CreateIndex { .. } => Some("CREATE INDEX"),
            Statement::Explain { statement, .. } => Self::change_kind(statement),
            _ => None,
        }
    }
    //~ // TODO(4e-c6): a helper of yours
    // @end
}
