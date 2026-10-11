//! `lower(s)` and `upper(s)`.

use std::any::Any;
use std::sync::Arc;

use super::abstract_expression::{ExprRef, Expression};
use crate::catalog::column::Column;
use crate::catalog::schema::Schema;
use crate::common::exception::{Exception, ExceptionType, Result};
use crate::storage::table::tuple::Tuple;
use crate::types::type_id::TypeId;
use crate::types::value::Value;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum StringExpressionType {
    Lower,
    Upper,
}

impl StringExpressionType {
    pub fn name(self) -> &'static str {
        match self {
            StringExpressionType::Lower => "lower",
            StringExpressionType::Upper => "upper",
        }
    }
}

#[derive(Clone, Debug)]
pub struct StringExpression {
    pub expr_type: StringExpressionType,
    children: Vec<ExprRef>,
    ret_type: Column,
}

impl StringExpression {
    /// The argument must be a VARCHAR, else an `Execution` error ("unexpected arg").
    pub fn new(arg: ExprRef, expr_type: StringExpressionType) -> Result<StringExpression> {
        if arg.return_type().type_id() != TypeId::Varchar {
            return Err(Exception::new(ExceptionType::Execution, "unexpected arg"));
        }
        Ok(StringExpression { expr_type, children: vec![arg], ret_type: Column::new_varchar("<val>", 256) })
    }

    /// The lower- or upper-case form of `val`.
    pub fn compute(&self, val: &str) -> String {
        todo!("3d-03: str::to_lowercase / to_uppercase, chosen by self.expr_type")
    }

    /// The transformed string as a VARCHAR value; the VARCHAR NULL stays NULL.
    fn result(&self, val: &Value) -> Value {
        todo!("3d-03: the transformed string as a VARCHAR value; the VARCHAR NULL stays NULL")
    }
}

impl Expression for StringExpression {
    fn evaluate(&self, tuple: &Tuple, schema: &Schema) -> Result<Value> {
        let val = self.children[0].evaluate(tuple, schema)?;
        Ok(self.result(&val))
    }

    fn evaluate_join(&self, left_tuple: &Tuple, left_schema: &Schema, right_tuple: &Tuple, right_schema: &Schema) -> Result<Value> {
        let val = self.children[0].evaluate_join(left_tuple, left_schema, right_tuple, right_schema)?;
        Ok(self.result(&val))
    }

    fn children(&self) -> &[ExprRef] {
        &self.children
    }

    fn return_type(&self) -> &Column {
        &self.ret_type
    }

    fn to_string(&self) -> String {
        format!("{}({})", self.expr_type.name(), self.children[0])
    }

    fn clone_with_children(&self, children: Vec<ExprRef>) -> ExprRef {
        Arc::new(StringExpression { children, ..self.clone() })
    }

    fn as_any(&self) -> &dyn Any {
        self
    }
}
