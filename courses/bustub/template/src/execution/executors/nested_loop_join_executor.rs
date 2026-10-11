//! For every tuple of the left child, scans the whole right child and outputs the pairs for which the join predicate is TRUE. A LEFT join also
//! outputs, once and with NULLs on the right, a left tuple that matched nothing.

use super::abstract_executor::{Executor, ExecutorBox, TupleStream};
use crate::catalog::schema::Schema;
use crate::common::exception::{Exception, ExceptionType, Result};
use crate::common::rid::Rid;
use crate::execution::expressions::abstract_expression::ExprRef;
use crate::execution::plans::plan_node::{JoinType, PlanKind, PlanRef};
use crate::storage::table::tuple::Tuple;
use crate::types::value::Value;

pub struct NestedLoopJoinExecutor<'e> {
    plan: PlanRef,
    predicate: ExprRef,
    join_type: JoinType,
    left: TupleStream<'e>,
    right: TupleStream<'e>,
    /// The left tuple being joined with the whole right side, and whether any right tuple matched it yet.
    current_left: Option<Tuple>,
    matched: bool,
    /// Output tuples made but not yet handed out (one call of `next` can find more matches than fit in a batch).
    pending: std::collections::VecDeque<Tuple>,
}

/// The values of all columns of `tuple` (with `schema`), in order.
pub fn values_of(tuple: &Tuple, schema: &Schema) -> Vec<Value> {
    (0..schema.column_count()).map(|i| tuple.get_value(schema, i)).collect()
}

/// A NULL of the type of every column of `schema`: the right side of an unmatched left join row.
pub fn nulls_for(schema: &Schema) -> Vec<Value> {
    schema.columns().iter().map(|c| Value::null(c.type_id())).collect()
}

impl<'e> NestedLoopJoinExecutor<'e> {
    pub fn new(plan: PlanRef, left: ExecutorBox<'e>, right: ExecutorBox<'e>) -> Result<NestedLoopJoinExecutor<'e>> {
        let PlanKind::NestedLoopJoin { predicate, join_type } = &plan.kind else { unreachable!("a NestedLoopJoinExecutor needs a NestedLoopJoin plan") };
        if !matches!(join_type, JoinType::Left | JoinType::Inner) {
            return Err(Exception::new(ExceptionType::NotImplemented, format!("join type {join_type} not supported")));
        }
        let (predicate, join_type) = (predicate.clone(), *join_type);
        Ok(NestedLoopJoinExecutor {
            plan,
            predicate,
            join_type,
            left: TupleStream::new(left),
            right: TupleStream::new(right),
            current_left: None,
            matched: false,
            pending: Default::default(),
        })
    }

    /// The output tuple for a pair: all the left values, then all the right values.
    fn joined(&self, left: &Tuple, right: &Tuple) -> Tuple {
        let mut values = values_of(left, self.left.output_schema());
        values.extend(values_of(right, self.right.output_schema()));
        Tuple::new(&values, &self.plan.output_schema)
    }

    /// What to output when the right side is exhausted for `left`: for a LEFT join and a left tuple that matched nothing, the left
    /// values followed by a NULL for every right column; otherwise nothing.
    fn unmatched_output(&self, left: &Tuple) -> Option<Tuple> {
        None // 3f-03: for a LEFT join and a left tuple that never matched: its values, then a NULL of the right column's type for every right column
    }
}

impl Executor for NestedLoopJoinExecutor<'_> {
    fn init(&mut self) -> Result<()> {
        todo!("3f-03: initialise both children (through the TupleStreams) and forget the current left tuple, whether it matched, and any pending output")
    }

    fn next(&mut self, tuple_batch: &mut Vec<Tuple>, rid_batch: &mut Vec<Rid>, batch_size: usize) -> Result<bool> {
        tuple_batch.clear();
        rid_batch.clear();
        todo!("3f-03: until the batch is full: hand out pending output first; if there is no current left tuple take the next one (end: stop) and init the right side; take the next right tuple and, if the predicate (evaluate_join) is TRUE, queue left+right values; when the right side is exhausted queue unmatched_output(..) if it gives something and drop the left tuple")
    }

    fn output_schema(&self) -> &Schema {
        &self.plan.output_schema
    }
}
