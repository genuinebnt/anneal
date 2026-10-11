//! Passes on the first `limit` tuples of the child and then stops, without reading the rest.

use super::abstract_executor::{Executor, ExecutorBox};
use crate::catalog::schema::Schema;
use crate::common::exception::Result;
use crate::common::rid::Rid;
use crate::execution::plans::plan_node::{PlanKind, PlanRef};
use crate::storage::table::tuple::Tuple;

pub struct LimitExecutor<'e> {
    plan: PlanRef,
    child: ExecutorBox<'e>,
    limit: usize,
    /// How many tuples were passed on.
    emitted: usize,
}

impl<'e> LimitExecutor<'e> {
    pub fn new(plan: PlanRef, child: ExecutorBox<'e>) -> LimitExecutor<'e> {
        let PlanKind::Limit { limit } = &plan.kind else { unreachable!("a LimitExecutor needs a Limit plan") };
        let limit = *limit;
        LimitExecutor { plan, child, limit, emitted: 0 }
    }
}

impl Executor for LimitExecutor<'_> {
    fn init(&mut self) -> Result<()> {
        todo!("3g-04: start counting from zero and initialise the child")
    }

    fn next(&mut self, tuple_batch: &mut Vec<Tuple>, rid_batch: &mut Vec<Rid>, batch_size: usize) -> Result<bool> {
        tuple_batch.clear();
        rid_batch.clear();
        todo!("3g-04: false once `limit` tuples were passed on; never asks the child for, or passes on, more than are still needed")
    }

    fn output_schema(&self) -> &Schema {
        &self.plan.output_schema
    }
}
