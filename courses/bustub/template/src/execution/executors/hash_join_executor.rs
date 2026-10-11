//! An equi-join by hashing. **Build**: reads the whole right child and puts each tuple in a hash table under the values of its join keys.
//! **Probe**: for each left tuple, looks up its keys and outputs the pairs. A LEFT join also outputs a left tuple with no match, once, with NULLs on the right.
//!
//! A NULL key never matches (`NULL = x` is unknown): tuples with a NULL in a key are not put in the table and match nothing when probing.

use std::collections::{HashMap, VecDeque};

use super::abstract_executor::{Executor, ExecutorBox, TupleStream};
use super::aggregation_executor::AggregateKey;
use super::nested_loop_join_executor::{nulls_for, values_of};
use crate::catalog::schema::Schema;
use crate::common::exception::{Exception, ExceptionType, Result};
use crate::common::rid::Rid;
use crate::execution::expressions::abstract_expression::ExprRef;
use crate::execution::plans::plan_node::{JoinType, PlanKind, PlanRef};
use crate::storage::table::tuple::Tuple;

pub struct HashJoinExecutor<'e> {
    plan: PlanRef,
    left_keys: Vec<ExprRef>,
    right_keys: Vec<ExprRef>,
    join_type: JoinType,
    left: TupleStream<'e>,
    right: TupleStream<'e>,
    /// The build side: the right tuples by their key.
    table: HashMap<AggregateKey, Vec<Tuple>>,
    pending: VecDeque<Tuple>,
}

impl<'e> HashJoinExecutor<'e> {
    pub fn new(plan: PlanRef, left: ExecutorBox<'e>, right: ExecutorBox<'e>) -> Result<HashJoinExecutor<'e>> {
        let PlanKind::HashJoin { left_key_expressions, right_key_expressions, join_type } = &plan.kind else { unreachable!("a HashJoinExecutor needs a HashJoin plan") };
        if !matches!(join_type, JoinType::Left | JoinType::Inner) {
            return Err(Exception::new(ExceptionType::NotImplemented, format!("join type {join_type} not supported")));
        }
        let (left_keys, right_keys, join_type) = (left_key_expressions.clone(), right_key_expressions.clone(), *join_type);
        Ok(HashJoinExecutor { plan, left_keys, right_keys, join_type, left: TupleStream::new(left), right: TupleStream::new(right), table: HashMap::new(), pending: VecDeque::new() })
    }

    /// The join key of a tuple: the key expressions evaluated on it with the schema of its side. `None` if any part is NULL.
    fn join_key(exprs: &[ExprRef], tuple: &Tuple, schema: &Schema) -> Result<Option<AggregateKey>> {
        todo!("3f-04: evaluate every key expression on the tuple; None if any value is NULL, else the AggregateKey of the values")
    }

    /// What to output for a left tuple that found no match: for a LEFT join, its values and a NULL for every right column.
    fn unmatched_output(&self, left: &Tuple) -> Option<Tuple> {
        None // 3f-04: for a LEFT join: the left values, then a NULL of the right column's type for every right column
    }
}

impl Executor for HashJoinExecutor<'_> {
    fn init(&mut self) -> Result<()> {
        todo!("3f-04: builds the table from the right child, leaving out tuples with a NULL join key")
    }

    fn next(&mut self, tuple_batch: &mut Vec<Tuple>, rid_batch: &mut Vec<Rid>, batch_size: usize) -> Result<bool> {
        tuple_batch.clear();
        rid_batch.clear();
        todo!("3f-04: for each left tuple: every right match gives a row of left then right values; no match (or a NULL key) gives the join type's unmatched row, if any")
    }

    fn output_schema(&self) -> &Schema {
        &self.plan.output_schema
    }
}
