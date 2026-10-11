//! A join whose inner side is a table with an index on the join column. For every tuple of the outer child, the `key_predicate` gives the key, the
//! inner table's index is probed for it, and each matching inner row is output with the outer tuple. A LEFT join also outputs, once and with NULLs, an
//! outer tuple that found no inner row.

use std::collections::VecDeque;
use std::sync::Arc;

use super::abstract_executor::{Executor, ExecutorBox, TupleStream};
use super::nested_loop_join_executor::{nulls_for, values_of};
use crate::catalog::catalog::{IndexInfo, TableInfo};
use crate::catalog::schema::{Schema, SchemaRef};
use crate::common::exception::{Exception, ExceptionType, Result};
use crate::common::rid::Rid;
use crate::execution::executor_context::ExecutorContext;
use crate::execution::expressions::abstract_expression::ExprRef;
use crate::execution::plans::plan_node::{JoinType, PlanKind, PlanRef};
use crate::storage::table::tuple::Tuple;

pub struct NestedIndexJoinExecutor<'e> {
    plan: PlanRef,
    key_predicate: ExprRef,
    join_type: JoinType,
    inner_table: &'e TableInfo<'e>,
    inner_schema: SchemaRef,
    index: Arc<IndexInfo<'e>>,
    child: TupleStream<'e>,
    pending: VecDeque<Tuple>,
}

impl<'e> NestedIndexJoinExecutor<'e> {
    pub fn new(ctx: &'e ExecutorContext<'e>, plan: PlanRef, child: ExecutorBox<'e>) -> Result<NestedIndexJoinExecutor<'e>> {
        let PlanKind::NestedIndexJoin { key_predicate, inner_table_oid, index_oid, inner_table_schema, join_type, .. } = &plan.kind else {
            unreachable!("a NestedIndexJoinExecutor needs a NestedIndexJoin plan")
        };
        if !matches!(join_type, JoinType::Left | JoinType::Inner) {
            return Err(Exception::new(ExceptionType::NotImplemented, format!("join type {join_type} not supported")));
        }
        let missing = |what: &str| Exception::new(ExceptionType::Execution, format!("the {what} of an index join does not exist"));
        let inner_table = ctx.catalog.table_info(*inner_table_oid).ok_or_else(|| missing("inner table"))?;
        let index = ctx.catalog.get_index_by_oid(*index_oid).ok_or_else(|| missing("index"))?;
        let (key_predicate, join_type, inner_schema) = (key_predicate.clone(), *join_type, inner_table_schema.clone());
        Ok(NestedIndexJoinExecutor { plan, key_predicate, join_type, inner_table, inner_schema, index, child: TupleStream::new(child), pending: VecDeque::new() })
    }

    /// The live inner tuples whose index key equals the key of `outer`.
    fn probe(&self, outer: &Tuple) -> Result<Vec<Tuple>> {
        // @begin 3f-05
        let key_value = self.key_predicate.evaluate(outer, self.child.output_schema())?;
        if key_value.is_null() {
            return Ok(vec![]);
        }
        let key = Tuple::new(&[key_value], &self.index.key_schema);
        let mut found = vec![];
        for rid in self.index.index.scan_key(&key) {
            let (meta, tuple) = self.inner_table.table.get_tuple(rid)?;
            if !meta.is_deleted {
                found.push(tuple);
            }
        }
        Ok(found)
        //~ todo!("3f-05: the live inner tuples whose key equals the outer tuple's key; a NULL key matches nothing")
        // @end
    }
}

impl Executor for NestedIndexJoinExecutor<'_> {
    fn init(&mut self) -> Result<()> {
        // @begin 3f-05
        self.pending.clear();
        self.child.init()
        //~ todo!("3f-05: forget pending output and initialise the child")
        // @end
    }

    fn next(&mut self, tuple_batch: &mut Vec<Tuple>, rid_batch: &mut Vec<Rid>, batch_size: usize) -> Result<bool> {
        tuple_batch.clear();
        rid_batch.clear();
        // @begin 3f-05
        while tuple_batch.len() < batch_size {
            if let Some(t) = self.pending.pop_front() {
                tuple_batch.push(t);
                rid_batch.push(Rid::default());
                continue;
            }
            let Some((outer, _)) = self.child.next()? else { break };
            let inner_tuples = self.probe(&outer)?;
            if inner_tuples.is_empty() {
                if self.join_type == JoinType::Left {
                    let mut values = values_of(&outer, self.child.output_schema());
                    values.extend(nulls_for(&self.inner_schema));
                    self.pending.push_back(Tuple::new(&values, &self.plan.output_schema));
                }
                continue;
            }
            for inner in inner_tuples {
                let mut values = values_of(&outer, self.child.output_schema());
                values.extend(values_of(&inner, &self.inner_schema));
                self.pending.push_back(Tuple::new(&values, &self.plan.output_schema));
            }
        }
        Ok(!tuple_batch.is_empty())
        //~ todo!("3f-05: for each outer tuple: every inner match gives a row of outer then inner values; with none, a LEFT join gives the outer values and NULLs")
        // @end
    }

    fn output_schema(&self) -> &Schema {
        &self.plan.output_schema
    }
}
