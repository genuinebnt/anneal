//! Computes the output expressions for each tuple of the child.

use super::abstract_executor::{Executor, ExecutorBox};
use crate::catalog::schema::Schema;
use crate::common::exception::Result;
use crate::common::rid::Rid;
use crate::execution::expressions::abstract_expression::ExprRef;
use crate::execution::plans::plan_node::{PlanKind, PlanRef};
use crate::storage::table::tuple::Tuple;

pub struct ProjectionExecutor<'e> {
    plan: PlanRef,
    child: ExecutorBox<'e>,
    /// Child tuples fetched but not yet projected (a batch of the child can be larger than one of ours).
    pending: Vec<(Tuple, Rid)>,
    pending_offset: usize,
}

impl<'e> ProjectionExecutor<'e> {
    pub fn new(plan: PlanRef, child: ExecutorBox<'e>) -> ProjectionExecutor<'e> {
        ProjectionExecutor { plan, child, pending: vec![], pending_offset: 0 }
    }

    fn expressions(&self) -> &[ExprRef] {
        match &self.plan.kind {
            PlanKind::Projection { expressions } => expressions,
            _ => unreachable!("a ProjectionExecutor needs a Projection plan"),
        }
    }
}

impl Executor for ProjectionExecutor<'_> {
    fn init(&mut self) -> Result<()> {
        self.pending.clear();
        self.pending_offset = 0;
        self.child.init()
    }

    fn next(&mut self, tuple_batch: &mut Vec<Tuple>, rid_batch: &mut Vec<Rid>, batch_size: usize) -> Result<bool> {
        tuple_batch.clear();
        rid_batch.clear();
        let (mut child_tuples, mut child_rids) = (vec![], vec![]);
        while tuple_batch.len() < batch_size {
            if self.pending_offset == self.pending.len() {
                if !self.child.next(&mut child_tuples, &mut child_rids, batch_size)? {
                    break;
                }
                self.pending = child_tuples.drain(..).zip(child_rids.drain(..)).collect();
                self.pending_offset = 0;
            }
            let (tuple, rid) = &self.pending[self.pending_offset];
            self.pending_offset += 1;
            let mut values = Vec::with_capacity(self.expressions().len());
            for expr in self.expressions() {
                values.push(expr.evaluate(tuple, self.child.output_schema())?);
            }
            tuple_batch.push(Tuple::new(&values, &self.plan.output_schema));
            rid_batch.push(*rid);
        }
        Ok(!tuple_batch.is_empty())
    }

    fn output_schema(&self) -> &Schema {
        &self.plan.output_schema
    }
}
