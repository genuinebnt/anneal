//! `text LIKE pattern`: `%` matches any run of characters (including none), `_` matches exactly one character, and a backslash makes the next
//! character literal. NULL on either side gives NULL.

use std::any::Any;
use std::sync::Arc;

use super::abstract_expression::{ExprRef, Expression};
use crate::catalog::column::Column;
use crate::catalog::schema::Schema;
use crate::common::exception::{Exception, ExceptionType, Result};
use crate::storage::table::tuple::Tuple;
use crate::types::type_id::TypeId;
use crate::types::value::Value;

/// Does `text` match `pattern`? Characters, not bytes (`_` matches one `char`). A pattern that ends in a lone backslash matches a
/// backslash. The cost must be at most proportional to `text.len() * pattern.len()`: a pattern of many `%` must not take exponential time.
pub fn like_matches(text: &str, pattern: &str) -> bool {
    todo!("3i-04: % any run, _ one character, \\ escapes; linear in practice, never exponential")
}

#[derive(Clone, Debug)]
pub struct LikeExpression {
    children: Vec<ExprRef>,
    ret_type: Column,
}

impl LikeExpression {
    /// Both sides must be VARCHARs, else a `NotImplemented` error.
    pub fn new(text: ExprRef, pattern: ExprRef) -> Result<LikeExpression> {
        if text.return_type().type_id() != TypeId::Varchar || pattern.return_type().type_id() != TypeId::Varchar {
            return Err(Exception::new(ExceptionType::NotImplemented, "LIKE needs strings on both sides"));
        }
        Ok(LikeExpression { children: vec![text, pattern], ret_type: Column::new("<val>", TypeId::Boolean) })
    }

    fn result(&self, text: &Value, pattern: &Value) -> Value {
        match (text.as_str(), pattern.as_str()) {
            (Some(t), Some(p)) => Value::boolean(like_matches(t, p)),
            _ => Value::null(TypeId::Boolean),
        }
    }
}

impl Expression for LikeExpression {
    fn evaluate(&self, tuple: &Tuple, schema: &Schema) -> Result<Value> {
        todo!("3i-04: evaluate both sides; NULL if either is NULL, else whether the text matches")
    }

    fn evaluate_join(&self, left_tuple: &Tuple, left_schema: &Schema, right_tuple: &Tuple, right_schema: &Schema) -> Result<Value> {
        todo!("3i-04: the same with evaluate_join")
    }

    fn children(&self) -> &[ExprRef] {
        &self.children
    }

    fn return_type(&self) -> &Column {
        &self.ret_type
    }

    fn to_string(&self) -> String {
        format!("({} like {})", self.children[0], self.children[1])
    }

    fn clone_with_children(&self, children: Vec<ExprRef>) -> ExprRef {
        Arc::new(LikeExpression { children, ..self.clone() })
    }

    fn as_any(&self) -> &dyn Any {
        self
    }
}
