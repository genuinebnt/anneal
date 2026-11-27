//! Reads a table through one of its indexes.
//!
//! Without `pred_keys` it reads every row **in key order** (rows that share a key in rid order); with `pred_keys` it looks each key up
//! and reads every row of that key. Rows that are deleted, or that fail the plan's `filter_predicate`, are skipped.
//!
//! Inside a transaction each row is the version the transaction may see. An index keeps the entries of a row's older versions, so an
//! entry counts only if that version has the entry's key: each row comes back once, under the key the transaction sees.

use std::sync::Arc;

use super::abstract_executor::Executor;
use crate::catalog::catalog::{IndexInfo, TableInfo};
use crate::catalog::schema::Schema;
use crate::common::exception::{Exception, ExceptionType, Result};
use crate::common::rid::Rid;
use crate::concurrency::transaction::{IsolationLevel, Transaction};
use crate::concurrency::transaction_manager::TransactionManager;
use crate::execution::execution_common::{read_visible_version, true_predicate};
use crate::execution::executor_context::ExecutorContext;
use crate::execution::expressions::abstract_expression::ExprRef;
use crate::execution::plans::plan_node::{PlanKind, PlanRef};
use crate::storage::table::tuple::Tuple;

pub struct IndexScanExecutor<'e> {
    plan: PlanRef,
    table_info: &'e TableInfo<'e>,
    index_info: Arc<IndexInfo<'e>>,
    filter_predicate: Option<ExprRef>,
    pred_keys: Vec<ExprRef>,
    /// The index entries `(key, rid)` to visit, in the order to visit them; `cursor` is how many were visited.
    entries: Vec<(Tuple, Rid)>,
    cursor: usize,
    /// The transaction to read as, with its manager.
    txn: Option<(Arc<Transaction>, &'e TransactionManager)>,
}

impl<'e> IndexScanExecutor<'e> {
    pub fn new(ctx: &'e ExecutorContext<'e>, plan: PlanRef) -> Result<IndexScanExecutor<'e>> {
        let PlanKind::IndexScan { table_oid, index_oid, filter_predicate, pred_keys } = &plan.kind else {
            unreachable!("an IndexScanExecutor needs an IndexScan plan")
        };
        let table_info = ctx
            .catalog
            .table_info(*table_oid)
            .ok_or_else(|| Exception::new(ExceptionType::Execution, "the table of an index scan does not exist"))?;
        let index_info = ctx
            .catalog
            .get_index_by_oid(*index_oid)
            .ok_or_else(|| Exception::new(ExceptionType::Execution, "the index of an index scan does not exist"))?;
        let (filter_predicate, pred_keys) = (filter_predicate.clone(), pred_keys.clone());
        Ok(IndexScanExecutor {
            plan,
            table_info,
            index_info,
            filter_predicate,
            pred_keys,
            entries: vec![],
            cursor: 0,
            txn: ctx.txn().cloned().zip(ctx.txn_mgr()),
        })
    }

    /// The entries this scan visits: with `pred_keys`, the entries of each key, a key given twice looked up once (in the order of the
    /// keys); without, every entry of the index, in order.
    fn collect_entries(&self) -> Result<Vec<(Tuple, Rid)>> {
        // @begin 3e-04
        if self.pred_keys.is_empty() {
            return Ok(self.index_info.index.scan_all());
        }
        let empty = Tuple::empty();
        let no_schema = Schema::new(vec![]);
        let mut entries: Vec<(Tuple, Rid)> = vec![];
        let mut looked_up: Vec<Tuple> = vec![];
        for key_expr in &self.pred_keys {
            let value = key_expr.evaluate(&empty, &no_schema)?;
            let key = Tuple::new(&[value], &self.index_info.key_schema);
            // `v1 = 4 or v1 = 4` is one lookup, not two: each row once
            if looked_up.iter().any(|k| k.data() == key.data()) {
                continue;
            }
            for rid in self.index_info.index.scan_key(&key) {
                entries.push((key.clone(), rid));
            }
            looked_up.push(key);
        }
        Ok(entries)
        //~ Ok(self.index_info.index.scan_all()) // 3e-04: with pred_keys: evaluate each key (a constant), make a one-column key tuple with the index's key schema and take the entries of its rids (Index::scan_key); a key given twice is looked up once
        // @end
    }
}

impl Executor for IndexScanExecutor<'_> {
    fn init(&mut self) -> Result<()> {
        // @begin 4b-07
        if let Some((txn, _)) = &self.txn {
            if txn.isolation_level() == IsolationLevel::Serializable {
                txn.append_scan_predicate(self.table_info.oid, self.filter_predicate.clone().unwrap_or_else(true_predicate));
            }
        }
        //~ // 4b-07: a serializable transaction remembers what it scans with: append_scan_predicate(table oid, the filter predicate, or true_predicate())
        // @end
        // @begin 3e-04
        self.entries = self.collect_entries()?;
        self.cursor = 0;
        Ok(())
        //~ todo!("3e-04: remember the entries to visit (collect_entries) and start at the first")
        // @end
    }

    fn next(&mut self, tuple_batch: &mut Vec<Tuple>, rid_batch: &mut Vec<Rid>, batch_size: usize) -> Result<bool> {
        tuple_batch.clear();
        rid_batch.clear();
        // @begin 4b-06
        if let Some((txn, txn_mgr)) = &self.txn {
            while tuple_batch.len() < batch_size && self.cursor < self.entries.len() {
                let (key, rid) = &self.entries[self.cursor];
                let rid = *rid;
                self.cursor += 1;
                // the index leads to a rid; the version this transaction may see there (maybe none) is read as the sequential scan does
                let Some(tuple) = read_visible_version(txn, txn_mgr, self.table_info, rid)? else { continue };
                // the entry of another version's key: the row comes back once, at the entry of the key this version has
                let attrs = self.index_info.index.metadata().get_key_attrs();
                if tuple.key_from_tuple(&self.table_info.schema, &self.index_info.key_schema, attrs).data() != key.data() {
                    continue;
                }
                if let Some(predicate) = &self.filter_predicate {
                    if predicate.evaluate(&tuple, &self.plan.output_schema)?.as_bool() != Some(true) {
                        continue;
                    }
                }
                tuple_batch.push(tuple);
                rid_batch.push(rid);
            }
            return Ok(!tuple_batch.is_empty());
        }
        //~ // 4b-06: when self.txn is Some: for each entry use read_visible_version (given) instead of the heap's tuple, skip None, skip a version whose key is not the entry's key, apply the filter as below
        // @end
        // @begin 3e-04
        while tuple_batch.len() < batch_size && self.cursor < self.entries.len() {
            let rid = self.entries[self.cursor].1;
            self.cursor += 1;
            let (meta, tuple) = self.table_info.table.get_tuple(rid)?;
            if meta.is_deleted {
                continue;
            }
            if let Some(predicate) = &self.filter_predicate {
                if predicate.evaluate(&tuple, &self.plan.output_schema)?.as_bool() != Some(true) {
                    continue;
                }
            }
            tuple_batch.push(tuple);
            rid_batch.push(rid);
        }
        Ok(!tuple_batch.is_empty())
        //~ todo!("3e-04: the batch holds the live tuples at the entries' rids that satisfy the filter; true if it is not empty")
        // @end
    }

    fn output_schema(&self) -> &Schema {
        &self.plan.output_schema
    }
}
