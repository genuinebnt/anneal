//! A wrapper that measures another executor for `EXPLAIN ANALYZE`: it passes every call through and records, in the node's [`NodeStats`], how often
//! it was initialised, how many batches and rows it produced and how long the calls took.

use std::sync::atomic::Ordering;
use std::sync::Arc;
use std::time::Instant;

use super::abstract_executor::{Executor, ExecutorBox};
use crate::catalog::schema::Schema;
use crate::common::exception::Result;
use crate::common::rid::Rid;
use crate::execution::analyze_stats::NodeStats;
use crate::storage::table::tuple::Tuple;

pub struct ProfilingExecutor<'e> {
    inner: ExecutorBox<'e>,
    stats: Arc<NodeStats>,
}

impl<'e> ProfilingExecutor<'e> {
    pub fn new(inner: ExecutorBox<'e>, stats: Arc<NodeStats>) -> ProfilingExecutor<'e> {
        ProfilingExecutor { inner, stats }
    }
}

impl Executor for ProfilingExecutor<'_> {
    fn init(&mut self) -> Result<()> {
        // @begin 3i-06
        let started = Instant::now();
        let result = self.inner.init();
        self.stats.loops.fetch_add(1, Ordering::Relaxed);
        self.stats.nanos.fetch_add(started.elapsed().as_nanos() as u64, Ordering::Relaxed);
        result
        //~ todo!("3i-06: count the call in `stats.loops`, add the time the inner init took to `stats.nanos`, return what it returned")
        // @end
    }

    fn next(&mut self, tuple_batch: &mut Vec<Tuple>, rid_batch: &mut Vec<Rid>, batch_size: usize) -> Result<bool> {
        // @begin 3i-06
        let started = Instant::now();
        let result = self.inner.next(tuple_batch, rid_batch, batch_size);
        self.stats.nanos.fetch_add(started.elapsed().as_nanos() as u64, Ordering::Relaxed);
        if let Ok(true) = result {
            self.stats.batches.fetch_add(1, Ordering::Relaxed);
            self.stats.rows.fetch_add(tuple_batch.len(), Ordering::Relaxed);
        }
        result
        //~ todo!("3i-06: time the inner call; when it produced a batch count the batch and its rows; return what it returned")
        // @end
    }

    fn output_schema(&self) -> &Schema {
        self.inner.output_schema()
    }
}
