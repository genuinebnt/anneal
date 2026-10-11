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
    // @begin 3i-05
    /// How many tuples were dropped so far.
    skipped: usize,
    //~ _offset: (),
    // @end
}

impl<'e> OffsetExecutor<'e> {
    pub fn new(plan: PlanRef, child: ExecutorBox<'e>) -> OffsetExecutor<'e> {
        let PlanKind::Offset { offset } = &plan.kind else { unreachable!("an OffsetExecutor needs an Offset plan") };
        let offset = *offset;
        // @begin 3i-05
        OffsetExecutor { plan, child, offset, skipped: 0 }
        //~ OffsetExecutor { plan, child, offset, _offset: () }
        // @end
    }
}

impl Executor for OffsetExecutor<'_> {
    fn init(&mut self) -> Result<()> {
        // @begin 3i-05
        self.skipped = 0;
        self.child.init()
        //~ todo!("3i-05: start counting from zero and initialise the child")
        // @end
    }

    fn next(&mut self, tuple_batch: &mut Vec<Tuple>, rid_batch: &mut Vec<Rid>, batch_size: usize) -> Result<bool> {
        tuple_batch.clear();
        rid_batch.clear();
        // @begin 3i-05
        while self.skipped < self.offset {
            let (mut tuples, mut rids) = (Vec::new(), Vec::new());
            if !self.child.next(&mut tuples, &mut rids, batch_size)? {
                return Ok(false);
            }
            let drop = (self.offset - self.skipped).min(tuples.len());
            self.skipped += drop;
            if drop < tuples.len() {
                // the batch that crosses the offset: pass on what is left of it
                tuple_batch.extend(tuples.drain(drop..));
                rid_batch.extend(rids.drain(drop.min(rids.len())..));
                return Ok(true);
            }
        }
        self.child.next(tuple_batch, rid_batch, batch_size)
        //~ todo!("3i-05: drop the first `offset` tuples (a batch may cross the offset), then pass the child's batches on unchanged")
        // @end
    }

    fn output_schema(&self) -> &Schema {
        &self.plan.output_schema
    }
}
