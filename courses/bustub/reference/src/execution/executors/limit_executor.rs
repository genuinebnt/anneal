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
        // @begin 3g-04
        self.emitted = 0;
        self.child.init()
        //~ todo!("3g-04: start counting from zero and initialise the child")
        // @end
    }

    fn next(&mut self, tuple_batch: &mut Vec<Tuple>, rid_batch: &mut Vec<Rid>, batch_size: usize) -> Result<bool> {
        tuple_batch.clear();
        rid_batch.clear();
        // @begin 3g-04
        if self.emitted >= self.limit {
            return Ok(false);
        }
        // ask the child for no more than we still need
        let wanted = batch_size.min(self.limit - self.emitted);
        if !self.child.next(tuple_batch, rid_batch, wanted)? {
            return Ok(false);
        }
        tuple_batch.truncate(wanted);
        rid_batch.truncate(wanted);
        self.emitted += tuple_batch.len();
        Ok(!tuple_batch.is_empty())
        //~ todo!("3g-04: false once `limit` tuples were passed on; otherwise pull a batch from the child asking for no more than are still needed, truncate it to that (a child may return more), count what is passed on")
        // @end
    }

    fn output_schema(&self) -> &Schema {
        &self.plan.output_schema
    }
}
