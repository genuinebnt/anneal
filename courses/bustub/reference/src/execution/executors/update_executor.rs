//! Replaces every tuple its child produces by a new one made of the plan's `target_expressions`, evaluated on the old tuple, and produces one row:
//! how many it updated.
//!
//! An update is a **delete followed by an insert**: the old tuple is marked deleted and the new one is stored under a new rid (it may have another
//! size), and the table's indexes follow.
//!
//! An index that is not a primary key takes any key. A primary key is unique and has no NULL, judged on the rows as they are once the
//! whole statement has run: `SET k = k + 1` over the keys 1, 2, 3 succeeds. A statement that would leave two live rows with one primary
//! key, or a primary key with a NULL, fails and changes nothing.

use std::collections::HashSet;
use std::sync::Arc;

use super::abstract_executor::{Executor, ExecutorBox, BUSTUB_BATCH_SIZE};
use crate::catalog::catalog::{IndexInfo, TableInfo};
use crate::catalog::schema::Schema;
use crate::common::exception::{Exception, ExceptionType, Result};
use crate::common::rid::Rid;
use crate::concurrency::transaction::Transaction;
use crate::concurrency::transaction_manager::TransactionManager;
use crate::execution::execution_common::{insert_index_entries, insert_mvcc, modify_tuple};
use crate::execution::executor_context::ExecutorContext;
use crate::execution::expressions::abstract_expression::ExprRef;
use crate::execution::plans::plan_node::{PlanKind, PlanRef};
use crate::storage::index::index::unique_violation;
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
        let table_info = ctx
            .catalog
            .table_info(*table_oid)
            .ok_or_else(|| Exception::new(ExceptionType::Execution, "the table of an update does not exist"))?;
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
        //~ todo!("3e-03: each target expression evaluated on the old tuple, laid out with the table's schema")
        // @end
    }

    /// Fails with a unique violation if, after replacing each old tuple at `changes[i].0` by `changes[i].1`, two live rows would have the
    /// same primary key: two new tuples with one key, or a new tuple with the key of a row the statement does not change. Fails with a
    /// not-null violation if a new primary key has a NULL ([`IndexInfo::check_primary_key`]). Without a primary key every change passes.
    fn check_constraints(&self, changes: &[(Rid, Tuple)]) -> Result<()> {
        // @begin 3e-03
        let Some(pk) = self.indexes.iter().find(|i| i.is_primary_key) else { return Ok(()) };
        let changed: HashSet<Rid> = changes.iter().map(|(rid, _)| *rid).collect();
        let mut new_keys = HashSet::new();
        for (_, new) in changes {
            let key = new.key_from_tuple(&self.table_info.schema, &pk.key_schema, pk.index.metadata().get_key_attrs());
            pk.check_primary_key(&key)?;
            // the old keys of the changed rows are about to go: only an unchanged row's entry is a conflict
            let held_by_unchanged_row = pk.index.scan_key(&key).iter().any(|rid| !changed.contains(rid));
            if held_by_unchanged_row || !new_keys.insert(key.data().to_vec()) {
                return Err(unique_violation(&pk.name));
            }
        }
        Ok(())
        //~ todo!("3e-03: the not-null error of IndexInfo::check_primary_key for a NULL primary key; Err(unique_violation(index name)) if the new tuples would give two live rows one primary key; Ok otherwise, and always without a primary key")
        // @end
    }

    /// Does any change give a tuple another primary key? Then the statement is a delete and an insert, not an in-place update.
    fn changes_primary_key(&self, changes: &[(Rid, Tuple)]) -> Result<bool> {
        let Some(pk) = self.indexes.iter().find(|i| i.is_primary_key) else { return Ok(false) };
        let attrs = pk.index.metadata().get_key_attrs();
        for (rid, new) in changes {
            let old = self.table_info.table.get_tuple(*rid)?.1;
            let (a, b) = (
                old.key_from_tuple(&self.table_info.schema, &pk.key_schema, attrs),
                new.key_from_tuple(&self.table_info.schema, &pk.key_schema, attrs),
            );
            if a.data() != b.data() {
                return Ok(true);
            }
        }
        Ok(false)
    }

    /// An update that changes a primary key: first delete every old tuple, then insert every new one, so that `SET k = k + 1` can reuse
    /// the tombstones it has just made. A new key with a NULL fails with a not-null violation before anything is written.
    fn update_by_delete_and_insert(&self, txn: &Arc<Transaction>, txn_mgr: &TransactionManager, changes: &[(Rid, Tuple)]) -> Result<()> {
        // @begin 4b-06
        if let Some(pk) = self.indexes.iter().find(|i| i.is_primary_key) {
            for (_, new) in changes {
                pk.check_primary_key(&new.key_from_tuple(&self.table_info.schema, &pk.key_schema, pk.index.metadata().get_key_attrs()))?;
            }
        }
        for (rid, _) in changes {
            modify_tuple(txn, txn_mgr, self.table_info, *rid, None)?;
        }
        for (_, new) in changes {
            insert_mvcc(txn, txn_mgr, self.table_info, &self.indexes, new)?;
        }
        Ok(())
        //~ todo!("4b-06: a NULL new primary key fails (IndexInfo::check_primary_key) before anything is written; otherwise every old tuple is deleted (modify_tuple) before any new one is inserted (insert_mvcc)")
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
                    // a changed indexed column gets its new key's entry; the old one stays for older readers
                    insert_index_entries(self.table_info, &self.indexes, new, *rid);
                }
            }
            self.done = true;
            tuple_batch.push(Tuple::new(&[Value::integer(changes.len() as i32)], &self.plan.output_schema));
            rid_batch.push(Rid::default());
            return Ok(true);
        }
        //~ // 4b-03: when self.txn is Some: collect (rid, make_new_tuple(old)) for every child tuple first; if changes_primary_key(..)? call update_by_delete_and_insert, else modify_tuple(.., Some(new)) and insert_index_entries(.., new, rid) for each; answer with one tuple holding the count
        // @end
        // @begin 3e-03
        if self.done {
            return Ok(false);
        }
        // read everything first: the uniqueness of the new primary keys is judged on the whole statement
        let (mut child_tuples, mut child_rids) = (vec![], vec![]);
        let mut changes: Vec<(Rid, Tuple)> = vec![];
        let mut old_tuples: Vec<Tuple> = vec![];
        while self.child.next(&mut child_tuples, &mut child_rids, BUSTUB_BATCH_SIZE)? {
            for (old, old_rid) in child_tuples.iter().zip(&child_rids) {
                changes.push((*old_rid, self.make_new_tuple(old)?));
                old_tuples.push(old.clone());
            }
        }
        self.check_constraints(&changes)?;
        // every old tuple and its index entries go before any new key arrives, so that keys can move between rows
        for ((old_rid, _), old) in changes.iter().zip(&old_tuples) {
            self.table_info.table.update_tuple_meta(&TupleMeta { ts: 0, is_deleted: true }, *old_rid)?;
            for index in &self.indexes {
                let key = old.key_from_tuple(&self.table_info.schema, &index.key_schema, index.index.metadata().get_key_attrs());
                index.index.delete_entry(&key, *old_rid);
            }
        }
        for (_, new) in &changes {
            let new_rid = self.table_info.table.insert_tuple(&TupleMeta { ts: 0, is_deleted: false }, new)?;
            for index in &self.indexes {
                let key = new.key_from_tuple(&self.table_info.schema, &index.key_schema, index.index.metadata().get_key_attrs());
                index.index.insert_entry(&key, new_rid);
            }
        }
        self.done = true;
        tuple_batch.push(Tuple::new(&[Value::integer(changes.len() as i32)], &self.plan.output_schema));
        rid_batch.push(Rid::default());
        Ok(true)
        //~ todo!("3e-03: once: replaces every tuple of the child by its new version (the old marked deleted, the new stored, every index following), or, if the new primary keys would be repeated or NULL (check_constraints), fails and changes nothing; answers with how many, then false")
        // @end
    }

    fn output_schema(&self) -> &Schema {
        &self.plan.output_schema
    }
}
