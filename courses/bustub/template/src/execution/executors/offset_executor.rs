//! `OFFSET n`: drops the first `n` tuples of the child and passes on the rest.

use super::abstract_executor::{Executor, ExecutorBox};
use crate::catalog::schema::Schema;
use crate::common::exception::Result;
use crate::common::rid::Rid;
use crate::execution::plans::plan_node::{PlanKind, PlanRef};
use crate::storage::table::tuple::Tuple;

pub struct OffsetExecutor<'e> {
    plan: PlanRef,
    child: ExecutorBox<'e>,
    offset: usize,
    _offset: (),
}

impl<'e> OffsetExecutor<'e> {
    pub fn new(plan: PlanRef, child: ExecutorBox<'e>) -> OffsetExecutor<'e> {
        let PlanKind::Offset { offset } = &plan.kind else { unreachable!("an OffsetExecutor needs an Offset plan") };
        let offset = *offset;
        OffsetExecutor { plan, child, offset, _offset: () }
    }
}

impl Executor for OffsetExecutor<'_> {
    fn init(&mut self) -> Result<()> {
        todo!("3i-05: start counting from zero and initialise the child")
    }

    fn next(&mut self, tuple_batch: &mut Vec<Tuple>, rid_batch: &mut Vec<Rid>, batch_size: usize) -> Result<bool> {
        tuple_batch.clear();
        rid_batch.clear();
        todo!("3i-05: drop the first `offset` tuples (a batch may cross the offset), then pass the child's batches on unchanged")
    }

    fn output_schema(&self) -> &Schema {
        &self.plan.output_schema
    }
}
