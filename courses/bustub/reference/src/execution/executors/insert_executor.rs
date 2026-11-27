//! Inserts every tuple of its child into a table and its indexes, then produces one row: how many it inserted.
//!
//! Every index of the table is unique over non-NULL keys, and a primary key has no NULL: a tuple whose key an index already holds, or
//! whose primary key has a NULL, is not inserted, and the statement fails at that tuple. The tuples before it stay inserted.

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
use crate::storage::index::index::unique_violation;
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

    /// Fails with a not-null violation if the primary key of `tuple` has a NULL ([`IndexInfo::check_primary_key`]), and with a
    /// unique violation if an index of the table already holds the key of `tuple`. Any other key with a NULL passes: NULL equals nothing.
    fn check_unique(&self, tuple: &Tuple) -> Result<()> {
        // @begin 3e-02
        for index in &self.indexes {
            let key = tuple.key_from_tuple(&self.table_info.schema, &index.key_schema, index.index.metadata().get_key_attrs());
            index.check_primary_key(&key)?;
            if !index.index.scan_key(&key).is_empty() {
                return Err(unique_violation(&index.name));
            }
        }
        Ok(())
        //~ todo!("3e-02: the not-null error of IndexInfo::check_primary_key for a NULL primary key; Err(unique_violation(index name)) if any index of the table already holds the tuple's key; Ok otherwise")
        // @end
    }

    /// Adds the tuple stored at `rid` to every index of the table whose key it has without a NULL.
    fn insert_into_indexes(&self, tuple: &Tuple, rid: Rid) {
        // @begin 3e-02
        for index in &self.indexes {
            let key = tuple.key_from_tuple(&self.table_info.schema, &index.key_schema, index.index.metadata().get_key_attrs());
            index.index.insert_entry(&key, rid);
        }
        //~ // 3e-02: in every index, the tuple's key (Tuple::key_from_tuple) maps to rid
        // @end
    }
}

impl Executor for InsertExecutor<'_> {
    fn init(&mut self) -> Result<()> {
        // @begin 3e-02
        self.done = false;
        self.child.init()
        //~ todo!("3e-02: forget that the count was produced and initialise the child")
        // @end
    }

    fn next(&mut self, tuple_batch: &mut Vec<Tuple>, rid_batch: &mut Vec<Rid>, _batch_size: usize) -> Result<bool> {
        tuple_batch.clear();
        rid_batch.clear();
        // @begin 4b-01
        if let Some((txn, txn_mgr)) = &self.txn {
            if self.done {
                return Ok(false);
            }
            let (mut child_tuples, mut child_rids) = (vec![], vec![]);
            let mut count = 0;
            while self.child.next(&mut child_tuples, &mut child_rids, BUSTUB_BATCH_SIZE)? {
                for tuple in &child_tuples {
                    insert_mvcc(txn, txn_mgr, self.table_info, &self.indexes, tuple)?;
                    count += 1;
                }
            }
            self.done = true;
            tuple_batch.push(Tuple::new(&[Value::integer(count)], &self.plan.output_schema));
            rid_batch.push(Rid::default());
            return Ok(true);
        }
        //~ // 4b-01: when self.txn is Some: as below, but insert each tuple with insert_mvcc (in execution_common.rs) instead of the heap and the indexes directly
        // @end
        // @begin 3e-02
        if self.done {
            return Ok(false);
        }
        let (mut child_tuples, mut child_rids) = (vec![], vec![]);
        let mut count = 0;
        while self.child.next(&mut child_tuples, &mut child_rids, BUSTUB_BATCH_SIZE)? {
            for tuple in &child_tuples {
                self.check_unique(tuple)?;
                let rid = self.table_info.table.insert_tuple(&TupleMeta { ts: 0, is_deleted: false }, tuple)?;
                self.insert_into_indexes(tuple, rid);
                count += 1;
            }
        }
        self.done = true;
        tuple_batch.push(Tuple::new(&[Value::integer(count)], &self.plan.output_schema));
        rid_batch.push(Rid::default());
        Ok(true)
        //~ todo!("3e-02: once: inserts every tuple of the child into the table and its indexes, failing at the first tuple whose key an index already holds (that tuple is in neither); answers with how many, then false")
        // @end
    }

    fn output_schema(&self) -> &Schema {
        &self.plan.output_schema
    }
}
