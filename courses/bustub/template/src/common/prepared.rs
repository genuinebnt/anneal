//! Prepared statements: `prepare` parses once and counts the `?` placeholders; `execute_prepared` puts values in their place and runs
//! the statements.
//!
//! A value becomes a *literal of the syntax tree*, never text that is parsed again, so a string that looks like SQL stays a string.

use crate::common::bustub_instance::BusTubInstance;
use crate::common::exception::{Exception, ExceptionType, Result};
use crate::common::result_writer::ResultWriter;
use crate::sql::ast::{visit_exprs_mut, Expr, Statement};
use crate::sql::parser;
use crate::types::type_id::TypeId;
use crate::types::value::Value;

pub struct PreparedStatement {
    _prepared: (),
}

impl PreparedStatement {
    /// How many `?` placeholders the SQL text has (over all its statements).
    pub fn param_count(&self) -> usize {
        todo!("3i-07: the number of placeholders")
    }
}

/// The literal the parser would have produced for `value`: an INTEGER type becomes `Expr::Integer`, DECIMAL `Expr::Float`, VARCHAR
/// `Expr::Str`, BOOLEAN `Expr::Bool`, and a NULL of any type `Expr::Null`. Other types are a `NotImplemented` error.
fn literal(value: &Value) -> Result<Expr> {
    let _ = value;
    todo!("3i-07: NULL, integers, decimals, strings and booleans become the matching literal; anything else is NotImplemented")
}

impl BusTubInstance {
    /// Parses `sql` once. `?` placeholders are numbered from the first, left to right.
    pub fn prepare(&self, sql: &str) -> Result<PreparedStatement> {
        let _ = sql;
        todo!("3i-07: parse, and count the placeholders (visit_exprs_mut walks every expression)")
    }

    /// Runs the statements with `params[i]` in place of the `i`th `?`. The number of values must equal the number of placeholders (an
    /// `Invalid` error otherwise). The prepared statement itself is not changed: it can be executed again with other values.
    pub fn execute_prepared(&self, prepared: &PreparedStatement, params: &[Value], writer: &mut dyn ResultWriter) -> Result<bool> {
        let _ = (prepared, params, writer);
        todo!("3i-07: check the number of values; replace each placeholder with the literal of its value in a copy of the statements; run them")
    }
}
