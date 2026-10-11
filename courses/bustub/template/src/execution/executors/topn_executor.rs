//! `ORDER BY ... LIMIT n` without sorting everything: the child is read once, only the best `n` tuples seen so far are kept in a **bounded heap**,
//! and they are output in order at the end.

use std::cmp::Ordering;
use std::collections::BinaryHeap;
use std::rc::Rc;
use std::sync::atomic::{AtomicUsize, Ordering as AtomicOrdering};
use std::sync::Arc;

use super::abstract_executor::{Executor, ExecutorBox, BUSTUB_BATCH_SIZE};
use crate::binder::bound_order_by::OrderBy;
use crate::catalog::schema::Schema;
use crate::common::exception::Result;
use crate::common::rid::Rid;
use crate::execution::execution_common::{generate_sort_key, SortEntry, TupleComparator};
use crate::execution::plans::plan_node::{PlanKind, PlanRef};
use crate::storage::table::tuple::Tuple;

/// A heap element: an entry, the position at which it arrived, and the comparator. `Ord` follows the query's order, so the **greatest**
/// element of a `BinaryHeap` is the one that would come *last* in the output: the worst of the kept tuples. Of two entries that compare
/// equal the later arrival is the greater (worse), so ties keep the earlier tuple, as a stable sort followed by a limit would.
struct HeapEntry {
    entry: SortEntry,
    seq: usize,
    cmp: Rc<TupleComparator>,
}

impl PartialEq for HeapEntry {
    fn eq(&self, other: &HeapEntry) -> bool {
        self.cmp(other) == Ordering::Equal
    }
}
impl Eq for HeapEntry {}
impl PartialOrd for HeapEntry {
    fn partial_cmp(&self, other: &HeapEntry) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}
impl Ord for HeapEntry {
    fn cmp(&self, other: &HeapEntry) -> Ordering {
        self.cmp.compare(&self.entry, &other.entry).then(self.seq.cmp(&other.seq))
    }
}

pub struct TopNExecutor<'e> {
    plan: PlanRef,
    child: ExecutorBox<'e>,
    order_bys: Vec<OrderBy>,
    n: usize,
    cmp: Rc<TupleComparator>,
    /// The result in output order, and how far `next` has gone.
    result: Vec<Tuple>,
    cursor: usize,
    /// How many tuples the heap holds right now (read by the `+ensure:topn` check).
    num_in_heap: Arc<AtomicUsize>,
}

impl<'e> TopNExecutor<'e> {
    pub fn new(plan: PlanRef, child: ExecutorBox<'e>, num_in_heap: Arc<AtomicUsize>) -> TopNExecutor<'e> {
        let PlanKind::TopN { order_bys, n } = &plan.kind else { unreachable!("a TopNExecutor needs a TopN plan") };
        let (order_bys, n) = (order_bys.clone(), *n);
        let cmp = Rc::new(TupleComparator::new(order_bys.clone()));
        TopNExecutor { plan, child, order_bys, n, cmp, result: vec![], cursor: 0, num_in_heap }
    }

    /// The number of tuples in the heap: never more than `n`.
    pub fn get_num_in_heap(&self) -> usize {
        self.num_in_heap.load(AtomicOrdering::SeqCst)
    }
}

impl Executor for TopNExecutor<'_> {
    fn init(&mut self) -> Result<()> {
        todo!("3g-04: init the child; read all its batches; keep a BinaryHeap<HeapEntry> of at most n entries (push while fewer than n; otherwise replace the greatest entry when the new one is smaller); store the heap's size in self.num_in_heap after each tuple; at the end pop the heap into self.result in output order (best first)")
    }

    fn next(&mut self, tuple_batch: &mut Vec<Tuple>, rid_batch: &mut Vec<Rid>, batch_size: usize) -> Result<bool> {
        tuple_batch.clear();
        rid_batch.clear();
        todo!("3g-04: hand out the next at most batch_size tuples of self.result (and a default rid for each)")
    }

    fn output_schema(&self) -> &Schema {
        &self.plan.output_schema
    }
}

/// Wraps the child of a TopN executor when `+ensure:topn` is on and asserts, each time the TopN pulls a batch, that its heap holds no more than `n`
/// tuples: a sort followed by a limit would fail.
pub struct TopNCheckExecutor<'e> {
    plan: PlanRef,
    child: ExecutorBox<'e>,
    num_in_heap: Arc<AtomicUsize>,
    n: usize,
}

impl<'e> TopNCheckExecutor<'e> {
    pub fn new(plan: PlanRef, child: ExecutorBox<'e>, num_in_heap: Arc<AtomicUsize>) -> TopNCheckExecutor<'e> {
        let PlanKind::TopN { n, .. } = &plan.kind else { unreachable!() };
        let n = *n;
        TopNCheckExecutor { plan, child, num_in_heap, n }
    }
}

impl Executor for TopNCheckExecutor<'_> {
    fn init(&mut self) -> Result<()> {
        self.child.init()
    }

    fn next(&mut self, tuple_batch: &mut Vec<Tuple>, rid_batch: &mut Vec<Rid>, batch_size: usize) -> Result<bool> {
        assert!(self.num_in_heap.load(AtomicOrdering::SeqCst) <= self.n, "Cannot store more than N elements");
        self.child.next(tuple_batch, rid_batch, batch_size)
    }

    fn output_schema(&self) -> &Schema {
        self.child.output_schema()
    }
}
