//! `left AND right` and `left OR right` in three-valued logic.

use std::any::Any;
use std::sync::Arc;

use super::abstract_expression::{ExprRef, Expression};
use super::comparison_expression::cmp_to_value;
use crate::catalog::column::Column;
use crate::catalog::schema::Schema;
use crate::common::exception::{Exception, ExceptionType, Result};
use crate::storage::table::tuple::Tuple;
use crate::types::type_id::TypeId;
use crate::types::value::{CmpBool, Value};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LogicType {
    And,
    Or,
}

impl LogicType {
    pub fn name(self) -> &'static str {
        match self {
            LogicType::And => "and",
            LogicType::Or => "or",
        }
    }
}

#[derive(Clone, Debug)]
pub struct LogicExpression {
    pub logic_type: LogicType,
    children: Vec<ExprRef>,
    ret_type: Column,
}

impl LogicExpression {
    /// Both sides must be BOOLEANs, else a `NotImplemented` error ("expect boolean from either side").
    pub fn new(left: ExprRef, right: ExprRef, logic_type: LogicType) -> Result<LogicExpression> {
        if left.return_type().type_id() != TypeId::Boolean || right.return_type().type_id() != TypeId::Boolean {
            return Err(Exception::new(ExceptionType::NotImplemented, "expect boolean from either side"));
        }
        Ok(LogicExpression { logic_type, children: vec![left, right], ret_type: Column::new("<val>", TypeId::Boolean) })
    }

    fn as_cmp_bool(val: &Value) -> CmpBool {
        match val.as_bool() {
            None => CmpBool::Null,
            Some(b) => CmpBool::from_bool(b),
        }
    }

    /// The truth table. FALSE AND anything is FALSE and TRUE OR anything is TRUE, even when the other side is NULL.
    fn perform_computation(&self, lhs: &Value, rhs: &Value) -> CmpBool {
        // @begin 3d-03
        let (l, r) = (Self::as_cmp_bool(lhs), Self::as_cmp_bool(rhs));
        match self.logic_type {
            LogicType::And => {
                if l == CmpBool::False || r == CmpBool::False {
                    CmpBool::False
                } else if l == CmpBool::True && r == CmpBool::True {
                    CmpBool::True
                } else {
                    CmpBool::Null
                }
            }
            LogicType::Or => {
                if l == CmpBool::False && r == CmpBool::False {
                    CmpBool::False
                } else if l == CmpBool::True || r == CmpBool::True {
                    CmpBool::True
                } else {
                    CmpBool::Null
                }
            }
        }
        //~ todo!("3d-03: AND: FALSE if either side is FALSE, TRUE if both are TRUE, else NULL. OR: TRUE if either side is TRUE, FALSE if both are FALSE, else NULL")
        // @end
    }
}

impl Expression for LogicExpression {
    fn evaluate(&self, tuple: &Tuple, schema: &Schema) -> Result<Value> {
        // @begin 3d-03
        let lhs = self.children[0].evaluate(tuple, schema)?;
        let rhs = self.children[1].evaluate(tuple, schema)?;
        Ok(cmp_to_value(self.perform_computation(&lhs, &rhs)))
        //~ todo!("3d-03: evaluates both sides and answers with the truth table's result as a BOOLEAN")
        // @end
    }

    fn evaluate_join(&self, left_tuple: &Tuple, left_schema: &Schema, right_tuple: &Tuple, right_schema: &Schema) -> Result<Value> {
        // @begin 3d-03
        let lhs = self.children[0].evaluate_join(left_tuple, left_schema, right_tuple, right_schema)?;
        let rhs = self.children[1].evaluate_join(left_tuple, left_schema, right_tuple, right_schema)?;
        Ok(cmp_to_value(self.perform_computation(&lhs, &rhs)))
        //~ todo!("3d-03: the same with evaluate_join")
        // @end
    }

    fn children(&self) -> &[ExprRef] {
        &self.children
    }

    fn return_type(&self) -> &Column {
        &self.ret_type
    }

    fn to_string(&self) -> String {
        format!("({}{}{})", self.children[0], self.logic_type.name(), self.children[1])
    }

    fn clone_with_children(&self, children: Vec<ExprRef>) -> ExprRef {
        Arc::new(LogicExpression { children, ..self.clone() })
    }

    fn as_any(&self) -> &dyn Any {
        self
    }
}
