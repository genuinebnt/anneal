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
    // @begin 3g-01
    order_bys.iter().map(|ob| ob.expr.evaluate(tuple, schema)).collect()
    //~ todo!("3g-01: the value of every order-by expression for this tuple, in order")
    // @end
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
        // @begin 3g-01
        for (i, ob) in self.order_bys.iter().enumerate() {
            let (x, y) = (&a[i], &b[i]);
            let ascending = ob.order_type != OrderByType::Desc;
            let ord = match (x.is_null(), y.is_null()) {
                (true, true) => Ordering::Equal,
                (false, false) => {
                    let o = Self::compare_values(x, y);
                    if ascending {
                        o
                    } else {
                        o.reverse()
                    }
                }
                (x_null, _) => {
                    let nulls_first = match ob.null_order {
                        OrderByNullType::NullsFirst => true,
                        OrderByNullType::NullsLast => false,
                        OrderByNullType::Default => ascending,
                    };
                    // the NULL one comes first iff NULLs go first
                    if x_null == nulls_first {
                        Ordering::Less
                    } else {
                        Ordering::Greater
                    }
                }
            };
            if ord != Ordering::Equal {
                return ord;
            }
        }
        Ordering::Equal
        //~ todo!("3g-01: the first key that differs decides: values by the key's direction, a NULL by the NULL rule; all equal is Equal")
        // @end
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
    // @begin 4a-03
    let mut values: Vec<Value> = (0..schema.column_count()).map(|i| base_tuple.get_value(schema, i)).collect();
    let mut deleted = base_meta.is_deleted;
    for log in undo_logs {
        if log.is_deleted {
            deleted = true;
            continue;
        }
        if deleted {
            values = null_values(schema);
            deleted = false;
        }
        let partial = get_undo_log_schema(schema, &log.modified_fields);
        let mut next = 0;
        for i in 0..schema.column_count() as usize {
            if log.modified_fields[i] {
                values[i] = log.tuple.get_value(&partial, next);
                next += 1;
            }
        }
    }
    if deleted {
        None
    } else {
        Some(Tuple::new(&values, schema))
    }
    //~ todo!("4a-03: the base tuple with every log applied in order: a deleting log removes the tuple, another restores its columns; None if deleted at the end")
    // @end
}

/// The undo logs that, applied in order to the version in the table, give the version `txn` can see: the newest one written at or before
/// its read timestamp, or one it wrote itself. `None` if the tuple did not exist for `txn` (every version is newer than its read
/// timestamp, or a log of the chain has been garbage collected).
///
/// `undo_link` is the head of the tuple's version chain.
pub fn collect_undo_logs(rid: Rid, base_meta: &TupleMeta, base_tuple: &Tuple, undo_link: Option<UndoLink>, txn: &Transaction, txn_mgr: &TransactionManager) -> Option<Vec<UndoLog>> {
    // @begin 4a-03
    // the version in the table is visible: committed no later than the read timestamp, or written by this very transaction
    if base_meta.ts <= txn.read_ts() || base_meta.ts == txn.temp_ts() {
        return Some(vec![]);
    }
    let mut logs = vec![];
    let mut link = undo_link;
    while let Some(current) = link.filter(|l| l.is_valid()) {
        let log = txn_mgr.get_undo_log_optional(current)?;
        let ts = log.ts;
        link = Some(log.prev_version);
        logs.push(log);
        if ts <= txn.read_ts() {
            return Some(logs);
        }
    }
    None
    //~ todo!("4a-03: the table's version is visible when its timestamp is at or before the read timestamp or it is this transaction's own; otherwise the chain up to the first log at or before it; None if the chain runs out or a log is gone")
    // @end
}

/// The undo log for the first change a transaction makes to a tuple. `base_tuple` is the tuple before the change (`None`: it did not
/// exist, so the log just says "did not exist"), `target_tuple` the tuple after it (`None`: a delete, so the log restores every column),
/// `ts` the timestamp of the base version and `prev_version` the log that was the head of the chain.
///
/// Otherwise the log restores exactly the columns whose values differ (compare with [`same_value`]), with `base_tuple`'s values.
pub fn generate_new_undo_log(schema: &Schema, base_tuple: Option<&Tuple>, target_tuple: Option<&Tuple>, ts: Timestamp, prev_version: UndoLink) -> UndoLog {
    // @begin 4a-04
    let n = schema.column_count();
    let Some(base) = base_tuple else {
        return UndoLog { is_deleted: true, modified_fields: vec![false; n as usize], tuple: Tuple::empty(), ts, prev_version };
    };
    let modified_fields: Vec<bool> = (0..n)
        .map(|i| match target_tuple {
            None => true,
            Some(target) => !same_value(&base.get_value(schema, i), &target.get_value(schema, i)),
        })
        .collect();
    let values: Vec<Value> = (0..n).filter(|&i| modified_fields[i as usize]).map(|i| base.get_value(schema, i)).collect();
    let tuple = Tuple::new(&values, &get_undo_log_schema(schema, &modified_fields));
    UndoLog { is_deleted: false, modified_fields, tuple, ts, prev_version }
    //~ todo!("4a-04: no base: a deleting log; no target: every column; otherwise the columns that differ, with the base tuple's values")
    // @end
}

/// The undo log that replaces `log` when the transaction that wrote it changes the tuple **again**: the same version (`ts` and
/// `prev_version` stay), restoring every column `log` restored and, in addition, the columns this change modifies (`base_tuple` holds
/// their values from before this change). A log that says "did not exist" stays as it is, and so does one for a tuple that is now
/// deleted (`base_tuple` is `None`): a deleting change makes the log restore every column.
pub fn generate_updated_undo_log(schema: &Schema, base_tuple: Option<&Tuple>, target_tuple: Option<&Tuple>, log: &UndoLog) -> UndoLog {
    // @begin 4a-04
    let Some(base) = base_tuple else { return log.clone() };
    if log.is_deleted {
        return log.clone();
    }
    let n = schema.column_count();
    let old_partial = get_undo_log_schema(schema, &log.modified_fields);
    let mut modified_fields = log.modified_fields.clone();
    let mut values = vec![];
    let mut next = 0;
    for i in 0..n {
        if log.modified_fields[i as usize] {
            values.push(log.tuple.get_value(&old_partial, next));
            next += 1;
            continue;
        }
        let changes = match target_tuple {
            None => true,
            Some(target) => !same_value(&base.get_value(schema, i), &target.get_value(schema, i)),
        };
        if changes {
            modified_fields[i as usize] = true;
            values.push(base.get_value(schema, i));
        }
    }
    let tuple = Tuple::new(&values, &get_undo_log_schema(schema, &modified_fields));
    UndoLog { is_deleted: false, modified_fields, tuple, ts: log.ts, prev_version: log.prev_version }
    //~ todo!("4a-04: a deleting log (or no base) stays; otherwise the log also restores the columns this change modifies, with their values from before it")
    // @end
}

/// Prints every tuple of `table` with its version chain to stderr, for finding out why a version is not what is expected:
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
    // @begin 4b-02
    meta.ts != txn.temp_ts() && meta.ts > txn.read_ts()
    //~ todo!("4b-02: a conflict if the tuple's ts is not this transaction's own temporary timestamp and is newer than its read timestamp")
    // @end
}

/// Taints `txn` and returns the error the statement fails with.
pub fn write_write_conflict(txn: &Transaction) -> Exception {
    txn.set_tainted();
    Exception::new(ExceptionType::Execution, "write-write conflict")
}

/// The transaction already holds an undo log for this tuple (the head of its chain is the transaction's own): widen it for this new
/// change instead of adding another (`generate_updated_undo_log`) and store it back with `Transaction::modify_undo_log`.
fn update_own_undo_log(txn: &Transaction, schema: &Schema, own: UndoLink, base: Option<&Tuple>, target: Option<&Tuple>) {
    // @begin 4b-03
    let idx = own.prev_log_idx as usize;
    let widened = generate_updated_undo_log(schema, base, target, &txn.get_undo_log(idx));
    txn.modify_undo_log(idx, widened);
    //~ todo!("4b-03: read the transaction's log own.prev_log_idx, generate the updated log for this change, and store it back")
    // @end
}

/// Changes the tuple at `rid` inside `txn`: to `target`, or deletes it if `target` is `None`. The tuple is updated **in place** (an
/// update needs a target of the same length) with the transaction's temporary timestamp, an undo log leaves the old version for
/// readers, and the rid joins the write set. Fails with a write-write conflict (and taints the transaction) if another transaction
/// wrote the tuple first.
pub fn modify_tuple(txn: &Arc<Transaction>, txn_mgr: &TransactionManager, table: &TableInfo<'_>, rid: Rid, target: Option<&Tuple>) -> Result<()> {
    let schema = &table.schema;
    let (meta, base_tuple, link) = get_tuple_and_undo_link(txn_mgr, table, rid)?;
    // @begin 4b-02
    if is_write_write_conflict(&meta, txn) {
        return Err(write_write_conflict(txn));
    }
    let base = if meta.is_deleted { None } else { Some(&base_tuple) };
    let new_meta = TupleMeta { ts: txn.temp_ts(), is_deleted: target.is_none() };
    // a delete keeps the old bytes: the tuple only gets marked
    let new_tuple = target.unwrap_or(&base_tuple);
    let own_log = link.filter(|l| l.is_valid() && l.prev_txn == txn.id());
    let new_link = if meta.ts == txn.temp_ts() {
        // written by this transaction before: its log (if it has one; a tuple it inserted has none) is widened
        if let Some(own) = own_log {
            update_own_undo_log(txn, schema, own, base, target);
        }
        link
    } else {
        // the first change: a log that restores the version being replaced
        Some(txn.append_undo_log(generate_new_undo_log(schema, base, target, meta.ts, link.unwrap_or_default())))
    };
    // the check runs again under the page latch: nobody may have written the tuple since it was read
    let written = update_tuple_and_undo_link(txn_mgr, table, rid, new_link, &new_meta, new_tuple, Some(&|m, _, _, _| !is_write_write_conflict(m, txn)))?;
    if !written {
        return Err(write_write_conflict(txn));
    }
    txn.append_write_set(table.oid, rid);
    Ok(())
    //~ todo!("4b-02: another transaction's write is a write-write conflict that taints this one; the first change leaves an undo log and a later one widens it; the tuple is written in place under the temporary timestamp and joins the write set")
    // @end
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
    // @begin 4b-06
    if let Some(pk) = indexes.iter().find(|i| i.is_primary_key) {
        let key = tuple.key_from_tuple(&table.schema, &pk.key_schema, pk.index.metadata().get_key_attrs());
        if let Some(&rid) = pk.index.scan_key(&key).first() {
            let (meta, _, _) = get_tuple_and_undo_link(txn_mgr, table, rid)?;
            if !meta.is_deleted {
                return Err(write_write_conflict(txn)); // the key is taken
            }
            return modify_tuple(txn, txn_mgr, table, rid, Some(tuple)); // a tombstone: reuse the rid
        }
        let rid = table.table.insert_tuple(&TupleMeta { ts: txn.temp_ts(), is_deleted: false }, tuple)?;
        txn.append_write_set(table.oid, rid);
        if !pk.index.insert_entry(&key, rid) {
            // another transaction inserted the key between the lookup and now: bury this tuple
            table.table.update_tuple_meta(&TupleMeta { ts: txn.temp_ts(), is_deleted: true }, rid)?;
            return Err(write_write_conflict(txn));
        }
        return Ok(());
    }
    //~ // 4b-06: with a primary-key index in indexes: look the key up (scan_key); a live tuple at its rid: taint and fail (write_write_conflict); a deleted one: reuse the rid with modify_tuple(.., Some(tuple)); no entry: insert the tuple (temp ts), add it to the write set, insert_entry, and if that fails bury the tuple (update_tuple_meta with is_deleted) and fail
    // @end
    // @begin 4b-01
    let rid = table.table.insert_tuple(&TupleMeta { ts: txn.temp_ts(), is_deleted: false }, tuple)?;
    txn.append_write_set(table.oid, rid);
    Ok(())
    //~ todo!("4b-01: insert the tuple with this transaction's temporary timestamp (not deleted) and add the rid to the write set")
    // @end
}
