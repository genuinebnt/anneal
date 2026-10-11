//! Wraps an executor and counts how often it is initialised and asked for tuples, to check that a nested loop join re-initialises its
//! right child for every left tuple.

use std::sync::atomic::Ordering;
use std::sync::Arc;

use super::abstract_executor::{Executor, ExecutorBox};
use crate::catalog::schema::Schema;
use crate::common::exception::Result;
use crate::common::rid::Rid;
use crate::execution::executor_context::InitCheckCounters;
use crate::execution::plans::plan_node::PlanRef;
use crate::storage::table::tuple::Tuple;

pub struct InitCheckExecutor<'e> {
    plan: PlanRef,
    child: ExecutorBox<'e>,
    counters: Arc<InitCheckCounters>,
}

impl<'e> InitCheckExecutor<'e> {
    pub fn new(plan: PlanRef, child: ExecutorBox<'e>) -> InitCheckExecutor<'e> {
        InitCheckExecutor { plan, child, counters: Arc::new(InitCheckCounters::default()) }
    }

    pub fn counters(&self) -> Arc<InitCheckCounters> {
        self.counters.clone()
    }
}

impl Executor for InitCheckExecutor<'_> {
    fn init(&mut self) -> Result<()> {
        self.counters.n_init.fetch_add(1, Ordering::SeqCst);
        self.child.init()
    }

    fn next(&mut self, tuple_batch: &mut Vec<Tuple>, rid_batch: &mut Vec<Rid>, batch_size: usize) -> Result<bool> {
        let result = self.child.next(tuple_batch, rid_batch, batch_size)?;
        if result {
            self.counters.n_next.fetch_add(1, Ordering::SeqCst);
        }
        Ok(result)
    }

    fn output_schema(&self) -> &Schema {
        &self.plan.output_schema
    }
}
