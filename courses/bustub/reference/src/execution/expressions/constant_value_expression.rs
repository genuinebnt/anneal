//! A literal.

use std::any::Any;
use std::sync::Arc;

use super::abstract_expression::{ExprRef, Expression};
use crate::catalog::column::Column;
use crate::catalog::schema::Schema;
use crate::common::exception::Result;
use crate::storage::table::tuple::Tuple;
use crate::types::type_id::TypeId;
use crate::types::value::Value;

#[derive(Clone, Debug)]
pub struct ConstantValueExpression {
    pub val: Value,
    ret_type: Column,
}

/// The column that describes a value: `<val>` of its type (a string gets the maximum length the planner uses for strings).
pub fn column_of(val: &Value) -> Column {
    if val.type_id() == TypeId::Varchar {
        Column::new_varchar("<val>", 256)
    } else {
        Column::new("<val>", val.type_id())
    }
}

impl ConstantValueExpression {
    pub fn new(val: Value) -> ConstantValueExpression {
        let ret_type = column_of(&val);
        ConstantValueExpression { val, ret_type }
    }
}

impl Expression for ConstantValueExpression {
    fn evaluate(&self, tuple: &Tuple, schema: &Schema) -> Result<Value> {
        // @begin 3d-01
        Ok(self.val.clone())
        //~ todo!("3d-01: the literal, whatever the tuple is")
        // @end
    }

    fn evaluate_join(&self, left_tuple: &Tuple, left_schema: &Schema, right_tuple: &Tuple, right_schema: &Schema) -> Result<Value> {
        // @begin 3d-01
        Ok(self.val.clone())
        //~ todo!("3d-01: the literal again")
        // @end
    }

    fn children(&self) -> &[ExprRef] {
        &[]
    }

    fn return_type(&self) -> &Column {
        &self.ret_type
    }

    fn to_string(&self) -> String {
        self.val.to_string()
    }

    fn clone_with_children(&self, _children: Vec<ExprRef>) -> ExprRef {
        Arc::new(self.clone())
    }

    fn as_any(&self) -> &dyn Any {
        self
    }
}
