//! Executors pull rows in the iterator (Volcano) model, in batches: `init` prepares, then `next` is called until it reports nothing more.
//!
//! Each call fills a batch of up to `batch_size` tuples and the rids they came from; a parent pulls from its children the same way.

use crate::catalog::schema::Schema;
use crate::common::exception::Result;
use crate::common::rid::Rid;
use crate::storage::table::tuple::Tuple;

/// The number of tuples an executor produces per call to `next`.
pub const BUSTUB_BATCH_SIZE: usize = 20;

pub trait Executor {
    /// Prepares to produce tuples from the start. Called once, and again whenever a parent wants the same output a second time.
    fn init(&mut self) -> Result<()>;

    /// Replaces the contents of `tuple_batch` and `rid_batch` with the next at-most-`batch_size` tuples (and the rids of their source
    /// rows). Returns `false` when there are no more tuples (the batches are then empty).
    fn next(&mut self, tuple_batch: &mut Vec<Tuple>, rid_batch: &mut Vec<Rid>, batch_size: usize) -> Result<bool>;

    /// The schema of the tuples this executor produces.
    fn output_schema(&self) -> &Schema;
}

/// A boxed executor. The lifetime is the executor context's: executors borrow the catalog.
pub type ExecutorBox<'e> = Box<dyn Executor + 'e>;

/// One tuple at a time from an executor that produces batches.
///
/// Executors that combine two inputs think in single tuples ("the next left tuple"); this wraps a child, keeps its current batch and refills it when it is used up.
pub struct TupleStream<'e> {
    child: ExecutorBox<'e>,
    batch: Vec<Tuple>,
    rids: Vec<Rid>,
    pos: usize,
}

impl<'e> TupleStream<'e> {
    pub fn new(child: ExecutorBox<'e>) -> TupleStream<'e> {
        TupleStream { child, batch: vec![], rids: vec![], pos: 0 }
    }

    /// Restarts the child (and forgets the rest of its current batch).
    pub fn init(&mut self) -> Result<()> {
        self.batch.clear();
        self.rids.clear();
        self.pos = 0;
        self.child.init()
    }

    /// The next tuple of the child and its rid, or `None` when the child is exhausted.
    pub fn next(&mut self) -> Result<Option<(Tuple, Rid)>> {
        if self.pos == self.batch.len() {
            if !self.child.next(&mut self.batch, &mut self.rids, BUSTUB_BATCH_SIZE)? {
                self.pos = 0;
                return Ok(None);
            }
            self.pos = 0;
        }
        self.pos += 1;
        Ok(Some((self.batch[self.pos - 1].clone(), self.rids[self.pos - 1])))
    }

    pub fn output_schema(&self) -> &Schema {
        self.child.output_schema()
    }
}
