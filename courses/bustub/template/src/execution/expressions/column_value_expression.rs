//! The `col_idx`th column of the `tuple_idx`th input.

use std::any::Any;
use std::sync::Arc;

use super::abstract_expression::{ExprRef, Expression};
use crate::catalog::column::Column;
use crate::catalog::schema::Schema;
use crate::common::exception::Result;
use crate::storage::table::tuple::Tuple;
use crate::types::value::Value;

#[derive(Clone, Debug)]
pub struct ColumnValueExpression {
    /// 0 = the (left) input, 1 = the right input of a join.
    tuple_idx: u32,
    /// The index of the column in that input's schema (`{A, B, C}` is `{0, 1, 2}`).
    col_idx: u32,
    ret_type: Column,
}

impl ColumnValueExpression {
    pub fn new(tuple_idx: u32, col_idx: u32, ret_type: Column) -> ColumnValueExpression {
        ColumnValueExpression { tuple_idx, col_idx, ret_type }
    }

    pub fn tuple_idx(&self) -> u32 {
        self.tuple_idx
    }

    pub fn col_idx(&self) -> u32 {
        self.col_idx
    }
}

impl Expression for ColumnValueExpression {
    fn evaluate(&self, tuple: &Tuple, schema: &Schema) -> Result<Value> {
        todo!("3d-01: the column of the tuple (module 3b's Tuple::get_value)")
    }

    fn evaluate_join(&self, left_tuple: &Tuple, left_schema: &Schema, right_tuple: &Tuple, right_schema: &Schema) -> Result<Value> {
        todo!("3d-01: tuple_idx 0 reads the left tuple, anything else the right")
    }

    fn children(&self) -> &[ExprRef] {
        &[]
    }

    fn return_type(&self) -> &Column {
        &self.ret_type
    }

    fn to_string(&self) -> String {
        format!("#{}.{}", self.tuple_idx, self.col_idx)
    }

    fn clone_with_children(&self, _children: Vec<ExprRef>) -> ExprRef {
        Arc::new(self.clone())
    }

    fn as_any(&self) -> &dyn Any {
        self
    }
}
