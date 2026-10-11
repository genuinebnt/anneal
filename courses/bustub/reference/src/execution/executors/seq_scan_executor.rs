//! Reads every row of a table in storage order, skipping deleted rows and, when a filter was merged into the scan, rows that do not satisfy the
//! plan's `filter_predicate`.

use std::sync::Arc;

use super::abstract_executor::Executor;
use crate::catalog::catalog::TableInfo;
use crate::catalog::schema::Schema;
use crate::common::exception::{Exception, ExceptionType, Result};
use crate::common::rid::Rid;
use crate::concurrency::transaction::{IsolationLevel, Transaction};
use crate::concurrency::transaction_manager::{get_tuple_and_undo_link, TransactionManager};
use crate::execution::execution_common::{collect_undo_logs, reconstruct_tuple, true_predicate};
use crate::execution::executor_context::ExecutorContext;
use crate::execution::expressions::abstract_expression::ExprRef;
use crate::execution::plans::plan_node::{PlanKind, PlanRef};
use crate::storage::table::table_iterator::TableIterator;
use crate::storage::table::tuple::Tuple;

pub struct SeqScanExecutor<'e> {
    plan: PlanRef,
    table_info: &'e TableInfo<'e>,
    filter_predicate: Option<ExprRef>,
    iter: Option<TableIterator<'e>>,
    /// The transaction to read as, with the manager that holds the version chains.
    txn: Option<(Arc<Transaction>, &'e TransactionManager)>,
}

/// Does `tuple` pass the scan's filter predicate? No predicate keeps everything; otherwise only the rows for which the predicate is
/// TRUE (FALSE and NULL are dropped). `schema` is the layout of the scan's tuples.
fn passes_filter(filter: &Option<ExprRef>, schema: &Schema, tuple: &Tuple) -> Result<bool> {
    // @begin 3e-01
    match filter {
        None => Ok(true),
        Some(predicate) => Ok(predicate.evaluate(tuple, schema)?.as_bool() == Some(true)),
    }
    //~ Ok(true) // 3e-01: evaluate the predicate on the tuple; keep the row only if the answer is TRUE
    // @end
}

impl<'e> SeqScanExecutor<'e> {
    pub fn new(ctx: &'e ExecutorContext<'e>, plan: PlanRef) -> Result<SeqScanExecutor<'e>> {
        let PlanKind::SeqScan { table_oid, filter_predicate, .. } = &plan.kind else { unreachable!("a SeqScanExecutor needs a SeqScan plan") };
        let table_info = ctx.catalog.table_info(*table_oid).ok_or_else(|| Exception::new(ExceptionType::Execution, "the table of a scan does not exist"))?;
        let filter_predicate = filter_predicate.clone();
        let txn = ctx.txn().cloned().zip(ctx.txn_mgr());
        Ok(SeqScanExecutor { plan, table_info, filter_predicate, iter: None, txn })
    }
}

impl Executor for SeqScanExecutor<'_> {
    fn init(&mut self) -> Result<()> {
        // @begin 4b-07
        if let Some((txn, _)) = &self.txn {
            if txn.isolation_level() == IsolationLevel::Serializable {
                txn.append_scan_predicate(self.table_info.oid, self.filter_predicate.clone().unwrap_or_else(true_predicate));
            }
        }
        //~ // 4b-07: a serializable transaction remembers what it scans with: txn.append_scan_predicate(table oid, the filter predicate, or true_predicate() if the scan has none)
        // @end
        // @begin 3e-01
        self.iter = Some(self.table_info.table.make_iterator());
        Ok(())
        //~ todo!("3e-01: start a table iterator (module 3c: the one that stops where the table ended when the scan began)")
        // @end
    }

    fn next(&mut self, tuple_batch: &mut Vec<Tuple>, rid_batch: &mut Vec<Rid>, batch_size: usize) -> Result<bool> {
        tuple_batch.clear();
        rid_batch.clear();
        // @begin 4a-05
        if let Some((txn, txn_mgr)) = &self.txn {
            let iter = self.iter.as_mut().expect("init is called before next");
            while tuple_batch.len() < batch_size && !iter.is_end() {
                let rid = iter.get_rid();
                iter.advance();
                let (meta, base_tuple, undo_link) = get_tuple_and_undo_link(txn_mgr, self.table_info, rid)?;
                let Some(logs) = collect_undo_logs(rid, &meta, &base_tuple, undo_link, txn, txn_mgr) else { continue };
                let Some(mut tuple) = reconstruct_tuple(&self.table_info.schema, &base_tuple, &meta, &logs) else { continue };
                if passes_filter(&self.filter_predicate, &self.plan.output_schema, &tuple)? {
                    tuple.set_rid(rid);
                    tuple_batch.push(tuple);
                    rid_batch.push(rid);
                }
            }
            return Ok(!tuple_batch.is_empty());
        }
        //~ // 4a-05: when self.txn is Some, scan with versions: for each rid, read the tuple, its meta and its undo link together (get_tuple_and_undo_link), collect_undo_logs for the transaction, reconstruct_tuple, skip the rids that are None, then filter and push as below
        // @end
        // @begin 3e-01
        let iter = self.iter.as_mut().expect("init is called before next");
        while tuple_batch.len() < batch_size && !iter.is_end() {
            let (meta, tuple) = iter.get_tuple()?;
            let rid = iter.get_rid();
            iter.advance();
            if !meta.is_deleted && passes_filter(&self.filter_predicate, &self.plan.output_schema, &tuple)? {
                tuple_batch.push(tuple);
                rid_batch.push(rid);
            }
        }
        Ok(!tuple_batch.is_empty())
        //~ todo!("3e-01: the batch holds the next live tuples that pass the filter, up to its size; true if it is not empty")
        // @end
    }

    fn output_schema(&self) -> &Schema {
        &self.plan.output_schema
    }
}
