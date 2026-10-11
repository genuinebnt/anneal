//! `left <op> right` with SQL's three-valued answer.

use std::any::Any;
use std::sync::Arc;

use super::abstract_expression::{ExprRef, Expression};
use crate::catalog::column::Column;
use crate::catalog::schema::Schema;
use crate::common::exception::Result;
use crate::storage::table::tuple::Tuple;
use crate::types::type_id::TypeId;
use crate::types::value::{CmpBool, Value};

/// Which comparison a comparison expression performs.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ComparisonType {
    Equal,
    NotEqual,
    LessThan,
    LessThanOrEqual,
    GreaterThan,
    GreaterThanOrEqual,
}

impl ComparisonType {
    /// The text `EXPLAIN` shows.
    pub fn symbol(self) -> &'static str {
        match self {
            ComparisonType::Equal => "=",
            ComparisonType::NotEqual => "!=",
            ComparisonType::LessThan => "<",
            ComparisonType::LessThanOrEqual => "<=",
            ComparisonType::GreaterThan => ">",
            ComparisonType::GreaterThanOrEqual => ">=",
        }
    }
}

/// A [`CmpBool`] as a BOOLEAN value; `Null` becomes the BOOLEAN NULL.
pub fn cmp_to_value(b: CmpBool) -> Value {
    match b {
        CmpBool::True => Value::boolean(true),
        CmpBool::False => Value::boolean(false),
        CmpBool::Null => Value::null(TypeId::Boolean),
    }
}

#[derive(Clone, Debug)]
pub struct ComparisonExpression {
    pub comp_type: ComparisonType,
    children: Vec<ExprRef>,
    ret_type: Column,
}

impl ComparisonExpression {
    pub fn new(left: ExprRef, right: ExprRef, comp_type: ComparisonType) -> ComparisonExpression {
        ComparisonExpression { comp_type, children: vec![left, right], ret_type: Column::new("<val>", TypeId::Boolean) }
    }

    /// The three-valued result of `lhs <op> rhs`; a NULL operand gives `Null`.
    fn perform_comparison(&self, lhs: &Value, rhs: &Value) -> Result<CmpBool> {
        // @begin 3d-02
        match self.comp_type {
            ComparisonType::Equal => lhs.compare_equals(rhs),
            ComparisonType::NotEqual => lhs.compare_not_equals(rhs),
            ComparisonType::LessThan => lhs.compare_less_than(rhs),
            ComparisonType::LessThanOrEqual => lhs.compare_less_than_equals(rhs),
            ComparisonType::GreaterThan => lhs.compare_greater_than(rhs),
            ComparisonType::GreaterThanOrEqual => lhs.compare_greater_than_equals(rhs),
        }
        //~ todo!("3d-02: the Value comparison for this expression's operator")
        // @end
    }
}

impl Expression for ComparisonExpression {
    fn evaluate(&self, tuple: &Tuple, schema: &Schema) -> Result<Value> {
        // @begin 3d-02
        let lhs = self.children[0].evaluate(tuple, schema)?;
        let rhs = self.children[1].evaluate(tuple, schema)?;
        Ok(cmp_to_value(self.perform_comparison(&lhs, &rhs)?))
        //~ todo!("3d-02: evaluates both children and compares them; NULL if either side is NULL")
        // @end
    }

    fn evaluate_join(&self, left_tuple: &Tuple, left_schema: &Schema, right_tuple: &Tuple, right_schema: &Schema) -> Result<Value> {
        // @begin 3d-02
        let lhs = self.children[0].evaluate_join(left_tuple, left_schema, right_tuple, right_schema)?;
        let rhs = self.children[1].evaluate_join(left_tuple, left_schema, right_tuple, right_schema)?;
        Ok(cmp_to_value(self.perform_comparison(&lhs, &rhs)?))
        //~ todo!("3d-02: the same, with evaluate_join on the children")
        // @end
    }

    fn children(&self) -> &[ExprRef] {
        &self.children
    }

    fn return_type(&self) -> &Column {
        &self.ret_type
    }

    fn to_string(&self) -> String {
        format!("({}{}{})", self.children[0], self.comp_type.symbol(), self.children[1])
    }

    fn clone_with_children(&self, children: Vec<ExprRef>) -> ExprRef {
        Arc::new(ComparisonExpression { children, ..self.clone() })
    }

    fn as_any(&self) -> &dyn Any {
        self
    }
}
