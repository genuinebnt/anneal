//! Inserts every tuple of its child into a table and its indexes, then produces one row: how many it inserted.

use std::sync::Arc;

use super::abstract_executor::{Executor, ExecutorBox, BUSTUB_BATCH_SIZE};
use crate::catalog::catalog::{IndexInfo, TableInfo};
use crate::catalog::schema::Schema;
use crate::common::exception::{Exception, ExceptionType, Result};
use crate::common::rid::Rid;
use crate::concurrency::transaction::Transaction;
use crate::concurrency::transaction_manager::TransactionManager;
use crate::execution::execution_common::insert_mvcc;
use crate::execution::executor_context::ExecutorContext;
use crate::execution::plans::plan_node::{PlanKind, PlanRef};
use crate::storage::table::tuple::{Tuple, TupleMeta};
use crate::types::value::Value;

pub struct InsertExecutor<'e> {
    plan: PlanRef,
    table_info: &'e TableInfo<'e>,
    indexes: Vec<Arc<IndexInfo<'e>>>,
    child: ExecutorBox<'e>,
    /// The count has been produced.
    done: bool,
    /// The transaction the statement runs in, with its manager.
    txn: Option<(Arc<Transaction>, &'e TransactionManager)>,
}

impl<'e> InsertExecutor<'e> {
    pub fn new(ctx: &'e ExecutorContext<'e>, plan: PlanRef, child: ExecutorBox<'e>) -> Result<InsertExecutor<'e>> {
        let PlanKind::Insert { table_oid } = &plan.kind else { unreachable!("an InsertExecutor needs an Insert plan") };
        let table_info = ctx.catalog.table_info(*table_oid).ok_or_else(|| Exception::new(ExceptionType::Execution, "the table of an insert does not exist"))?;
        let indexes = ctx.catalog.get_table_indexes(&table_info.name);
        let txn = ctx.txn().cloned().zip(ctx.txn_mgr());
        Ok(InsertExecutor { plan, table_info, indexes, child, done: false, txn })
    }

    /// Adds the tuple stored at `rid` to every index of the table. An index that already holds the key keeps its entry.
    fn insert_into_indexes(&self, tuple: &Tuple, rid: Rid) {
        // 3e-02: for every index, the key of the tuple (Tuple::key_from_tuple with the table's schema, the index's key schema and its key attributes) maps to rid
    }
}

impl Executor for InsertExecutor<'_> {
    fn init(&mut self) -> Result<()> {
        todo!("3e-02: forget that the count was produced and initialise the child")
    }

    fn next(&mut self, tuple_batch: &mut Vec<Tuple>, rid_batch: &mut Vec<Rid>, _batch_size: usize) -> Result<bool> {
        tuple_batch.clear();
        rid_batch.clear();
        // 4b-01: when self.txn is Some: as below, but insert each tuple with insert_mvcc (in execution_common.rs) instead of the heap and the indexes directly
        todo!("3e-02: once: pull every batch from the child, insert each tuple into the heap (a TupleMeta with ts 0 and is_deleted false), call insert_into_indexes with the new rid, count them; answer with one tuple holding the count; every later call returns false")
    }

    fn output_schema(&self) -> &Schema {
        &self.plan.output_schema
    }
}
