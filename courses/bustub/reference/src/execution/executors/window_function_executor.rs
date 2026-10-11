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
        // @begin 3g-05
        let schema = self.child.output_schema();
        let mut index_of: HashMap<AggregateKey, usize> = HashMap::new();
        let mut partitions: Vec<Vec<usize>> = vec![];
        for (i, row) in rows.iter().enumerate() {
            let mut key = Vec::with_capacity(wf.partition_by.len());
            for e in &wf.partition_by {
                key.push(e.evaluate(row, schema)?);
            }
            let slot = *index_of.entry(AggregateKey { group_bys: key }).or_insert_with(|| {
                partitions.push(vec![]);
                partitions.len() - 1
            });
            partitions[slot].push(i);
        }
        Ok(partitions)
        //~ todo!("3g-05: one list of row indexes per distinct partition key, in order of first appearance")
        // @end
    }

    /// The value of an aggregate window function for each row when there is **no** `order by`: every row of a partition gets the aggregate
    /// of the whole partition. Returns one value per row of `rows`.
    fn compute_whole_partitions(&self, wf: &WindowFunction, rows: &[Tuple], partitions: &[Vec<usize>]) -> Result<Vec<Value>> {
        // @begin 3g-05
        let agg_type = aggregation_of(wf.func_type).expect("rank needs an order by");
        let table = SimpleAggregationHashTable::new(vec![agg_type]);
        let schema = self.child.output_schema();
        let mut out = vec![Value::null(crate::types::type_id::TypeId::Integer); rows.len()];
        for partition in partitions {
            let mut running = table.generate_initial_aggregate_value();
            for &i in partition {
                let input = AggregateValue { aggregates: vec![wf.function.evaluate(&rows[i], schema)?] };
                table.combine_aggregate_values(&mut running, &input)?;
            }
            for &i in partition {
                out[i] = running.aggregates[0].clone();
            }
        }
        Ok(out)
        //~ todo!("3g-05: every row of a partition gets the aggregate of the whole partition")
        // @end
    }

    /// The value of a window function for each row when there is an `order by` (`keys[i]` is row `i`'s sort key; the rows are already
    /// sorted by it): an aggregate over the partition's rows from the first up to the **last peer** of this row, or, for `rank`, the 1-based
    /// position of the row's first peer in the partition (ties share a rank and leave gaps).
    fn compute_ordered(&self, wf: &WindowFunction, rows: &[Tuple], keys: &[SortKey], partitions: &[Vec<usize>], cmp: &TupleComparator) -> Result<Vec<Value>> {
        // @begin 3g-05
        let schema = self.child.output_schema();
        let table = aggregation_of(wf.func_type).map(|t| SimpleAggregationHashTable::new(vec![t]));
        let mut out = vec![Value::null(crate::types::type_id::TypeId::Integer); rows.len()];
        for partition in partitions {
            let mut running = table.as_ref().map(|t| t.generate_initial_aggregate_value());
            let mut start = 0;
            while start < partition.len() {
                // the peers of the row at `start`: following rows whose sort key is equal
                let mut end = start + 1;
                while end < partition.len() && cmp.compare_keys(&keys[partition[start]], &keys[partition[end]]) == std::cmp::Ordering::Equal {
                    end += 1;
                }
                let value = match (&table, running.as_mut()) {
                    (Some(table), Some(running)) => {
                        for &i in &partition[start..end] {
                            let input = AggregateValue { aggregates: vec![wf.function.evaluate(&rows[i], schema)?] };
                            table.combine_aggregate_values(running, &input)?;
                        }
                        running.aggregates[0].clone()
                    }
                    _ => Value::integer(start as i32 + 1), // rank
                };
                for &i in &partition[start..end] {
                    out[i] = value.clone();
                }
                start = end;
            }
        }
        Ok(out)
        //~ todo!("3g-05: rows with equal sort keys are peers: an aggregate covers the partition up to the last peer; rank is the 1-based position of the first peer")
        // @end
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
