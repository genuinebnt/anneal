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
        // @begin 3e-03
        for index in &self.indexes {
            let key = tuple.key_from_tuple(&self.table_info.schema, &index.key_schema, index.index.metadata().get_key_attrs());
            index.index.delete_entry(&key);
        }
        //~ // 3e-03: for every index, delete the entry for the key of the tuple (Tuple::key_from_tuple, as in the insert executor)
        // @end
    }
}

impl Executor for DeleteExecutor<'_> {
    fn init(&mut self) -> Result<()> {
        // @begin 3e-03
        self.done = false;
        self.child.init()
        //~ todo!("3e-03: forget that the count was produced and initialise the child")
        // @end
    }

    fn next(&mut self, tuple_batch: &mut Vec<Tuple>, rid_batch: &mut Vec<Rid>, _batch_size: usize) -> Result<bool> {
        tuple_batch.clear();
        rid_batch.clear();
        // @begin 4b-02
        if let Some((txn, txn_mgr)) = &self.txn {
            if self.done {
                return Ok(false);
            }
            let (mut child_tuples, mut child_rids) = (vec![], vec![]);
            let mut count = 0;
            while self.child.next(&mut child_tuples, &mut child_rids, BUSTUB_BATCH_SIZE)? {
                for rid in &child_rids {
                    // the index entries stay: a deleted tuple is a tombstone, and its key will find it again
                    modify_tuple(txn, txn_mgr, self.table_info, *rid, None)?;
                    count += 1;
                }
            }
            self.done = true;
            tuple_batch.push(Tuple::new(&[Value::integer(count)], &self.plan.output_schema));
            rid_batch.push(Rid::default());
            return Ok(true);
        }
        //~ // 4b-02: when self.txn is Some: once, for every rid of the child call modify_tuple(.., None) (index entries stay), count, and answer with one tuple holding the count
        // @end
        // @begin 3e-03
        if self.done {
            return Ok(false);
        }
        let (mut child_tuples, mut child_rids) = (vec![], vec![]);
        let mut count = 0;
        while self.child.next(&mut child_tuples, &mut child_rids, BUSTUB_BATCH_SIZE)? {
            for (tuple, rid) in child_tuples.iter().zip(&child_rids) {
                self.table_info.table.update_tuple_meta(&TupleMeta { ts: 0, is_deleted: true }, *rid)?;
                self.delete_from_indexes(tuple);
                count += 1;
            }
        }
        self.done = true;
        tuple_batch.push(Tuple::new(&[Value::integer(count)], &self.plan.output_schema));
        rid_batch.push(Rid::default());
        Ok(true)
        //~ todo!("3e-03: once: for every tuple (and rid) of the child mark the heap tuple deleted (update_tuple_meta with is_deleted: true), remove its index entries (delete_from_indexes) and count it; answer with one tuple holding the count; every later call returns false")
        // @end
    }

    fn output_schema(&self) -> &Schema {
        &self.plan.output_schema
    }
}
