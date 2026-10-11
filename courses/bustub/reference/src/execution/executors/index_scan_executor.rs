//! Reads a table through one of its indexes.
//!
//! Without `pred_keys` it reads every row **in key order**; with `pred_keys` it looks each key up. Rows that are deleted, or that fail the
//! plan's `filter_predicate`, are skipped.

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
    /// The rids to visit, in the order to visit them; `cursor` is how many were visited.
    rids: Vec<Rid>,
    cursor: usize,
    /// The transaction to read as, with its manager.
    txn: Option<(Arc<Transaction>, &'e TransactionManager)>,
}

impl<'e> IndexScanExecutor<'e> {
    pub fn new(ctx: &'e ExecutorContext<'e>, plan: PlanRef) -> Result<IndexScanExecutor<'e>> {
        let PlanKind::IndexScan { table_oid, index_oid, filter_predicate, pred_keys } = &plan.kind else { unreachable!("an IndexScanExecutor needs an IndexScan plan") };
        let table_info = ctx.catalog.table_info(*table_oid).ok_or_else(|| Exception::new(ExceptionType::Execution, "the table of an index scan does not exist"))?;
        let index_info = ctx.catalog.get_index_by_oid(*index_oid).ok_or_else(|| Exception::new(ExceptionType::Execution, "the index of an index scan does not exist"))?;
        let (filter_predicate, pred_keys) = (filter_predicate.clone(), pred_keys.clone());
        Ok(IndexScanExecutor { plan, table_info, index_info, filter_predicate, pred_keys, rids: vec![], cursor: 0, txn: ctx.txn().cloned().zip(ctx.txn_mgr()) })
    }

    /// The rids this scan visits: with `pred_keys`, the rid under each key (in the order of the keys); without, every rid of the index in
    /// key order.
    fn collect_rids(&self) -> Result<Vec<Rid>> {
        // @begin 3e-04
        if self.pred_keys.is_empty() {
            return Ok(self.index_info.index.scan_all());
        }
        let empty = Tuple::empty();
        let no_schema = Schema::new(vec![]);
        let mut rids = vec![];
        for key_expr in &self.pred_keys {
            let value = key_expr.evaluate(&empty, &no_schema)?;
            let key = Tuple::new(&[value], &self.index_info.key_schema);
            for rid in self.index_info.index.scan_key(&key) {
                // `v1 = 4 or v1 = 4` is one row, not two: a rid is visited once, at its first key
                if !rids.contains(&rid) {
                    rids.push(rid);
                }
            }
        }
        Ok(rids)
        //~ Ok(self.index_info.index.scan_all()) // 3e-04: with pred_keys: evaluate each key (a constant), make a one-column key tuple with the index's key schema and look it up (Index::scan_key); a rid found under two keys is visited once
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
        self.rids = self.collect_rids()?;
        self.cursor = 0;
        Ok(())
        //~ todo!("3e-04: remember the rids to visit (collect_rids) and start at the first")
        // @end
    }

    fn next(&mut self, tuple_batch: &mut Vec<Tuple>, rid_batch: &mut Vec<Rid>, batch_size: usize) -> Result<bool> {
        tuple_batch.clear();
        rid_batch.clear();
        // @begin 4b-06
        if let Some((txn, txn_mgr)) = &self.txn {
            while tuple_batch.len() < batch_size && self.cursor < self.rids.len() {
                let rid = self.rids[self.cursor];
                self.cursor += 1;
                // the index leads to a rid; the version this transaction may see there (maybe none) is read as the sequential scan does
                let Some(tuple) = read_visible_version(txn, txn_mgr, self.table_info, rid)? else { continue };
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
        //~ // 4b-06: when self.txn is Some: for each rid use read_visible_version (given) instead of the heap's tuple, skip None, apply the filter as below
        // @end
        // @begin 3e-04
        while tuple_batch.len() < batch_size && self.cursor < self.rids.len() {
            let rid = self.rids[self.cursor];
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
        //~ todo!("3e-04: fill the batch from the rids: fetch each tuple from the heap (get_tuple), skip deleted ones and those for which the filter predicate (if any) is not TRUE; true if the batch is not empty")
        // @end
    }

    fn output_schema(&self) -> &Schema {
        &self.plan.output_schema
    }
}
