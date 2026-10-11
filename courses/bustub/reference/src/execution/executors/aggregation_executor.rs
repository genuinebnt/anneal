//! `GROUP BY` and the aggregate functions `count(*)`, `count(x)`, `sum(x)`, `min(x)` and `max(x)`, by hashing: one pass over the child folds
//! each row into its group's running values.

use std::collections::HashMap;
use std::hash::{Hash, Hasher};

use super::abstract_executor::{Executor, ExecutorBox, BUSTUB_BATCH_SIZE};
use crate::catalog::schema::Schema;
use crate::common::exception::Result;
use crate::common::rid::Rid;
use crate::execution::expressions::abstract_expression::ExprRef;
use crate::execution::plans::plan_node::{AggregationType, PlanKind, PlanRef};
use crate::storage::table::tuple::Tuple;
use crate::types::type_id::TypeId;
use crate::types::value::Value;

/// The values of a tuple's group-by expressions: which group the tuple belongs to.
///
/// **Two keys are equal if their values are "the same group"**: unlike SQL's `=`, two NULLs are the same, and equal numbers are the same
/// whatever their integer type. `Hash` agrees with `Eq`.
#[derive(Clone, Debug)]
pub struct AggregateKey {
    pub group_bys: Vec<Value>,
}

/// A value reduced to a form on which "same group" is plain equality: (kind, integer part, float bits, text).
fn canonical(v: &Value) -> (u8, i64, u64, &str) {
    match v {
        Value::Null(_) => (0, 0, 0, ""),
        Value::Boolean(b) => (1, *b as i64, 0, ""),
        Value::TinyInt(_) | Value::SmallInt(_) | Value::Integer(_) | Value::BigInt(_) => (2, v.as_i64().unwrap(), 0, ""),
        Value::Decimal(d) => (3, 0, if *d == 0.0 { 0 } else { d.to_bits() }, ""),
        Value::Timestamp(t) => (4, *t as i64, 0, ""),
        Value::Varchar(s) => (5, 0, 0, s),
    }
}

impl PartialEq for AggregateKey {
    fn eq(&self, other: &AggregateKey) -> bool {
        // @begin 3f-01
        self.group_bys.len() == other.group_bys.len() && self.group_bys.iter().zip(&other.group_bys).all(|(a, b)| canonical(a) == canonical(b))
        //~ todo!("3f-01: the same number of values, each pair in the same group (two NULLs are the same; equal integers of any width are the same)")
        // @end
    }
}

impl Eq for AggregateKey {}

impl Hash for AggregateKey {
    fn hash<H: Hasher>(&self, state: &mut H) {
        // @begin 3f-01
        for v in &self.group_bys {
            canonical(v).hash(state);
        }
        //~ todo!("3f-01: feed the canonical form of every value to the hasher, so that keys that are equal hash alike")
        // @end
    }
}

/// The running aggregate values of one group, one per aggregate.
#[derive(Clone, Debug)]
pub struct AggregateValue {
    pub aggregates: Vec<Value>,
}

/// The hash table of groups.
pub struct SimpleAggregationHashTable {
    ht: HashMap<AggregateKey, AggregateValue>,
    agg_types: Vec<AggregationType>,
}

impl SimpleAggregationHashTable {
    pub fn new(agg_types: Vec<AggregationType>) -> SimpleAggregationHashTable {
        SimpleAggregationHashTable { ht: HashMap::new(), agg_types }
    }

    /// The value of every aggregate before any row was combined: `count(*)` starts at 0, everything else at NULL ("nothing seen yet").
    pub fn generate_initial_aggregate_value(&self) -> AggregateValue {
        let aggregates = self
            .agg_types
            .iter()
            .map(|t| match t {
                AggregationType::CountStarAggregate => Value::integer(0),
                _ => Value::null(TypeId::Integer),
            })
            .collect();
        AggregateValue { aggregates }
    }

    /// Folds one input row (the values of the aggregate expressions) into the running values.
    pub fn combine_aggregate_values(&self, result: &mut AggregateValue, input: &AggregateValue) -> Result<()> {
        // @begin 3f-01
        for (i, agg_type) in self.agg_types.iter().enumerate() {
            let (running, x) = (&result.aggregates[i], &input.aggregates[i]);
            result.aggregates[i] = match agg_type {
                AggregationType::CountStarAggregate => running.add(&Value::integer(1))?,
                _ if x.is_null() => continue,
                AggregationType::CountAggregate => {
                    if running.is_null() {
                        Value::integer(1)
                    } else {
                        running.add(&Value::integer(1))?
                    }
                }
                AggregationType::SumAggregate => {
                    if running.is_null() {
                        x.clone()
                    } else {
                        running.add(x)?
                    }
                }
                AggregationType::MinAggregate => {
                    if running.is_null() {
                        x.clone()
                    } else {
                        running.min(x)?
                    }
                }
                AggregationType::MaxAggregate => {
                    if running.is_null() {
                        x.clone()
                    } else {
                        running.max(x)?
                    }
                }
            };
        }
        Ok(())
        //~ todo!("3f-01: count(*) counts rows; the others ignore a NULL input: count counts, sum adds, min and max keep the extreme")
        // @end
    }

    /// Finds (or creates) the group of `key` and folds `value` into it.
    pub fn insert_combine(&mut self, key: AggregateKey, value: &AggregateValue) -> Result<()> {
        let initial = self.generate_initial_aggregate_value();
        let mut running = self.ht.remove(&key).unwrap_or(initial);
        self.combine_aggregate_values(&mut running, value)?;
        self.ht.insert(key, running);
        Ok(())
    }

    /// Adds a group that no row was combined into: its values are the initial values (`select count(*) from empty_table` is `0`).
    pub fn insert_initial(&mut self, key: AggregateKey) {
        let initial = self.generate_initial_aggregate_value();
        self.ht.insert(key, initial);
    }

    pub fn is_empty(&self) -> bool {
        self.ht.is_empty()
    }

    pub fn clear(&mut self) {
        self.ht.clear();
    }

    pub fn entries(&self) -> impl Iterator<Item = (&AggregateKey, &AggregateValue)> {
        self.ht.iter()
    }
}

pub struct AggregationExecutor<'e> {
    plan: PlanRef,
    child: ExecutorBox<'e>,
    group_bys: Vec<ExprRef>,
    aggregates: Vec<ExprRef>,
    table: SimpleAggregationHashTable,
    /// The output tuples, built by `init`; `cursor` is how many were handed out.
    results: Vec<Tuple>,
    cursor: usize,
}

impl<'e> AggregationExecutor<'e> {
    pub fn new(plan: PlanRef, child: ExecutorBox<'e>) -> AggregationExecutor<'e> {
        let PlanKind::Aggregation { group_bys, aggregates, agg_types } = &plan.kind else { unreachable!("an AggregationExecutor needs an Aggregation plan") };
        let (group_bys, aggregates) = (group_bys.clone(), aggregates.clone());
        let table = SimpleAggregationHashTable::new(agg_types.clone());
        AggregationExecutor { plan, child, group_bys, aggregates, table, results: vec![], cursor: 0 }
    }

    /// The group a child tuple belongs to.
    fn make_aggregate_key(&self, tuple: &Tuple) -> Result<AggregateKey> {
        // @begin 3f-02
        let mut keys = Vec::with_capacity(self.group_bys.len());
        for expr in &self.group_bys {
            keys.push(expr.evaluate(tuple, self.child.output_schema())?);
        }
        Ok(AggregateKey { group_bys: keys })
        //~ todo!("3f-02: every group-by expression evaluated on the tuple (with the child's output schema)")
        // @end
    }

    /// The values the aggregates are computed from, for a child tuple.
    fn make_aggregate_value(&self, tuple: &Tuple) -> Result<AggregateValue> {
        // @begin 3f-02
        let mut vals = Vec::with_capacity(self.aggregates.len());
        for expr in &self.aggregates {
            vals.push(expr.evaluate(tuple, self.child.output_schema())?);
        }
        Ok(AggregateValue { aggregates: vals })
        //~ todo!("3f-02: every aggregate expression evaluated on the tuple")
        // @end
    }
}

impl Executor for AggregationExecutor<'_> {
    fn init(&mut self) -> Result<()> {
        // @begin 3f-02
        self.child.init()?;
        self.table.clear();
        let (mut tuples, mut rids) = (vec![], vec![]);
        while self.child.next(&mut tuples, &mut rids, BUSTUB_BATCH_SIZE)? {
            for tuple in &tuples {
                let key = self.make_aggregate_key(tuple)?;
                let value = self.make_aggregate_value(tuple)?;
                self.table.insert_combine(key, &value)?;
            }
        }
        // `select count(*) from empty`: with no GROUP BY there is always exactly one group
        if self.table.is_empty() && self.group_bys.is_empty() {
            self.table.insert_initial(AggregateKey { group_bys: vec![] });
        }
        self.results = self
            .table
            .entries()
            .map(|(key, value)| {
                let values: Vec<Value> = key.group_bys.iter().chain(&value.aggregates).cloned().collect();
                Tuple::new(&values, &self.plan.output_schema)
            })
            .collect();
        self.cursor = 0;
        Ok(())
        //~ todo!("3f-02: one output tuple per group (group-by values, then the aggregates); with no GROUP BY an empty input still gives one group")
        // @end
    }

    fn next(&mut self, tuple_batch: &mut Vec<Tuple>, rid_batch: &mut Vec<Rid>, batch_size: usize) -> Result<bool> {
        tuple_batch.clear();
        rid_batch.clear();
        // @begin 3f-02
        while tuple_batch.len() < batch_size && self.cursor < self.results.len() {
            tuple_batch.push(self.results[self.cursor].clone());
            rid_batch.push(Rid::default());
            self.cursor += 1;
        }
        Ok(!tuple_batch.is_empty())
        //~ todo!("3f-02: hand out the next at most batch_size result tuples (and a default rid for each)")
        // @end
    }

    fn output_schema(&self) -> &Schema {
        &self.plan.output_schema
    }
}
