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
    // @begin 3i-07
    statements: Vec<Statement>,
    params: usize,
    //~ _prepared: (),
    // @end
}

impl PreparedStatement {
    /// How many `?` placeholders the SQL text has (over all its statements).
    pub fn param_count(&self) -> usize {
        // @begin 3i-07
        self.params
        //~ todo!("3i-07: the number of placeholders")
        // @end
    }
}

/// The literal the parser would have produced for `value`: an INTEGER type becomes `Expr::Integer`, DECIMAL `Expr::Float`, VARCHAR
/// `Expr::Str`, BOOLEAN `Expr::Bool`, and a NULL of any type `Expr::Null`. Other types are a `NotImplemented` error.
fn literal(value: &Value) -> Result<Expr> {
    // @begin 3i-07
    if value.is_null() {
        return Ok(Expr::Null);
    }
    match value.type_id() {
        TypeId::TinyInt | TypeId::SmallInt | TypeId::Integer | TypeId::BigInt => Ok(Expr::Integer(value.as_i64().expect("an integer"))),
        TypeId::Decimal => Ok(Expr::Float(format!("{}", value.as_f64().expect("a decimal")))),
        TypeId::Varchar => Ok(Expr::Str(value.as_str().expect("a string").to_string())),
        TypeId::Boolean => Ok(Expr::Bool(value.as_bool().expect("a boolean"))),
        other => Err(Exception::new(ExceptionType::NotImplemented, format!("a parameter of type {other:?} is not supported"))),
    }
    //~ let _ = value;
    //~ todo!("3i-07: NULL, integers, decimals, strings and booleans become the matching literal; anything else is NotImplemented")
    // @end
}

impl BusTubInstance {
    /// Parses `sql` once. `?` placeholders are numbered from the first, left to right.
    pub fn prepare(&self, sql: &str) -> Result<PreparedStatement> {
        // @begin 3i-07
        let mut statements = parser::parse(sql)?;
        let mut params = 0;
        visit_exprs_mut(&mut statements, &mut |e| {
            if matches!(e, Expr::Param(_)) {
                params += 1;
            }
        });
        Ok(PreparedStatement { statements, params })
        //~ let _ = sql;
        //~ todo!("3i-07: parse, and count the placeholders (visit_exprs_mut walks every expression)")
        // @end
    }

    /// Runs the statements with `params[i]` in place of the `i`th `?`. The number of values must equal the number of placeholders (an
    /// `Invalid` error otherwise). The prepared statement itself is not changed: it can be executed again with other values.
    pub fn execute_prepared(&self, prepared: &PreparedStatement, params: &[Value], writer: &mut dyn ResultWriter) -> Result<bool> {
        // @begin 3i-07
        if params.len() != prepared.param_count() {
            return Err(Exception::new(
                ExceptionType::Invalid,
                format!("the statement has {} parameter(s), {} value(s) were given", prepared.param_count(), params.len()),
            ));
        }
        let mut statements = prepared.statements.clone();
        let mut failure: Option<Exception> = None;
        visit_exprs_mut(&mut statements, &mut |e| {
            if let Expr::Param(i) = e {
                match literal(&params[*i]) {
                    Ok(l) => *e = l,
                    Err(err) => failure = failure.take().or(Some(err)),
                }
            }
        });
        if let Some(err) = failure {
            return Err(err);
        }
        self.execute_statements(&statements, writer, None, None)
        //~ let _ = (prepared, params, writer);
        //~ todo!("3i-07: check the number of values; replace each placeholder with the literal of its value in a copy of the statements; run them")
        // @end
    }
}
