//! The **sort key** of a tuple and the comparison of two sort entries. Sorting, top-N and window functions all order rows with these.

use std::cmp::Ordering;

use crate::binder::bound_order_by::{OrderBy, OrderByNullType, OrderByType};
use crate::catalog::schema::Schema;
use crate::common::exception::Result;
use crate::common::rid::Rid;
use crate::concurrency::transaction::{Timestamp, Transaction, UndoLink, UndoLog};
use crate::concurrency::transaction_manager::TransactionManager;
use crate::storage::table::tuple::{Tuple, TupleMeta};
use crate::types::value::{CmpBool, Value};

/// The values of a tuple's ORDER BY expressions.
pub type SortKey = Vec<Value>;
/// A sort key and the tuple it belongs to.
pub type SortEntry = (SortKey, Tuple);

/// Evaluates each ORDER BY expression on `tuple` (laid out with `schema`).
pub fn generate_sort_key(tuple: &Tuple, order_bys: &[OrderBy], schema: &Schema) -> Result<SortKey> {
    todo!("3g-01: the value of every order-by expression for this tuple, in order")
}

/// Orders sort entries by their keys according to the `ORDER BY` list.
#[derive(Clone, Debug)]
pub struct TupleComparator {
    order_bys: Vec<OrderBy>,
}

impl TupleComparator {
    pub fn new(order_bys: Vec<OrderBy>) -> TupleComparator {
        TupleComparator { order_bys }
    }

    /// Compares two non-NULL values (the same type): `Less`, `Equal` or `Greater`.
    fn compare_values(a: &Value, b: &Value) -> Ordering {
        if a.compare_equals(b) == Ok(CmpBool::True) {
            Ordering::Equal
        } else if a.compare_less_than(b) == Ok(CmpBool::True) {
            Ordering::Less
        } else {
            Ordering::Greater
        }
    }

    /// The order of two entries (by their keys). `Less` means `a` comes first.
    pub fn compare(&self, a: &SortEntry, b: &SortEntry) -> Ordering {
        self.compare_keys(&a.0, &b.0)
    }

    /// The order of two sort keys: key by key; the first key that differs decides. `Less` means `a` comes first.
    ///
    /// For each key: `Desc` reverses the order of values; a NULL is the **smallest** value (first when ascending, last when
    /// descending) unless the key says `NULLS FIRST` or `NULLS LAST`, which put NULLs first or last whatever the direction.
    pub fn compare_keys(&self, a: &SortKey, b: &SortKey) -> Ordering {
        todo!("3g-01: for each order-by key in turn: both NULL: equal; one NULL: by the NULL rule above; else compare the values (Value::compare_equals / compare_less_than), reversed for DESC; return the first result that is not Equal; Equal if all are")
    }
}

// ---- Project 4: versions ------------------------------------------------------------------------------------------------------------

/// The schema of an undo log's tuple: only the columns the log modifies, in table order.
pub fn get_undo_log_schema(schema: &Schema, modified_fields: &[bool]) -> Schema {
    let attrs: Vec<u32> = (0..schema.column_count()).filter(|&i| modified_fields[i as usize]).collect();
    Schema::copy_schema(schema, &attrs)
}

/// Whether two values are the same for versioning: two NULLs are, a NULL and a value are not.
pub fn same_value(a: &Value, b: &Value) -> bool {
    if a.is_null() || b.is_null() {
        return a.is_null() && b.is_null();
    }
    a.compare_equals(b) == Ok(CmpBool::True)
}

/// A tuple of NULLs, one per column.
pub fn null_values(schema: &Schema) -> Vec<Value> {
    schema.columns().iter().map(|c| Value::null(c.type_id())).collect()
}

/// The tuple as of an older version: start from `base_tuple` (the version in the table) and apply `undo_logs` in order, **all of them**,
/// whatever their timestamps. `None` if the result is a deleted tuple.
///
/// A log that `is_deleted` makes the tuple not exist. A log that restores columns writes their old values into the tuple; its tuple
/// holds only those columns (see [`get_undo_log_schema`]). A tuple that does not exist (a deleted base, or after a deleting log) and is
/// then restored by a log starts from all NULLs: such a log restores every column.
pub fn reconstruct_tuple(schema: &Schema, base_tuple: &Tuple, base_meta: &TupleMeta, undo_logs: &[UndoLog]) -> Option<Tuple> {
    todo!("4a-03: values of the base tuple; deleted = base_meta.is_deleted; for each log in order: a deleting log sets deleted; otherwise (if deleted, restart from null_values) copy the log's partial tuple into the columns it modifies. None if deleted at the end")
}

/// The undo logs that, applied in order to the version in the table, give the version `txn` can see: the newest one written at or before
/// its read timestamp, or one it wrote itself. `None` if the tuple did not exist for `txn` (every version is newer than its read
/// timestamp, or a log of the chain has been garbage collected).
///
/// `undo_link` is the head of the tuple's version chain.
pub fn collect_undo_logs(rid: Rid, base_meta: &TupleMeta, base_tuple: &Tuple, undo_link: Option<UndoLink>, txn: &Transaction, txn_mgr: &TransactionManager) -> Option<Vec<UndoLog>> {
    todo!("4a-03: the table's version is visible if its ts <= read_ts or it is this transaction's own temp ts: no logs; otherwise follow the chain from undo_link, collecting logs until one has ts <= read_ts; None if the chain ends first or a log is gone")
}

/// The undo log for the first change a transaction makes to a tuple. `base_tuple` is the tuple before the change (`None`: it did not
/// exist, so the log just says "did not exist"), `target_tuple` the tuple after it (`None`: a delete, so the log restores every column),
/// `ts` the timestamp of the base version and `prev_version` the log that was the head of the chain.
///
/// Otherwise the log restores exactly the columns whose values differ (compare with [`same_value`]), with `base_tuple`'s values.
pub fn generate_new_undo_log(schema: &Schema, base_tuple: Option<&Tuple>, target_tuple: Option<&Tuple>, ts: Timestamp, prev_version: UndoLink) -> UndoLog {
    todo!("4a-04: no base: a deleting log (no fields, empty tuple); no target: every column; else the columns that differ, with the base tuple's values in a tuple under the partial schema")
}

/// The undo log that replaces `log` when the transaction that wrote it changes the tuple **again**: the same version (`ts` and
/// `prev_version` stay), restoring every column `log` restored and, in addition, the columns this change modifies (`base_tuple` holds
/// their values from before this change). A log that says "did not exist" stays as it is, and so does one for a tuple that is now
/// deleted (`base_tuple` is `None`): a deleting change makes the log restore every column.
pub fn generate_updated_undo_log(schema: &Schema, base_tuple: Option<&Tuple>, target_tuple: Option<&Tuple>, log: &UndoLog) -> UndoLog {
    todo!("4a-04: keep a deleting log, and any log when base is None; else walk the columns: one the log already restores keeps its logged value; another that this change modifies (or all of them, for a delete) is added with the base tuple's value; rebuild the partial tuple")
}

/// Prints every tuple of `table` with its version chain to stderr, for finding out why a version is not what you expect:
///
/// ```text
/// debug_hook: before the scan
/// RID=0/0 ts=txn8 tuple=(1, <NULL>, <NULL>)
///   txn8@0 (2, _, _) ts=1
/// RID=0/1 ts=3 <del marker> tuple=(<NULL>, <NULL>, <NULL>)
///   txn5@0 <del> ts=2
/// ```
pub fn txn_mgr_dbg(info: &str, txn_mgr: &TransactionManager, table: &crate::catalog::catalog::TableInfo<'_>) {
    use crate::concurrency::transaction::TXN_START_ID;
    let ts_name = |ts: Timestamp| if ts >= TXN_START_ID { format!("txn{}", ts ^ TXN_START_ID) } else { ts.to_string() };
    eprintln!("debug_hook: {info}");
    let mut iter = table.table.make_eager_iterator();
    while !iter.is_end() {
        let rid = iter.get_rid();
        iter.advance();
        let Ok((meta, tuple)) = table.table.get_tuple(rid) else { continue };
        let marker = if meta.is_deleted { " <del marker>" } else { "" };
        eprintln!("RID={}/{} ts={}{marker} tuple={}", rid.page_id().0, rid.slot_num(), ts_name(meta.ts), tuple.to_string(&table.schema));
        let mut link = txn_mgr.get_undo_link(rid);
        while let Some(l) = link.filter(|l| l.is_valid()) {
            let Some(log) = txn_mgr.get_undo_log_optional(l) else {
                eprintln!("  txn{}@{} <garbage collected>", l.prev_txn ^ TXN_START_ID, l.prev_log_idx);
                break;
            };
            let body = if log.is_deleted {
                "<del>".to_string()
            } else {
                let partial = get_undo_log_schema(&table.schema, &log.modified_fields);
                let mut next = 0;
                let cells: Vec<String> = log
                    .modified_fields
                    .iter()
                    .map(|&m| {
                        if !m {
                            return "_".to_string();
                        }
                        let v = log.tuple.get_value(&partial, next);
                        next += 1;
                        if v.is_null() { "<NULL>".to_string() } else { v.to_string() }
                    })
                    .collect();
                format!("({})", cells.join(", "))
            };
            eprintln!("  txn{}@{} {body} ts={}", l.prev_txn ^ TXN_START_ID, l.prev_log_idx, ts_name(log.ts));
            link = Some(log.prev_version);
        }
    }
}

// ---- Project 4: writing versions ----------------------------------------------------------------------------------------------------------

use std::sync::Arc;

use crate::catalog::catalog::TableInfo;
use crate::common::exception::{Exception, ExceptionType};
use crate::concurrency::transaction_manager::{get_tuple_and_undo_link, update_tuple_and_undo_link};

/// Would `txn` collide with another transaction by writing a tuple whose metadata is `meta`? Yes if the tuple's version is neither
/// the transaction's own write nor one it can see: it carries another transaction's temporary timestamp (uncommitted), or a commit
/// timestamp after `txn`'s read timestamp (committed since `txn` began). Both are larger than the read timestamp.
pub fn is_write_write_conflict(meta: &TupleMeta, txn: &Transaction) -> bool {
    todo!("4b-02: a conflict if the tuple's ts is not this transaction's own temporary timestamp and is newer than its read timestamp")
}

/// Taints `txn` and returns the error the statement fails with.
pub fn write_write_conflict(txn: &Transaction) -> Exception {
    txn.set_tainted();
    Exception::new(ExceptionType::Execution, "write-write conflict")
}

/// The transaction already holds an undo log for this tuple (the head of its chain is the transaction's own): widen it for this new
/// change instead of adding another (`generate_updated_undo_log`) and store it back with `Transaction::modify_undo_log`.
fn update_own_undo_log(txn: &Transaction, schema: &Schema, own: UndoLink, base: Option<&Tuple>, target: Option<&Tuple>) {
    todo!("4b-03: read the transaction's log own.prev_log_idx, generate the updated log for this change, and store it back")
}

/// Changes the tuple at `rid` inside `txn`: to `target`, or deletes it if `target` is `None`. The tuple is updated **in place** (an
/// update needs a target of the same length) with the transaction's temporary timestamp, an undo log leaves the old version for
/// readers, and the rid joins the write set. Fails with a write-write conflict (and taints the transaction) if another transaction
/// wrote the tuple first.
pub fn modify_tuple(txn: &Arc<Transaction>, txn_mgr: &TransactionManager, table: &TableInfo<'_>, rid: Rid, target: Option<&Tuple>) -> Result<()> {
    let schema = &table.schema;
    let (meta, base_tuple, link) = get_tuple_and_undo_link(txn_mgr, table, rid)?;
    todo!("4b-02: read the tuple with its link (above); conflict check (taint and Err); the first change appends generate_new_undo_log (prev = the old head link), a tuple this transaction wrote already keeps its link (4b-03: its log is widened); write the meta (temp ts; deleted if target is None), the new bytes (a delete keeps the old ones) and the link with update_tuple_and_undo_link, whose check repeats the conflict test; then add the rid to the write set")
}

/// The version of the tuple at `rid` that `txn` may see, with its metadata; `None` if it did not exist for `txn`.
///
/// Reads the tuple and its link together, collects the undo logs and reconstructs, as a sequential scan does.
pub fn read_visible_version(txn: &Transaction, txn_mgr: &TransactionManager, table: &TableInfo<'_>, rid: Rid) -> Result<Option<Tuple>> {
    let (meta, base_tuple, link) = get_tuple_and_undo_link(txn_mgr, table, rid)?;
    let Some(logs) = collect_undo_logs(rid, &meta, &base_tuple, link, txn, txn_mgr) else { return Ok(None) };
    Ok(reconstruct_tuple(&table.schema, &base_tuple, &meta, &logs).map(|mut t| {
        t.set_rid(rid);
        t
    }))
}

/// The predicate "true": what a serializable scan with no filter records, since it read everything.
pub fn true_predicate() -> crate::execution::expressions::abstract_expression::ExprRef {
    use crate::execution::expressions::constant_value_expression::ConstantValueExpression;
    Arc::new(ConstantValueExpression::new(Value::boolean(true)))
}

/// Inserts `tuple` into `table` inside `txn`. A table with a primary-key index goes through the index: an existing live tuple under the
/// key is a duplicate (the transaction is tainted and the statement fails); a tombstone under the key is **reused** (the same rid
/// becomes live again, as a normal in-place change, [`modify_tuple`]); a new key gets a new tuple and then its entry, and if the entry
/// cannot be added (another transaction took the key first) the new tuple is buried and the statement fails. Without a primary key the
/// tuple is simply added. The tuple carries the transaction's temporary timestamp and its rid joins the write set.
pub fn insert_mvcc(txn: &Arc<Transaction>, txn_mgr: &TransactionManager, table: &TableInfo<'_>, indexes: &[Arc<crate::catalog::catalog::IndexInfo<'_>>], tuple: &Tuple) -> Result<()> {
    // 4b-06: with a primary-key index in indexes: look the key up (scan_key); a live tuple at its rid: taint and fail (write_write_conflict); a deleted one: reuse the rid with modify_tuple(.., Some(tuple)); no entry: insert the tuple (temp ts), add it to the write set, insert_entry, and if that fails bury the tuple (update_tuple_meta with is_deleted) and fail
    todo!("4b-01: insert the tuple with this transaction's temporary timestamp (not deleted) and add the rid to the write set")
}
