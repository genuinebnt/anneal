//! RIGHT and FULL OUTER joins as a nested loop join that remembers which right tuples found a partner.
//!
//! A nested loop join drives from the left side, which is enough for a LEFT join: a left tuple that matched nothing is padded with NULLs. A right
//! tuple that no left tuple matched is known only once **every** left tuple has been seen, so this join keeps the right side in memory with a flag
//! per tuple and reports the unmatched ones at the end.

use std::collections::VecDeque;

use super::abstract_executor::{Executor, ExecutorBox, TupleStream};
use super::nested_loop_join_executor::{nulls_for, values_of};
use crate::catalog::schema::Schema;
use crate::common::exception::Result;
use crate::common::rid::Rid;
use crate::execution::expressions::abstract_expression::ExprRef;
use crate::execution::plans::plan_node::{JoinType, PlanKind, PlanRef};
use crate::storage::table::tuple::Tuple;

pub struct OuterJoinExecutor<'e> {
    plan: PlanRef,
    predicate: ExprRef,
    /// `Right` or `Outer`.
    join_type: JoinType,
    left: TupleStream<'e>,
    right: ExecutorBox<'e>,
    _outer: (),
}

impl<'e> OuterJoinExecutor<'e> {
    pub fn new(plan: PlanRef, left: ExecutorBox<'e>, right: ExecutorBox<'e>) -> OuterJoinExecutor<'e> {
        let PlanKind::NestedLoopJoin { predicate, join_type } = &plan.kind else { unreachable!("an OuterJoinExecutor needs a NestedLoopJoin plan") };
        let (predicate, join_type) = (predicate.clone(), *join_type);
        OuterJoinExecutor {
            plan,
            predicate,
            join_type,
            left: TupleStream::new(left),
            right,
            _outer: (),
        }
    }

    fn joined(&self, left_values: &[crate::types::value::Value], right: &Tuple, right_schema: &Schema) -> Tuple {
        let mut values = left_values.to_vec();
        values.extend(values_of(right, right_schema));
        Tuple::new(&values, &self.plan.output_schema)
    }
}

impl Executor for OuterJoinExecutor<'_> {
    fn init(&mut self) -> Result<()> {
        todo!("3j-01: initialise both children, read the whole right side into memory with a flag per tuple, forget pending output")
    }

    fn next(&mut self, tuple_batch: &mut Vec<Tuple>, rid_batch: &mut Vec<Rid>, batch_size: usize) -> Result<bool> {
        tuple_batch.clear();
        rid_batch.clear();
        todo!("3j-01: every left tuple against every right tuple, flagging the matched right tuples; an unmatched left tuple (FULL) gets NULLs on the right, and at the end unmatched right tuples get NULLs on the left")
    }

    fn output_schema(&self) -> &Schema {
        &self.plan.output_schema
    }
}
