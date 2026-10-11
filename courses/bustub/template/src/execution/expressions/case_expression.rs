//! `CASE WHEN c1 THEN r1 [WHEN c2 THEN r2 ...] [ELSE e] END`: the first branch whose condition is TRUE decides, and only that branch's result is
//! evaluated. `COALESCE` and `NULLIF` are CASE expressions.

use std::any::Any;
use std::sync::Arc;

use super::abstract_expression::{ExprRef, Expression};
use super::constant_value_expression::ConstantValueExpression;
use crate::catalog::column::Column;
use crate::catalog::schema::Schema;
use crate::common::exception::{Exception, ExceptionType, Result};
use crate::storage::table::tuple::Tuple;
use crate::types::type_id::TypeId;
use crate::types::value::Value;

#[derive(Clone, Debug)]
pub struct CaseExpression {
    /// `condition, result, condition, result, ..., [else]`.
    children: Vec<ExprRef>,
    branches: usize,
    has_else: bool,
    ret_type: Column,
}

impl CaseExpression {
    /// Every condition must be a BOOLEAN. The results (and the ELSE, if any) must all have one type, which is the type of the whole
    /// expression; a result that is a bare NULL literal (the engine types it INTEGER) takes that type instead of being a mismatch. A
    /// mismatch is a `MismatchType` error. At least one branch is required.
    pub fn new(branches: Vec<(ExprRef, ExprRef)>, otherwise: Option<ExprRef>) -> Result<CaseExpression> {
        todo!("3i-03: conditions must be BOOLEAN; results share one type, a NULL literal takes it; errors are MismatchType")
    }

    // TODO(3i-03): a helper of yours, if you want one
}

impl Expression for CaseExpression {
    fn evaluate(&self, tuple: &Tuple, schema: &Schema) -> Result<Value> {
        todo!("3i-03: the result of the first branch whose condition is TRUE; the ELSE (or a NULL of the result type) if none; evaluate only what you return")
    }

    fn evaluate_join(&self, left_tuple: &Tuple, left_schema: &Schema, right_tuple: &Tuple, right_schema: &Schema) -> Result<Value> {
        todo!("3i-03: the same with evaluate_join")
    }

    fn children(&self) -> &[ExprRef] {
        &self.children
    }

    fn return_type(&self) -> &Column {
        &self.ret_type
    }

    fn to_string(&self) -> String {
        let mut s = String::from("(case");
        for i in 0..self.branches {
            s.push_str(&format!(" when {} then {}", self.children[2 * i], self.children[2 * i + 1]));
        }
        if self.has_else {
            s.push_str(&format!(" else {}", self.children[2 * self.branches]));
        }
        s.push_str(" end)");
        s
    }

    fn clone_with_children(&self, children: Vec<ExprRef>) -> ExprRef {
        Arc::new(CaseExpression { children, ..self.clone() })
    }

    fn as_any(&self) -> &dyn Any {
        self
    }
}
