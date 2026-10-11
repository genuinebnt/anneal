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
        // @begin 3i-03
        if branches.is_empty() {
            return Err(Exception::new(ExceptionType::Invalid, "CASE needs at least one WHEN"));
        }
        let is_null_literal = |e: &ExprRef| e.as_any().downcast_ref::<ConstantValueExpression>().is_some_and(|c| c.val.is_null());
        let mut results: Vec<ExprRef> = branches.iter().map(|(_, r)| r.clone()).collect();
        results.extend(otherwise.clone());
        // the type of the CASE: the first result that is not a bare NULL (a CASE of nothing but NULLs is INTEGER)
        let ret = results.iter().find(|r| !is_null_literal(r)).map(|r| r.return_type().with_column_name("<val>")).unwrap_or_else(|| Column::new("<val>", TypeId::Integer));
        let mut children = Vec::new();
        for (cond, result) in &branches {
            if cond.return_type().type_id() != TypeId::Boolean {
                return Err(Exception::new(ExceptionType::MismatchType, "argument of CASE/WHEN must be type boolean"));
            }
            children.push(cond.clone());
            children.push(Self::unify(result, &ret, &is_null_literal)?);
        }
        let has_else = otherwise.is_some();
        if let Some(e) = &otherwise {
            children.push(Self::unify(e, &ret, &is_null_literal)?);
        }
        Ok(CaseExpression { children, branches: branches.len(), has_else, ret_type: ret })
        //~ todo!("3i-03: conditions must be BOOLEAN; results share one type, a NULL literal takes it; errors are MismatchType")
        // @end
    }

    // @begin 3i-03
    fn unify(result: &ExprRef, ret: &Column, is_null_literal: &dyn Fn(&ExprRef) -> bool) -> Result<ExprRef> {
        if is_null_literal(result) {
            return Ok(Arc::new(ConstantValueExpression::new(Value::null(ret.type_id()))));
        }
        if result.return_type().type_id() != ret.type_id() {
            return Err(Exception::new(
                ExceptionType::MismatchType,
                format!("CASE types {:?} and {:?} cannot be matched", ret.type_id(), result.return_type().type_id()),
            ));
        }
        Ok(result.clone())
    }

    fn pick<F: Fn(&ExprRef) -> Result<Value>>(&self, eval: F) -> Result<Value> {
        for i in 0..self.branches {
            if eval(&self.children[2 * i])?.as_bool() == Some(true) {
                return eval(&self.children[2 * i + 1]);
            }
        }
        if self.has_else {
            return eval(&self.children[2 * self.branches]);
        }
        Ok(Value::null(self.ret_type.type_id()))
    }
    //~ // TODO(3i-03): a helper of yours, if you want one
    // @end
}

impl Expression for CaseExpression {
    fn evaluate(&self, tuple: &Tuple, schema: &Schema) -> Result<Value> {
        // @begin 3i-03
        self.pick(|e| e.evaluate(tuple, schema))
        //~ todo!("3i-03: the result of the first branch whose condition is TRUE, else the ELSE (or a NULL of the result type); only the chosen result is evaluated")
        // @end
    }

    fn evaluate_join(&self, left_tuple: &Tuple, left_schema: &Schema, right_tuple: &Tuple, right_schema: &Schema) -> Result<Value> {
        // @begin 3i-03
        self.pick(|e| e.evaluate_join(left_tuple, left_schema, right_tuple, right_schema))
        //~ todo!("3i-03: the same with evaluate_join")
        // @end
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
