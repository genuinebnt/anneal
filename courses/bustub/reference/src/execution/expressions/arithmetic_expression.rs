//! `left + right` and `left - right` on INTEGERs.

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
pub enum ArithmeticType {
    Plus,
    Minus,
}

impl ArithmeticType {
    pub fn symbol(self) -> &'static str {
        match self {
            ArithmeticType::Plus => "+",
            ArithmeticType::Minus => "-",
        }
    }
}

#[derive(Clone, Debug)]
pub struct ArithmeticExpression {
    pub compute_type: ArithmeticType,
    children: Vec<ExprRef>,
    ret_type: Column,
}

impl ArithmeticExpression {
    /// Both sides must be INTEGERs, else a `NotImplemented` error ("only support integer for now").
    pub fn new(left: ExprRef, right: ExprRef, compute_type: ArithmeticType) -> Result<ArithmeticExpression> {
        if left.return_type().type_id() != TypeId::Integer || right.return_type().type_id() != TypeId::Integer {
            return Err(Exception::new(ExceptionType::NotImplemented, "only support integer for now"));
        }
        Ok(ArithmeticExpression { compute_type, children: vec![left, right], ret_type: Column::new("<val>", TypeId::Integer) })
    }

    /// `None` (a NULL result) if either side is NULL. An overflow is an `OutOfRange` error.
    fn perform_computation(&self, lhs: &Value, rhs: &Value) -> Result<Option<i32>> {
        // @begin 3d-02
        let (Value::Integer(l), Value::Integer(r)) = (lhs, rhs) else {
            return Ok(None);
        };
        let out_of_range = || Exception::new(ExceptionType::OutOfRange, "Integer overflow.");
        let result = match self.compute_type {
            ArithmeticType::Plus => l.checked_add(*r),
            ArithmeticType::Minus => l.checked_sub(*r),
        };
        // `i32::MIN` is the stored encoding of the INTEGER NULL (module 3a), so it is not a usable result either
        Ok(Some(result.filter(|v| *v != i32::MIN).ok_or_else(out_of_range)?))
        //~ todo!("3d-02: None if either side is NULL; otherwise the sum or difference; an overflow, or a result equal to the NULL encoding, is an OutOfRange error")
        // @end
    }

    fn result(&self, lhs: &Value, rhs: &Value) -> Result<Value> {
        Ok(match self.perform_computation(lhs, rhs)? {
            Some(v) => Value::integer(v),
            None => Value::null(TypeId::Integer),
        })
    }
}

impl Expression for ArithmeticExpression {
    fn evaluate(&self, tuple: &Tuple, schema: &Schema) -> Result<Value> {
        // @begin 3d-02
        let lhs = self.children[0].evaluate(tuple, schema)?;
        let rhs = self.children[1].evaluate(tuple, schema)?;
        self.result(&lhs, &rhs)
        //~ todo!("3d-02: evaluates both children and combines them; a None result is the INTEGER NULL")
        // @end
    }

    fn evaluate_join(&self, left_tuple: &Tuple, left_schema: &Schema, right_tuple: &Tuple, right_schema: &Schema) -> Result<Value> {
        // @begin 3d-02
        let lhs = self.children[0].evaluate_join(left_tuple, left_schema, right_tuple, right_schema)?;
        let rhs = self.children[1].evaluate_join(left_tuple, left_schema, right_tuple, right_schema)?;
        self.result(&lhs, &rhs)
        //~ todo!("3d-02: the same with evaluate_join")
        // @end
    }

    fn children(&self) -> &[ExprRef] {
        &self.children
    }

    fn return_type(&self) -> &Column {
        &self.ret_type
    }

    fn to_string(&self) -> String {
        format!("({}{}{})", self.children[0], self.compute_type.symbol(), self.children[1])
    }

    fn clone_with_children(&self, children: Vec<ExprRef>) -> ExprRef {
        Arc::new(ArithmeticExpression { children, ..self.clone() })
    }

    fn as_any(&self) -> &dyn Any {
        self
    }
}
