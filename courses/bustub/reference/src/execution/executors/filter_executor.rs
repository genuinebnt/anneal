//! Passes on the child's tuples for which the predicate is TRUE; FALSE and NULL are dropped.

use super::abstract_executor::{Executor, ExecutorBox};
use crate::catalog::schema::Schema;
use crate::common::exception::Result;
use crate::common::rid::Rid;
use crate::execution::expressions::abstract_expression::ExprRef;
use crate::execution::plans::plan_node::{PlanKind, PlanRef};
use crate::storage::table::tuple::Tuple;

pub struct FilterExecutor<'e> {
    plan: PlanRef,
    child: ExecutorBox<'e>,
    pending: Vec<(Tuple, Rid)>,
    pending_offset: usize,
}

impl<'e> FilterExecutor<'e> {
    pub fn new(plan: PlanRef, child: ExecutorBox<'e>) -> FilterExecutor<'e> {
        FilterExecutor { plan, child, pending: vec![], pending_offset: 0 }
    }

    fn predicate(&self) -> &ExprRef {
        match &self.plan.kind {
            PlanKind::Filter { predicate } => predicate,
            _ => unreachable!("a FilterExecutor needs a Filter plan"),
        }
    }
}

impl Executor for FilterExecutor<'_> {
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
            let value = self.predicate().evaluate(tuple, self.child.output_schema())?;
            if value.as_bool() == Some(true) {
                tuple_batch.push(tuple.clone());
                rid_batch.push(*rid);
            }
        }
        Ok(!tuple_batch.is_empty())
    }

    fn output_schema(&self) -> &Schema {
        &self.plan.output_schema
    }
}
