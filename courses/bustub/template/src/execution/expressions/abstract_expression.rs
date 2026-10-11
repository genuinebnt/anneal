//! An expression is a tree: every node has children and, given a tuple, computes a [`Value`]. Expressions are a trait and are shared as
//! `Arc<dyn Expression>`.

use std::any::Any;
use std::fmt;
use std::sync::Arc;

use crate::catalog::column::Column;
use crate::catalog::schema::Schema;
use crate::common::exception::Result;
use crate::storage::table::tuple::Tuple;
use crate::types::value::Value;

/// A shared expression node. Cloning it clones the pointer, not the tree.
pub type ExprRef = Arc<dyn Expression>;

pub trait Expression: Send + Sync + fmt::Debug {
    /// The value of this expression for one tuple whose layout is `schema`.
    fn evaluate(&self, tuple: &Tuple, schema: &Schema) -> Result<Value>;

    /// The value for a **join**: `left_tuple` and `right_tuple` are the two inputs (a column reference says which one it reads).
    fn evaluate_join(&self, left_tuple: &Tuple, left_schema: &Schema, right_tuple: &Tuple, right_schema: &Schema) -> Result<Value>;

    /// The children of this node; their order can matter (`a - b`).
    fn children(&self) -> &[ExprRef];

    /// The `idx`th child.
    fn child_at(&self, idx: usize) -> &ExprRef {
        &self.children()[idx]
    }

    /// The type (and name) of the value this expression produces.
    fn return_type(&self) -> &Column;

    /// A text form for `EXPLAIN`: `(#0.0>5)`, `lower(#0.1)`.
    fn to_string(&self) -> String;

    /// The same node with other children. (The optimizer rewrites trees by building new ones; nodes are never mutated.)
    fn clone_with_children(&self, children: Vec<ExprRef>) -> ExprRef;

    /// The optimizer's `dynamic_cast`: `expr.as_any().downcast_ref::<ComparisonExpression>()`.
    fn as_any(&self) -> &dyn Any;
}

impl fmt::Display for dyn Expression {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&Expression::to_string(self))
    }
}
