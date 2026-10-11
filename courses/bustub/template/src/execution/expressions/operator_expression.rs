//! `*`, `/`, `%`, `||`, unary `-`, `NOT` and `IS [NOT] NULL`, and `+` and `-` on every numeric type. One expression type with an [`Operator`] tag.

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
pub enum Operator {
    Add,
    Subtract,
    Multiply,
    Divide,
    Modulo,
    /// `a || b` on strings.
    Concat,
    /// `-a`.
    Negate,
    Not,
    IsNull,
    IsNotNull,
}

impl Operator {
    /// How many operands the operator takes.
    pub fn arity(self) -> usize {
        match self {
            Operator::Negate | Operator::Not | Operator::IsNull | Operator::IsNotNull => 1,
            _ => 2,
        }
    }

    /// The operator as `EXPLAIN` prints it.
    pub fn symbol(self) -> &'static str {
        match self {
            Operator::Add => "+",
            Operator::Subtract => "-",
            Operator::Multiply => "*",
            Operator::Divide => "/",
            Operator::Modulo => "%",
            Operator::Concat => "||",
            Operator::Negate => "-",
            Operator::Not => "not ",
            Operator::IsNull => " is null",
            Operator::IsNotNull => " is not null",
        }
    }
}

#[derive(Clone, Debug)]
pub struct OperatorExpression {
    pub op: Operator,
    children: Vec<ExprRef>,
    ret_type: Column,
}

impl OperatorExpression {
    /// Checks the operand count and the operand types and works out the result type: the arithmetic operators need numbers (DECIMAL if
    /// either side is, else the wider integer type), `||` needs two strings, `NOT` a boolean; `IS [NOT] NULL` takes anything and gives a
    /// boolean. Anything else is a `NotImplemented` error that names the operator.
    pub fn new(op: Operator, children: Vec<ExprRef>) -> Result<OperatorExpression> {
        todo!("3i-01: operand count (arity), operand types, the result type; see the doc comment")
    }

    /// The value of the operator on already evaluated operands. A NULL operand gives a NULL of the result type, except for
    /// `IS [NOT] NULL`, which never gives NULL.
    fn compute(&self, args: &[Value]) -> Result<Value> {
        todo!("3i-01: a NULL operand gives NULL (except IS [NOT] NULL); arithmetic follows Value's; || concatenates; NOT flips a boolean")
    }
}

/// The type of an arithmetic result: DECIMAL if either side is, else the wider integer type.
fn widest(a: TypeId, b: TypeId) -> TypeId {
    todo!("3i-01: DECIMAL if either is DECIMAL, else the wider of the two integer types")
}

impl Expression for OperatorExpression {
    fn evaluate(&self, tuple: &Tuple, schema: &Schema) -> Result<Value> {
        todo!("3i-01: evaluate the children and compute")
    }

    fn evaluate_join(&self, left_tuple: &Tuple, left_schema: &Schema, right_tuple: &Tuple, right_schema: &Schema) -> Result<Value> {
        todo!("3i-01: the same with evaluate_join")
    }

    fn children(&self) -> &[ExprRef] {
        &self.children
    }

    fn return_type(&self) -> &Column {
        &self.ret_type
    }

    fn to_string(&self) -> String {
        match self.op.arity() {
            1 => match self.op {
                Operator::IsNull | Operator::IsNotNull => format!("({}{})", self.children[0], self.op.symbol()),
                _ => format!("({}{})", self.op.symbol(), self.children[0]),
            },
            _ => format!("({}{}{})", self.children[0], self.op.symbol(), self.children[1]),
        }
    }

    fn clone_with_children(&self, children: Vec<ExprRef>) -> ExprRef {
        Arc::new(OperatorExpression { children, ..self.clone() })
    }

    fn as_any(&self) -> &dyn Any {
        self
    }
}
