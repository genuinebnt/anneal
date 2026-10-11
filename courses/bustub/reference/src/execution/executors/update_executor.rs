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
        // @begin 3e-03
        let mut values = Vec::with_capacity(self.target_expressions.len());
        for expr in &self.target_expressions {
            values.push(expr.evaluate(old, self.child.output_schema())?);
        }
        Ok(Tuple::new(&values, &self.table_info.schema))
        //~ todo!("3e-03: evaluate each target expression on the old tuple (with the child's output schema) and build a tuple of those values with the table's schema")
        // @end
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
        // @begin 4b-06
        for (rid, _) in changes {
            modify_tuple(txn, txn_mgr, self.table_info, *rid, None)?;
        }
        for (_, new) in changes {
            insert_mvcc(txn, txn_mgr, self.table_info, &self.indexes, new)?;
        }
        Ok(())
        //~ todo!("4b-06: modify_tuple(.., None) for every rid first; then insert_mvcc for every new tuple")
        // @end
    }
}

impl Executor for UpdateExecutor<'_> {
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
        // @begin 4b-03
        if let Some((txn, txn_mgr)) = &self.txn {
            if self.done {
                return Ok(false);
            }
            // read everything first: the changes must not be seen by the scan that finds them
            let (mut child_tuples, mut child_rids) = (vec![], vec![]);
            let mut changes: Vec<(Rid, Tuple)> = vec![];
            while self.child.next(&mut child_tuples, &mut child_rids, BUSTUB_BATCH_SIZE)? {
                for (old, rid) in child_tuples.iter().zip(&child_rids) {
                    changes.push((*rid, self.make_new_tuple(old)?));
                }
            }
            if self.changes_primary_key(&changes)? {
                self.update_by_delete_and_insert(txn, txn_mgr, &changes)?;
            } else {
                for (rid, new) in &changes {
                    modify_tuple(txn, txn_mgr, self.table_info, *rid, Some(new))?;
                }
            }
            self.done = true;
            tuple_batch.push(Tuple::new(&[Value::integer(changes.len() as i32)], &self.plan.output_schema));
            rid_batch.push(Rid::default());
            return Ok(true);
        }
        //~ // 4b-03: when self.txn is Some: collect (rid, make_new_tuple(old)) for every child tuple first; if changes_primary_key(..)? call update_by_delete_and_insert, else modify_tuple(.., Some(new)) for each; answer with one tuple holding the count
        // @end
        // @begin 3e-03
        if self.done {
            return Ok(false);
        }
        let (mut child_tuples, mut child_rids) = (vec![], vec![]);
        let mut count = 0;
        while self.child.next(&mut child_tuples, &mut child_rids, BUSTUB_BATCH_SIZE)? {
            for (old, old_rid) in child_tuples.iter().zip(&child_rids) {
                let new = self.make_new_tuple(old)?;
                // delete the old tuple and its index entries ...
                self.table_info.table.update_tuple_meta(&TupleMeta { ts: 0, is_deleted: true }, *old_rid)?;
                for index in &self.indexes {
                    let key = old.key_from_tuple(&self.table_info.schema, &index.key_schema, index.index.metadata().get_key_attrs());
                    index.index.delete_entry(&key);
                }
                // ... and insert the new one
                let new_rid = self.table_info.table.insert_tuple(&TupleMeta { ts: 0, is_deleted: false }, &new)?;
                for index in &self.indexes {
                    let key = new.key_from_tuple(&self.table_info.schema, &index.key_schema, index.index.metadata().get_key_attrs());
                    index.index.insert_entry(&key, new_rid);
                }
                count += 1;
            }
        }
        self.done = true;
        tuple_batch.push(Tuple::new(&[Value::integer(count)], &self.plan.output_schema));
        rid_batch.push(Rid::default());
        Ok(true)
        //~ todo!("3e-03: once: for every tuple of the child build the new tuple (make_new_tuple), mark the old heap tuple deleted and delete its index entries, insert the new tuple into the heap and its index entries under the new rid, count; answer with one tuple holding the count; later calls return false")
        // @end
    }

    fn output_schema(&self) -> &Schema {
        &self.plan.output_schema
    }
}
