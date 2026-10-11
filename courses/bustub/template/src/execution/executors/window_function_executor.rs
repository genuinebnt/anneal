//! Window functions (`count(*) over (...)`, `sum(x) over (partition by ... order by ...)`, `rank() over (...)`). Unlike `GROUP BY` a window function
//! keeps every row and adds a column: the aggregate over the row's *partition* (rows with equal `partition by` values), and, with an `order by`,
//! over the rows of the partition up to the current row and its **peers** (rows equal to it in the order-by key); that is the default frame.
//! Without `order by` the frame is the whole partition.
//!
//! Output: all rows. If any window function has an `order by` the rows come out in that order (the planner guarantees that all window functions of
//! one query order alike), otherwise in input order.

use std::collections::{BTreeMap, HashMap};

use super::abstract_executor::{Executor, ExecutorBox, BUSTUB_BATCH_SIZE};
use super::aggregation_executor::{AggregateKey, AggregateValue, SimpleAggregationHashTable};
use crate::catalog::schema::Schema;
use crate::common::exception::Result;
use crate::common::rid::Rid;
use crate::execution::execution_common::{generate_sort_key, SortKey, TupleComparator};
use crate::execution::expressions::abstract_expression::ExprRef;
use crate::execution::plans::plan_node::{AggregationType, PlanKind, PlanRef, WindowFunction, WindowFunctionType};
use crate::storage::table::tuple::Tuple;
use crate::types::value::Value;

pub struct WindowFunctionExecutor<'e> {
    plan: PlanRef,
    child: ExecutorBox<'e>,
    columns: Vec<ExprRef>,
    window_functions: BTreeMap<u32, WindowFunction>,
    results: Vec<Tuple>,
    cursor: usize,
}

/// The aggregate that computes a window function of an aggregate kind (`rank` is not one).
fn aggregation_of(t: WindowFunctionType) -> Option<AggregationType> {
    match t {
        WindowFunctionType::CountStarAggregate => Some(AggregationType::CountStarAggregate),
        WindowFunctionType::CountAggregate => Some(AggregationType::CountAggregate),
        WindowFunctionType::SumAggregate => Some(AggregationType::SumAggregate),
        WindowFunctionType::MinAggregate => Some(AggregationType::MinAggregate),
        WindowFunctionType::MaxAggregate => Some(AggregationType::MaxAggregate),
        WindowFunctionType::Rank => None,
    }
}

impl<'e> WindowFunctionExecutor<'e> {
    pub fn new(plan: PlanRef, child: ExecutorBox<'e>) -> WindowFunctionExecutor<'e> {
        let PlanKind::Window { columns, window_functions } = &plan.kind else { unreachable!("a WindowFunctionExecutor needs a Window plan") };
        let (columns, window_functions) = (columns.clone(), window_functions.clone());
        WindowFunctionExecutor { plan, child, columns, window_functions, results: vec![], cursor: 0 }
    }

    /// Groups the rows by the values of the window function's `partition by` expressions. Returns the rows' indexes, one list per
    /// partition, each in the order of `rows`. Without `partition by` there is one partition holding every row.
    fn partition_rows(&self, wf: &WindowFunction, rows: &[Tuple]) -> Result<Vec<Vec<usize>>> {
        todo!("3g-05: evaluate the partition-by expressions of each row into an AggregateKey; give each distinct key a partition (a Vec of row indexes), in order of first appearance, and push the row's index onto its partition")
    }

    /// The value of an aggregate window function for each row when there is **no** `order by`: every row of a partition gets the aggregate
    /// of the whole partition. Returns one value per row of `rows`.
    fn compute_whole_partitions(&self, wf: &WindowFunction, rows: &[Tuple], partitions: &[Vec<usize>]) -> Result<Vec<Value>> {
        todo!("3g-05: for each partition fold the function's value of each of its rows into one running aggregate (SimpleAggregationHashTable::new(vec![aggregation_of(..)]), generate_initial_aggregate_value, combine_aggregate_values), then give every row of the partition that final value")
    }

    /// The value of a window function for each row when there is an `order by` (`keys[i]` is row `i`'s sort key; the rows are already
    /// sorted by it): an aggregate over the partition's rows from the first up to the **last peer** of this row, or, for `rank`, the 1-based
    /// position of the row's first peer in the partition (ties share a rank and leave gaps).
    fn compute_ordered(&self, wf: &WindowFunction, rows: &[Tuple], keys: &[SortKey], partitions: &[Vec<usize>], cmp: &TupleComparator) -> Result<Vec<Value>> {
        todo!("3g-05: walk each partition in order, one group of peers at a time (rows whose keys compare Equal with cmp.compare_keys): for an aggregate fold the whole peer group into the running aggregate and give every row of the group the result; for rank give every row of the group the 1-based index of the group's first row in the partition")
    }
}

impl Executor for WindowFunctionExecutor<'_> {
    fn init(&mut self) -> Result<()> {
        self.child.init()?;
        let child_schema = self.child.output_schema().clone();
        let mut rows: Vec<Tuple> = vec![];
        let (mut tuples, mut rids) = (vec![], vec![]);
        while self.child.next(&mut tuples, &mut rids, BUSTUB_BATCH_SIZE)? {
            rows.append(&mut tuples);
        }
        // the common ORDER BY of the window functions that have one (the planner made sure they agree)
        let order_bys = self.window_functions.values().find(|wf| !wf.order_by.is_empty()).map(|wf| wf.order_by.clone()).unwrap_or_default();
        let cmp = TupleComparator::new(order_bys.clone());
        let mut keys: Vec<SortKey> = vec![];
        if !order_bys.is_empty() {
            let mut entries = Vec::with_capacity(rows.len());
            for row in rows {
                entries.push((generate_sort_key(&row, &order_bys, &child_schema)?, row));
            }
            entries.sort_by(|a, b| cmp.compare(a, b));
            (keys, rows) = entries.into_iter().unzip();
        }
        // one column of values per window function
        let mut computed: HashMap<u32, Vec<Value>> = HashMap::new();
        for (idx, wf) in &self.window_functions {
            let partitions = self.partition_rows(wf, &rows)?;
            let values = if wf.order_by.is_empty() {
                self.compute_whole_partitions(wf, &rows, &partitions)?
            } else {
                self.compute_ordered(wf, &rows, &keys, &partitions, &cmp)?
            };
            computed.insert(*idx, values);
        }
        self.results.clear();
        for (r, row) in rows.iter().enumerate() {
            let mut values = Vec::with_capacity(self.columns.len());
            for (c, expr) in self.columns.iter().enumerate() {
                values.push(match computed.get(&(c as u32)) {
                    Some(col) => col[r].clone(),
                    None => expr.evaluate(row, &child_schema)?,
                });
            }
            self.results.push(Tuple::new(&values, &self.plan.output_schema));
        }
        self.cursor = 0;
        Ok(())
    }

    fn next(&mut self, tuple_batch: &mut Vec<Tuple>, rid_batch: &mut Vec<Rid>, batch_size: usize) -> Result<bool> {
        tuple_batch.clear();
        rid_batch.clear();
        while tuple_batch.len() < batch_size && self.cursor < self.results.len() {
            tuple_batch.push(self.results[self.cursor].clone());
            rid_batch.push(Rid::default());
            self.cursor += 1;
        }
        Ok(!tuple_batch.is_empty())
    }

    fn output_schema(&self) -> &Schema {
        &self.plan.output_schema
    }
}
