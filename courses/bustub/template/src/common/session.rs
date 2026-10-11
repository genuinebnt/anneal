//! A session: one client's connection to the database, with the transaction it has open, if any.
//!
//! A statement runs in the session's own transaction when none is open (autocommit), or in the one `BEGIN` started.

use std::sync::Arc;

use crate::common::bustub_instance::BusTubInstance;
use crate::common::exception::{Exception, ExceptionType, Result};
use crate::common::result_writer::SimpleStreamWriter;
use crate::concurrency::transaction::{IsolationLevel, Transaction};
use crate::sql::ast::{Expr, Statement};
use crate::sql::parser;

pub struct Session<'a> {
    db: &'a BusTubInstance,
    _session: (),
}

impl<'a> Session<'a> {
    /// A session with no open transaction.
    pub fn new(db: &'a BusTubInstance) -> Session<'a> {
        let _ = db;
        todo!("4e-01: a session with no open transaction")
    }

    /// Is an explicit transaction open (a failed one counts: it is open until COMMIT or ROLLBACK)?
    pub fn in_transaction(&self) -> bool {
        todo!("4e-02: whether BEGIN was run and not yet ended")
    }

    /// The isolation level of the open transaction.
    pub fn isolation_level(&self) -> Option<IsolationLevel> {
        todo!("4e-03: the level of the open transaction, None without one")
    }

    /// Did the open transaction fail? Until COMMIT or ROLLBACK every other statement is refused.
    pub fn is_failed(&self) -> bool {
        todo!("4e-04: whether the open transaction hit an error")
    }

    /// Runs the statements of `sql` in order and returns what they printed: the rows of each (one string per row, cells separated by
    /// one space), and for transaction control the tag `BEGIN`, `COMMIT` or `ROLLBACK`. Stops at the first error.
    pub fn execute(&mut self, sql: &str) -> Result<Vec<String>> {
        let _ = sql;
        todo!("4e-01: parse, run each statement in its own transaction (autocommit); the rows each printed, one string per row")
    }

    // TODO(4e-01): helpers of yours

    // TODO(4e-02): BEGIN, COMMIT and ROLLBACK

    // TODO(4e-03): isolation level names
}

impl Drop for Session<'_> {
    fn drop(&mut self) {
    }
}
