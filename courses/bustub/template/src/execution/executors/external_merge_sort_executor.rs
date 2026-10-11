//! `ORDER BY` for inputs that may not fit in memory.
//!
//! **Pass 0** cuts the input into *runs*, each sorted and stored on pages of the buffer pool; every later pass **merges** `K` runs into one
//! longer run, until one run, the sorted output, remains. `K` is a const generic.

use std::cmp::Ordering;

use super::abstract_executor::{Executor, ExecutorBox, BUSTUB_BATCH_SIZE};
use crate::binder::bound_order_by::OrderBy;
use crate::buffer::buffer_pool_manager::BufferPoolManager;
use crate::catalog::schema::Schema;
use crate::common::config::{PageId, BUSTUB_PAGE_SIZE};
use crate::common::exception::{Exception, ExceptionType, Result};
use crate::common::rid::Rid;
use crate::execution::execution_common::{generate_sort_key, SortEntry, TupleComparator};
use crate::execution::executor_context::ExecutorContext;
use crate::execution::plans::plan_node::{PlanKind, PlanRef};
use crate::storage::table::tuple::Tuple;

/// A page of a run: `[count: u32 LE][tuple][tuple]...`, each tuple as `Tuple::serialize_to` writes it (a 4-byte length, the bytes).
const COUNT_BYTES: usize = 4;

/// How many bytes a tuple takes in a run page.
pub fn serialized_size(tuple: &Tuple) -> usize {
    4 + tuple.data().len()
}

/// A sorted sequence of tuples stored on pages.
pub struct MergeSortRun<'e> {
    pages: Vec<PageId>,
    bpm: &'e BufferPoolManager,
}

/// Writes tuples into pages, one after the other, and gives the run that holds them.
pub struct RunBuilder<'e> {
    bpm: &'e BufferPoolManager,
    pages: Vec<PageId>,
    /// The page being filled: its bytes so far and how many tuples they hold.
    buffer: Vec<u8>,
    count: u32,
}

impl<'e> RunBuilder<'e> {
    pub fn new(bpm: &'e BufferPoolManager) -> RunBuilder<'e> {
        RunBuilder { bpm, pages: vec![], buffer: Vec::with_capacity(BUSTUB_PAGE_SIZE), count: 0 }
    }

    /// Writes the buffer into a new page of the buffer pool and starts an empty buffer.
    fn flush_page(&mut self) {
        todo!("3g-02: an empty buffer writes nothing; otherwise the count and the buffered tuples go into one new page and the buffer starts again")
    }

    /// Appends a tuple to the run. A tuple that cannot fit in a page is an error.
    pub fn push(&mut self, tuple: &Tuple) -> Result<()> {
        todo!("3g-02: an error if the tuple alone cannot fit in a page; the current page is flushed when the tuple does not fit in what is left")
    }

    /// Writes the last page and returns the run.
    pub fn finish(mut self) -> MergeSortRun<'e> {
        self.flush_page();
        MergeSortRun { pages: self.pages, bpm: self.bpm }
    }
}

impl<'e> MergeSortRun<'e> {
    /// A run holding these tuples, in this order.
    pub fn from_tuples<'t>(bpm: &'e BufferPoolManager, tuples: impl IntoIterator<Item = &'t Tuple>) -> Result<MergeSortRun<'e>> {
        let mut builder = RunBuilder::new(bpm);
        for t in tuples {
            builder.push(t)?;
        }
        Ok(builder.finish())
    }

    pub fn page_count(&self) -> usize {
        self.pages.len()
    }

    /// The tuples of page number `index` of the run, in order.
    pub fn read_page(&self, index: usize) -> Vec<Tuple> {
        todo!("3g-02: read the count, then that many tuples one after another (Tuple::deserialize_from; each takes 4 + its length bytes)")
    }

    /// All the tuples of the run, in order, one at a time.
    pub fn iter(&self) -> RunIterator<'_, 'e> {
        RunIterator { run: self, next_page: 0, current: Vec::new().into_iter() }
    }

    /// Frees the pages of the run (when its tuples have been merged into another).
    pub fn delete_pages(self) {
        for p in self.pages {
            self.bpm.delete_page(p);
        }
    }
}

/// Reads a run front to back, a page at a time.
pub struct RunIterator<'r, 'e> {
    run: &'r MergeSortRun<'e>,
    next_page: usize,
    current: std::vec::IntoIter<Tuple>,
}

impl Iterator for RunIterator<'_, '_> {
    type Item = Tuple;

    fn next(&mut self) -> Option<Tuple> {
        todo!("3g-02: the next tuple of the current page; when the page is used up load the next one (read_page); None after the last page")
    }
}

pub struct ExternalMergeSortExecutor<'e, const K: usize> {
    plan: PlanRef,
    child: ExecutorBox<'e>,
    bpm: &'e BufferPoolManager,
    order_bys: Vec<OrderBy>,
    cmp: TupleComparator,
    /// The sorted output (one run), the number of its pages already read, and the tuples of the page being read out.
    sorted: Option<MergeSortRun<'e>>,
    next_page: usize,
    buffered: std::collections::VecDeque<Tuple>,
}

impl<'e, const K: usize> ExternalMergeSortExecutor<'e, K> {
    pub fn new(ctx: &'e ExecutorContext<'e>, plan: PlanRef, child: ExecutorBox<'e>) -> ExternalMergeSortExecutor<'e, K> {
        assert!(K >= 2, "a merge needs at least two runs");
        let PlanKind::Sort { order_bys } = &plan.kind else { unreachable!("a sort executor needs a Sort plan") };
        let order_bys = order_bys.clone();
        ExternalMergeSortExecutor { plan, child, bpm: ctx.bpm, cmp: TupleComparator::new(order_bys.clone()), order_bys, sorted: None, next_page: 0, buffered: Default::default() }
    }

    /// Pass 0: reads the whole child and cuts it into sorted runs of **one page each**: collect tuples while they fit in a page, sort
    /// them (a stable sort), write them as a run.
    pub fn generate_initial_runs(&mut self) -> Result<Vec<MergeSortRun<'e>>> {
        todo!("3g-02: the whole child is cut into runs of one page each, every run sorted (a stable sort)")
    }

    /// Sorts the entries (stable) and writes them as a run, emptying `entries`.
    fn write_sorted_run(&self, entries: &mut Vec<SortEntry>) -> Result<MergeSortRun<'e>> {
        entries.sort_by(|a, b| self.cmp.compare(a, b));
        let run = MergeSortRun::from_tuples(self.bpm, entries.iter().map(|(_, t)| t))?;
        entries.clear();
        Ok(run)
    }

    /// Merges sorted runs into one sorted run: repeatedly take the smallest of the runs' current tuples. When tuples compare equal
    /// the one from the earlier run goes first (so that the whole sort is stable). The merged runs' pages are freed.
    pub fn merge_runs(&self, runs: Vec<MergeSortRun<'e>>) -> Result<MergeSortRun<'e>> {
        todo!("3g-03: the smallest current tuple of the runs goes next, the earlier run winning ties; the inputs' pages are freed")
    }
}

impl<const K: usize> Executor for ExternalMergeSortExecutor<'_, K> {
    fn init(&mut self) -> Result<()> {
        todo!("3g-03: the initial runs are merged K at a time until one run is left, which is then read from its first page; the pages of an earlier sort are freed")
    }

    fn next(&mut self, tuple_batch: &mut Vec<Tuple>, rid_batch: &mut Vec<Rid>, batch_size: usize) -> Result<bool> {
        tuple_batch.clear();
        rid_batch.clear();
        todo!("3g-03: hands out the next batch of the sorted run, a page at a time; an empty input gives nothing")
    }

    fn output_schema(&self) -> &Schema {
        &self.plan.output_schema
    }
}

impl<const K: usize> Drop for ExternalMergeSortExecutor<'_, K> {
    fn drop(&mut self) {
        if let Some(run) = self.sorted.take() {
            run.delete_pages();
        }
    }
}
