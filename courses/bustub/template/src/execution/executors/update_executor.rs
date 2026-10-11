//! Replaces every tuple its child produces by a new one made of the plan's `target_expressions`, evaluated on the old tuple, and produces one row:
//! how many it updated.
//!
//! An update is a **delete followed by an insert**: the old tuple is marked deleted and the new one is stored under a new rid (it may have another
//! size), and the table's indexes follow.

use std::sync::Arc;

use super::abstract_executor::{Executor, ExecutorBox, BUSTUB_BATCH_SIZE};
use crate::catalog::catalog::{IndexInfo, TableInfo};
use crate::catalog::schema::Schema;
use crate::common::exception::{Exception, ExceptionType, Result};
use crate::common::rid::Rid;
use crate::concurrency::transaction::Transaction;
use crate::concurrency::transaction_manager::TransactionManager;
use crate::execution::execution_common::{insert_mvcc, modify_tuple};
use crate::execution::executor_context::ExecutorContext;
use crate::execution::expressions::abstract_expression::ExprRef;
use crate::execution::plans::plan_node::{PlanKind, PlanRef};
use crate::storage::table::tuple::{Tuple, TupleMeta};
use crate::types::value::Value;

pub struct UpdateExecutor<'e> {
    plan: PlanRef,
    table_info: &'e TableInfo<'e>,
    indexes: Vec<Arc<IndexInfo<'e>>>,
    target_expressions: Vec<ExprRef>,
    child: ExecutorBox<'e>,
    done: bool,
    /// The transaction the statement runs in, with its manager.
    txn: Option<(Arc<Transaction>, &'e TransactionManager)>,
}

impl<'e> UpdateExecutor<'e> {
    pub fn new(ctx: &'e ExecutorContext<'e>, plan: PlanRef, child: ExecutorBox<'e>) -> Result<UpdateExecutor<'e>> {
        let PlanKind::Update { table_oid, target_expressions } = &plan.kind else { unreachable!("an UpdateExecutor needs an Update plan") };
        let table_info = ctx.catalog.table_info(*table_oid).ok_or_else(|| Exception::new(ExceptionType::Execution, "the table of an update does not exist"))?;
        let indexes = ctx.catalog.get_table_indexes(&table_info.name);
        let target_expressions = target_expressions.clone();
        let txn = ctx.txn().cloned().zip(ctx.txn_mgr());
        Ok(UpdateExecutor { plan, table_info, indexes, target_expressions, child, done: false, txn })
    }

    /// The new tuple for `old`: every target expression evaluated on the old tuple, laid out with the table's schema.
    fn make_new_tuple(&self, old: &Tuple) -> Result<Tuple> {
        todo!("3e-03: each target expression evaluated on the old tuple, laid out with the table's schema")
    }

    /// Does any change give a tuple another primary key? Then the statement is a delete and an insert, not an in-place update.
    fn changes_primary_key(&self, changes: &[(Rid, Tuple)]) -> Result<bool> {
        let Some(pk) = self.indexes.iter().find(|i| i.is_primary_key) else { return Ok(false) };
        let attrs = pk.index.metadata().get_key_attrs();
        for (rid, new) in changes {
            let old = self.table_info.table.get_tuple(*rid)?.1;
            let (a, b) = (old.key_from_tuple(&self.table_info.schema, &pk.key_schema, attrs), new.key_from_tuple(&self.table_info.schema, &pk.key_schema, attrs));
            if a.data() != b.data() {
                return Ok(true);
            }
        }
        Ok(false)
    }

    /// An update that changes a primary key: first delete every old tuple, then insert every new one, so that `SET k = k + 1` can reuse
    /// the tombstones it has just made.
    fn update_by_delete_and_insert(&self, txn: &Arc<Transaction>, txn_mgr: &TransactionManager, changes: &[(Rid, Tuple)]) -> Result<()> {
        todo!("4b-06: modify_tuple(.., None) for every rid first; then insert_mvcc for every new tuple")
    }
}

impl Executor for UpdateExecutor<'_> {
    fn init(&mut self) -> Result<()> {
        todo!("3e-03: forget that the count was produced and initialise the child")
    }

    fn next(&mut self, tuple_batch: &mut Vec<Tuple>, rid_batch: &mut Vec<Rid>, _batch_size: usize) -> Result<bool> {
        tuple_batch.clear();
        rid_batch.clear();
        // 4b-03: when self.txn is Some: collect (rid, make_new_tuple(old)) for every child tuple first; if changes_primary_key(..)? call update_by_delete_and_insert, else modify_tuple(.., Some(new)) for each; answer with one tuple holding the count
        todo!("3e-03: once: every tuple of the child is replaced by its new version (the old marked deleted, the new stored, the indexes following); answers with how many, then false")
    }

    fn output_schema(&self) -> &Schema {
        &self.plan.output_schema
    }
}
