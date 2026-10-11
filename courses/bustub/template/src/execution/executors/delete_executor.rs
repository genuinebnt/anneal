//! Deletes every tuple its child produces and produces one row: how many it deleted.
//!
//! Deleting marks the tuple deleted in the heap and removes its index entries.

use std::sync::Arc;

use super::abstract_executor::{Executor, ExecutorBox, BUSTUB_BATCH_SIZE};
use crate::catalog::catalog::{IndexInfo, TableInfo};
use crate::catalog::schema::Schema;
use crate::common::exception::{Exception, ExceptionType, Result};
use crate::common::rid::Rid;
use crate::concurrency::transaction::Transaction;
use crate::concurrency::transaction_manager::TransactionManager;
use crate::execution::execution_common::modify_tuple;
use crate::execution::executor_context::ExecutorContext;
use crate::execution::plans::plan_node::{PlanKind, PlanRef};
use crate::storage::table::tuple::{Tuple, TupleMeta};
use crate::types::value::Value;

pub struct DeleteExecutor<'e> {
    plan: PlanRef,
    table_info: &'e TableInfo<'e>,
    indexes: Vec<Arc<IndexInfo<'e>>>,
    child: ExecutorBox<'e>,
    done: bool,
    /// The transaction the statement runs in, with its manager.
    txn: Option<(Arc<Transaction>, &'e TransactionManager)>,
}

impl<'e> DeleteExecutor<'e> {
    pub fn new(ctx: &'e ExecutorContext<'e>, plan: PlanRef, child: ExecutorBox<'e>) -> Result<DeleteExecutor<'e>> {
        let PlanKind::Delete { table_oid } = &plan.kind else { unreachable!("a DeleteExecutor needs a Delete plan") };
        let table_info = ctx.catalog.table_info(*table_oid).ok_or_else(|| Exception::new(ExceptionType::Execution, "the table of a delete does not exist"))?;
        let indexes = ctx.catalog.get_table_indexes(&table_info.name);
        let txn = ctx.txn().cloned().zip(ctx.txn_mgr());
        Ok(DeleteExecutor { plan, table_info, indexes, child, done: false, txn })
    }

    /// Removes the entries of `tuple` from every index of the table.
    fn delete_from_indexes(&self, tuple: &Tuple) {
        // 3e-03: for every index, delete the entry for the key of the tuple (Tuple::key_from_tuple, as in the insert executor)
    }
}

impl Executor for DeleteExecutor<'_> {
    fn init(&mut self) -> Result<()> {
        todo!("3e-03: forget that the count was produced and initialise the child")
    }

    fn next(&mut self, tuple_batch: &mut Vec<Tuple>, rid_batch: &mut Vec<Rid>, _batch_size: usize) -> Result<bool> {
        tuple_batch.clear();
        rid_batch.clear();
        // 4b-02: when self.txn is Some: once, for every rid of the child call modify_tuple(.., None) (index entries stay), count, and answer with one tuple holding the count
        todo!("3e-03: marks every tuple of the child deleted and removes its index entries; answers once with how many, then false")
    }

    fn output_schema(&self) -> &Schema {
        &self.plan.output_schema
    }
}
