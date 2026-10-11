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
    // @begin 4e-01
    /// The open transaction, if any.
    txn: Option<Arc<Transaction>>,
    /// The open transaction hit an error: only COMMIT and ROLLBACK are accepted until it ends (4e-04).
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
    /// one space), and for transaction control the tag `BEGIN`, `COMMIT` or `ROLLBACK`. Stops at the first error.
    pub fn execute(&mut self, sql: &str) -> Result<Vec<String>> {
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
                return Err(e);
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
    fn run_statement(&mut self, statement: &Statement) -> Result<Vec<String>> {
        match statement {
            // @begin 4e-02
            Statement::Begin { isolation } => self.begin(isolation.as_deref()),
            Statement::Commit => self.commit(),
            Statement::Rollback => self.rollback(),
            // @end
            // @begin 4e-03
            Statement::Set { name, value } if name == "default_transaction_isolation" => {
                let Expr::Str(level) = value else {
                    return Err(Exception::new(ExceptionType::Invalid, "default_transaction_isolation takes a level name"));
                };
                self.default_isolation = Self::level(level)?;
                Ok(vec![])
            }
            // @end
            other => self.run_data_statement(other),
        }
    }

    fn run_data_statement(&mut self, statement: &Statement) -> Result<Vec<String>> {
        // @begin 4e-04
        if self.failed {
            return Err(Exception::new(
                ExceptionType::Execution,
                "current transaction is aborted, commands ignored until end of transaction block",
            ));
        }
        // @end
        let autocommit = self.txn.is_none();
        let txn = match &self.txn {
            Some(t) => t.clone(),
            None => self.db.txn_manager.begin(self.default_isolation)?,
        };
        let mut out = String::new();
        let outcome = self.db.execute_statements(std::slice::from_ref(statement), &mut SimpleStreamWriter::new(&mut out, true, " "), None, Some(&txn));
        let ok = matches!(outcome, Ok(true));
        if !ok {
            // a failed statement ends the transaction's usefulness: undo it at once
            let _ = self.db.txn_manager.abort(&txn);
            // @begin 4e-04
            // an explicit transaction stays "failed" until the client says COMMIT or ROLLBACK
            if !autocommit {
                self.txn = None;
                self.failed = true;
            }
            // @end
            return match outcome {
                Err(e) => Err(e),
                Ok(_) => Err(Exception::new(ExceptionType::Execution, "could not execute the statement: it conflicts with a concurrent transaction")),
            };
        }
        if autocommit {
            self.finish(&txn)?;
        }
        Ok(out.lines().map(|l| l.trim_end().to_string()).collect())
    }

    /// Commits `txn`; a refusal (a conflict, or a serializable transaction that no longer validates) is an error.
    fn finish(&mut self, txn: &Arc<Transaction>) -> Result<()> {
        if self.db.txn_manager.commit(txn)? {
            Ok(())
        } else {
            Err(Exception::new(ExceptionType::Execution, "could not commit: the transaction conflicts with a concurrent one"))
        }
    }
    //~ // TODO(4e-01): helpers of yours
    // @end

    // @begin 4e-02
    fn begin(&mut self, isolation: Option<&str>) -> Result<Vec<String>> {
        if self.in_transaction() {
            return Ok(vec!["WARNING: there is already a transaction in progress".to_string()]);
        }
        let level = match isolation {
            // @begin 4e-03
            Some(name) => Self::level(name)?,
            //~ Some(_) => return Err(Exception::new(ExceptionType::NotImplemented, "isolation levels arrive in stage 4e-03")),
            // @end
            None => self.default_isolation,
        };
        self.txn = Some(self.db.txn_manager.begin(level)?);
        Ok(vec!["BEGIN".to_string()])
    }

    fn commit(&mut self) -> Result<Vec<String>> {
        if self.failed {
            self.failed = false;
            return Ok(vec!["ROLLBACK".to_string()]);
        }
        let Some(txn) = self.txn.take() else {
            return Ok(vec!["WARNING: there is no transaction in progress".to_string()]);
        };
        self.finish(&txn)?;
        Ok(vec!["COMMIT".to_string()])
    }

    fn rollback(&mut self) -> Result<Vec<String>> {
        if self.failed {
            // the failed transaction was undone when it failed; this only ends it
            self.failed = false;
            return Ok(vec!["ROLLBACK".to_string()]);
        }
        match self.txn.take() {
            Some(txn) => {
                self.db.txn_manager.abort(&txn)?;
                Ok(vec!["ROLLBACK".to_string()])
            }
            None => Ok(vec!["WARNING: there is no transaction in progress".to_string()]),
        }
    }
    //~ // TODO(4e-02): BEGIN, COMMIT and ROLLBACK
    // @end

    // @begin 4e-03
    /// `read uncommitted`, `repeatable read` and `snapshot`, `serializable`; PostgreSQL's `read committed` has no equivalent here.
    fn level(name: &str) -> Result<IsolationLevel> {
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
