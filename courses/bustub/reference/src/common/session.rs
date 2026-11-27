//! A session: one client's connection to the database, with the transaction it has open, if any.
//!
//! A statement runs in the session's own transaction when none is open (autocommit), or in the one `BEGIN` started.

use std::sync::Arc;

use crate::common::bustub_instance::BusTubInstance;
use crate::common::db_error::{Context, DbResult};
use crate::common::exception::{Exception, ExceptionType};
use crate::common::result_writer::SimpleStreamWriter;
use crate::concurrency::transaction::{IsolationLevel, Transaction};
use crate::sql::ast::{Expr, Statement};
use crate::sql::parser;

// @begin 4e-04
/// A transaction outcome a client has to act on, with the SQLSTATE code it is reported under.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TxnError {
    /// A concurrent transaction wrote the same row first.
    WriteConflict,
    /// The commit was refused: a transaction that committed meanwhile conflicts with this one.
    CommitRefused,
    /// A statement other than COMMIT or ROLLBACK in a failed transaction.
    InFailedTransaction,
    /// COMMIT or ROLLBACK with no transaction open: a warning, not an error.
    NoTransaction,
    /// BEGIN inside a transaction: a warning, not an error.
    AlreadyInTransaction,
}

impl TxnError {
    /// `40001` for a conflict (the whole transaction may succeed if run again), `25P02` in a failed transaction, `25P01` with no
    /// transaction open and `25001` inside one.
    pub fn code(self) -> &'static str {
        match self {
            TxnError::WriteConflict | TxnError::CommitRefused => "40001",
            TxnError::InFailedTransaction => "25P02",
            TxnError::NoTransaction => "25P01",
            TxnError::AlreadyInTransaction => "25001",
        }
    }

    /// The reply to a statement that only warns: `WARNING <code>: <message>`.
    fn warning(self) -> Vec<String> {
        vec![format!("WARNING {}: {self}", self.code())]
    }
}

impl std::fmt::Display for TxnError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            TxnError::WriteConflict => "could not serialize access due to concurrent update",
            TxnError::CommitRefused => "could not serialize access: a concurrent transaction committed a conflicting change",
            TxnError::InFailedTransaction => "current transaction is aborted, commands ignored until end of transaction block",
            TxnError::NoTransaction => "there is no transaction in progress",
            TxnError::AlreadyInTransaction => "there is already a transaction in progress",
        })
    }
}

impl std::error::Error for TxnError {}
//~ // TODO(4e-04): the outcomes of a transaction a client must act on, with their codes
// @end

pub struct Session<'a> {
    db: &'a BusTubInstance,
    // @begin 4e-01
    /// The open transaction, if any.
    txn: Option<Arc<Transaction>>,
    /// The open transaction hit an error: only COMMIT and ROLLBACK are accepted until it ends.
    failed: bool,
    /// What `BEGIN` and autocommit use when no level is named.
    default_isolation: IsolationLevel,
    //~ _session: (),
    // @end
}

impl<'a> Session<'a> {
    /// A session with no open transaction.
    pub fn new(db: &'a BusTubInstance) -> Session<'a> {
        // @begin 4e-01
        Session { db, txn: None, failed: false, default_isolation: IsolationLevel::SnapshotIsolation }
        //~ let _ = db;
        //~ todo!("4e-01: a session with no open transaction")
        // @end
    }

    /// Is an explicit transaction open (a failed one counts: it is open until COMMIT or ROLLBACK)?
    pub fn in_transaction(&self) -> bool {
        // @begin 4e-02
        self.txn.is_some() || self.failed
        //~ todo!("4e-02: whether BEGIN was run and not yet ended")
        // @end
    }

    /// The isolation level of the open transaction.
    pub fn isolation_level(&self) -> Option<IsolationLevel> {
        // @begin 4e-03
        self.txn.as_ref().map(|t| t.isolation_level())
        //~ todo!("4e-03: the level of the open transaction, None without one")
        // @end
    }

    /// Did the open transaction fail? Until COMMIT or ROLLBACK every other statement is refused.
    pub fn is_failed(&self) -> bool {
        // @begin 4e-04
        self.failed
        //~ todo!("4e-04: whether the open transaction hit an error")
        // @end
    }

    /// Runs the statements of `sql` in order and returns what they printed: the rows of each (one string per row, cells separated by
    /// one space), for transaction control the tag `BEGIN`, `COMMIT` or `ROLLBACK`, and for a stray one a line
    /// `WARNING <code>: <message>`. Stops at the first error.
    ///
    /// # Errors
    ///
    /// The error of the first statement that fails, with its SQLSTATE code: `40001` for a conflict with a concurrent transaction
    /// (running the transaction again may succeed) and `25P02` for a statement in a failed transaction.
    pub fn execute(&mut self, sql: &str) -> DbResult<Vec<String>> {
        // @begin 4e-01
        let statements = match parser::parse(sql) {
            Ok(s) => s,
            Err(e) => {
                // @begin 4e-04
                // PostgreSQL: any error, a syntax error included, fails the open transaction
                if let Some(txn) = self.txn.take() {
                    let _ = self.db.txn_manager.abort(&txn);
                    self.failed = true;
                }
                // @end
                return Err(e.into());
            }
        };
        let mut lines = Vec::new();
        for statement in &statements {
            lines.extend(self.run_statement(statement)?);
        }
        Ok(lines)
        //~ let _ = sql;
        //~ todo!("4e-01: parse, run each statement in its own transaction (autocommit); the rows each printed, one string per row")
        // @end
    }

    // @begin 4e-01
    fn run_statement(&mut self, statement: &Statement) -> DbResult<Vec<String>> {
        match statement {
            // @begin 4e-02
            Statement::Begin { isolation } => self.begin(isolation.as_deref()),
            Statement::Commit => self.commit(),
            Statement::Rollback => self.rollback(),
            // @end
            // @begin 4e-03
            Statement::Set { name, value } if name == "default_transaction_isolation" => {
                let Expr::Str(level) = value else {
                    return Err(Exception::new(ExceptionType::Invalid, "default_transaction_isolation takes a level name").into());
                };
                self.default_isolation = Self::level(level)?;
                Ok(vec![])
            }
            // @end
            other => self.run_data_statement(other),
        }
    }

    fn run_data_statement(&mut self, statement: &Statement) -> DbResult<Vec<String>> {
        // @begin 4e-04
        if self.failed {
            return Err(TxnError::InFailedTransaction.into());
        }
        // @end
        let autocommit = self.txn.is_none();
        let txn = match &self.txn {
            Some(t) => t.clone(),
            None => self.db.txn_manager.begin(self.default_isolation).context("beginning a transaction")?,
        };
        let mut out = String::new();
        let outcome = self.db.execute_statements(
            std::slice::from_ref(statement),
            &mut SimpleStreamWriter::new(&mut out, true, " "),
            None,
            Some(&txn),
        );
        if !matches!(outcome, Ok(true)) {
            // a failed statement ends the transaction's usefulness: undo it at once
            let _ = self.db.txn_manager.abort(&txn);
            // @begin 4e-04
            // an explicit transaction stays "failed" until the client says COMMIT or ROLLBACK
            if !autocommit {
                self.txn = None;
                self.failed = true;
            }
            // @end
            return Err(match outcome {
                // @begin 4e-04
                Ok(_) => TxnError::WriteConflict.into(),
                //~ Ok(_) => Exception::new(ExceptionType::Execution, "could not execute the statement: it conflicts with a concurrent transaction").into(),
                // @end
                Err(e) => e.into(),
            });
        }
        if autocommit {
            self.finish(&txn)?;
        }
        Ok(out.lines().map(|l| l.trim_end().to_string()).collect())
    }

    /// Commits `txn`; a refusal (a conflict, or a serializable transaction that no longer validates) is an error.
    fn finish(&mut self, txn: &Arc<Transaction>) -> DbResult<()> {
        if self.db.txn_manager.commit(txn).context("committing a transaction")? {
            Ok(())
        } else {
            // @begin 4e-04
            Err(TxnError::CommitRefused.into())
            //~ Err(Exception::new(ExceptionType::Execution, "could not commit: the transaction conflicts with a concurrent one").into())
            // @end
        }
    }
    //~ // TODO(4e-01): helpers of yours
    // @end

    // @begin 4e-02
    fn begin(&mut self, isolation: Option<&str>) -> DbResult<Vec<String>> {
        if self.in_transaction() {
            // @begin 4e-04
            return Ok(TxnError::AlreadyInTransaction.warning());
            //~ return Ok(vec!["WARNING 25001: there is already a transaction in progress".to_string()]);
            // @end
        }
        let level = match isolation {
            // @begin 4e-03
            Some(name) => Self::level(name)?,
            //~ Some(_) => return Err(Exception::new(ExceptionType::NotImplemented, "isolation levels arrive in stage 4e-03").into()),
            // @end
            None => self.default_isolation,
        };
        self.txn = Some(self.db.txn_manager.begin(level).context("beginning a transaction")?);
        Ok(vec!["BEGIN".to_string()])
    }

    fn commit(&mut self) -> DbResult<Vec<String>> {
        if self.failed {
            self.failed = false;
            return Ok(vec!["ROLLBACK".to_string()]);
        }
        let Some(txn) = self.txn.take() else {
            // @begin 4e-04
            return Ok(TxnError::NoTransaction.warning());
            //~ return Ok(vec!["WARNING 25P01: there is no transaction in progress".to_string()]);
            // @end
        };
        self.finish(&txn)?;
        Ok(vec!["COMMIT".to_string()])
    }

    fn rollback(&mut self) -> DbResult<Vec<String>> {
        if self.failed {
            // the failed transaction was undone when it failed; this only ends it
            self.failed = false;
            return Ok(vec!["ROLLBACK".to_string()]);
        }
        let Some(txn) = self.txn.take() else {
            // @begin 4e-04
            return Ok(TxnError::NoTransaction.warning());
            //~ return Ok(vec!["WARNING 25P01: there is no transaction in progress".to_string()]);
            // @end
        };
        self.db.txn_manager.abort(&txn).context("rolling back a transaction")?;
        Ok(vec!["ROLLBACK".to_string()])
    }
    //~ // TODO(4e-02): BEGIN, COMMIT and ROLLBACK
    // @end

    // @begin 4e-03
    /// `read uncommitted`, `repeatable read` and `snapshot`, `serializable`; PostgreSQL's `read committed` has no equivalent here.
    fn level(name: &str) -> Result<IsolationLevel, Exception> {
        match name.trim().to_lowercase().as_str() {
            "read uncommitted" => Ok(IsolationLevel::ReadUncommitted),
            "repeatable read" | "snapshot" | "snapshot isolation" => Ok(IsolationLevel::SnapshotIsolation),
            "serializable" => Ok(IsolationLevel::Serializable),
            other => Err(Exception::new(ExceptionType::NotImplemented, format!("isolation level {other:?} is not supported"))),
        }
    }
    //~ // TODO(4e-03): isolation level names
    // @end
}

impl Drop for Session<'_> {
    fn drop(&mut self) {
        // @begin 4e-02
        // a client that goes away mid-transaction rolls it back: a running transaction would hold the watermark (and so the garbage
        // collector) back for ever
        if let Some(txn) = self.txn.take() {
            let _ = self.db.txn_manager.abort(&txn);
        }
        // @end
    }
}
