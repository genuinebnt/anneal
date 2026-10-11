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
        // @begin 3g-04
        self.child.init()?;
        self.result.clear();
        self.cursor = 0;
        self.num_in_heap.store(0, AtomicOrdering::SeqCst);
        let child_schema = self.child.output_schema().clone();
        let mut heap: BinaryHeap<HeapEntry> = BinaryHeap::new();
        let (mut tuples, mut rids) = (vec![], vec![]);
        let mut seq = 0;
        // `limit 0` needs nothing from the child
        while self.n > 0 && self.child.next(&mut tuples, &mut rids, BUSTUB_BATCH_SIZE)? {
            for tuple in tuples.drain(..) {
                let key = generate_sort_key(&tuple, &self.order_bys, &child_schema)?;
                let candidate = HeapEntry { entry: (key, tuple), seq, cmp: self.cmp.clone() };
                seq += 1;
                if heap.len() < self.n {
                    heap.push(candidate);
                } else if heap.peek().is_some_and(|worst| candidate < *worst) {
                    heap.pop();
                    heap.push(candidate);
                }
                self.num_in_heap.store(heap.len(), AtomicOrdering::SeqCst);
            }
        }
        // the heap pops the worst first: reverse to get the output order
        let mut best: Vec<Tuple> = Vec::with_capacity(heap.len());
        while let Some(e) = heap.pop() {
            best.push(e.entry.1);
        }
        best.reverse();
        self.result = best;
        Ok(())
        //~ todo!("3g-04: keeps the best n tuples of the child in a bounded heap and produces them best first")
        // @end
    }

    fn next(&mut self, tuple_batch: &mut Vec<Tuple>, rid_batch: &mut Vec<Rid>, batch_size: usize) -> Result<bool> {
        tuple_batch.clear();
        rid_batch.clear();
        // @begin 3g-04
        while tuple_batch.len() < batch_size && self.cursor < self.result.len() {
            tuple_batch.push(self.result[self.cursor].clone());
            rid_batch.push(Rid::default());
            self.cursor += 1;
        }
        Ok(!tuple_batch.is_empty())
        //~ todo!("3g-04: hands out the next batch of the result, best first")
        // @end
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
