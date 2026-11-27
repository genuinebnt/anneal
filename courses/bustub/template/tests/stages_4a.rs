//! Tests for module 4a: timestamps, transactions and version chains.

use std::sync::Arc;

use proptest::prelude::*;

use bustub::catalog::catalog::TableInfo;
use bustub::catalog::column::Column;
use bustub::catalog::schema::Schema;
use bustub::common::bustub_instance::BusTubInstance;
use bustub::common::result_writer::SimpleStreamWriter;
use bustub::common::rid::Rid;
use bustub::concurrency::transaction::{IsolationLevel, Transaction, TransactionState, UndoLink, UndoLog, INVALID_TS, TXN_START_ID};
use bustub::concurrency::watermark::Watermark;
use bustub::execution::execution_common::{txn_mgr_dbg, collect_undo_logs, generate_new_undo_log, generate_updated_undo_log, get_undo_log_schema, reconstruct_tuple};
use bustub::storage::table::tuple::{Tuple, TupleMeta};
use bustub::types::type_id::TypeId;
use bustub::types::value::Value;

const SI: IsolationLevel = IsolationLevel::SnapshotIsolation;

fn int(v: i32) -> Value {
    Value::integer(v)
}
fn int_null() -> Value {
    Value::null(TypeId::Integer)
}
fn dbl(v: f64) -> Value {
    Value::decimal(v)
}
fn dbl_null() -> Value {
    Value::null(TypeId::Decimal)
}
fn boolean(v: bool) -> Value {
    Value::boolean(v)
}
fn bool_null() -> Value {
    Value::null(TypeId::Boolean)
}

/// `a integer, b double, c boolean`
fn abc() -> Schema {
    Schema::new(vec![Column::new("a", TypeId::Integer), Column::new("b", TypeId::Decimal), Column::new("c", TypeId::Boolean)])
}
fn schema_of(cols: &[(&str, TypeId)]) -> Schema {
    Schema::new(cols.iter().map(|(n, t)| Column::new(n, *t)).collect())
}

fn new_db() -> BusTubInstance {
    BusTubInstance::new(64)
}

fn begin(db: &BusTubInstance) -> Arc<Transaction> {
    db.txn_manager.begin(SI).unwrap()
}

fn commit(db: &BusTubInstance, txn: &Arc<Transaction>) {
    assert!(db.txn_manager.commit(txn).unwrap(), "commit failed");
}

fn maintable(db: &BusTubInstance) -> Arc<TableInfo<'static>> {
    db.catalog.write().unwrap().create_table("maintable", &abc()).unwrap()
}

fn meta(ts: i64, is_deleted: bool) -> TupleMeta {
    TupleMeta { ts, is_deleted }
}

fn insert(table: &TableInfo<'_>, m: TupleMeta, values: &[Value]) -> Rid {
    table.table.insert_tuple(&m, &Tuple::new(values, &table.schema)).unwrap()
}

/// Runs SQL inside a transaction and returns the rows as lines of space separated cells (sorted).
fn query(db: &BusTubInstance, txn: &Arc<Transaction>, sql: &str) -> Vec<String> {
    let mut out = String::new();
    assert!(db.execute_sql_txn(sql, &mut SimpleStreamWriter::new(&mut out, true, " "), txn).unwrap_or_else(|e| panic!("{sql}: {e}")), "{sql} failed");
    let mut rows: Vec<String> = out.lines().map(|l| l.trim_end().to_string()).collect();
    rows.sort();
    rows
}

fn verify(schema: &Schema, tuple: &Tuple, expected: &[Value]) {
    for (i, want) in expected.iter().enumerate() {
        let got = tuple.get_value(schema, i as u32);
        let same = (got.is_null() && want.is_null()) || got.compare_equals(want) == Ok(bustub::types::value::CmpBool::True);
        assert!(same, "column {i}: got {got}, expected {want} (tuple {})", tuple.to_string(schema));
    }
}

fn undo_log(is_deleted: bool, modified: &[bool], partial: &Schema, values: &[Value], ts: i64, prev: UndoLink) -> UndoLog {
    UndoLog { is_deleted, modified_fields: modified.to_vec(), tuple: Tuple::new(values, partial), ts, prev_version: prev }
}

// ---- 4a-01: the watermark ----------------------------------------------------------------------------------------------------------

#[test]
fn s4a_01_with_no_readers_the_watermark_is_the_last_commit() {
    let mut w = Watermark::new(0);
    assert_eq!(w.get_watermark(), 0, "with no readers the watermark is the last commit");
    w.update_commit_ts(7);
    assert_eq!(w.get_watermark(), 7, "with no readers the watermark is the last commit");
}

#[test]
fn s4a_01_the_watermark_is_the_smallest_read_timestamp() {
    let mut w = Watermark::new(0);
    w.add_txn(3).unwrap();
    w.add_txn(1).unwrap();
    w.add_txn(2).unwrap();
    assert_eq!(w.get_watermark(), 1, "the watermark is the smallest read timestamp");
}

#[test]
fn s4a_01_removing_the_smallest_moves_the_watermark_up() {
    let mut w = Watermark::new(0);
    for ts in [1, 2, 5] {
        w.add_txn(ts).unwrap();
    }
    w.remove_txn(1);
    assert_eq!(w.get_watermark(), 2, "removing the smallest moves the watermark up");
    w.remove_txn(2);
    assert_eq!(w.get_watermark(), 5, "removing the smallest moves the watermark up");
}

#[test]
fn s4a_01_removing_another_reader_leaves_the_watermark_alone() {
    let mut w = Watermark::new(0);
    for ts in [1, 2, 5] {
        w.add_txn(ts).unwrap();
    }
    w.remove_txn(5);
    w.remove_txn(2);
    assert_eq!(w.get_watermark(), 1, "removing another reader leaves the watermark alone");
}

#[test]
fn s4a_01_two_readers_at_the_same_timestamp_are_counted() {
    let mut w = Watermark::new(0);
    w.add_txn(4).unwrap();
    w.add_txn(4).unwrap();
    w.remove_txn(4);
    assert_eq!(w.get_watermark(), 4, "one reader is still there");
    w.update_commit_ts(9);
    w.remove_txn(4);
    assert_eq!(w.get_watermark(), 9, "none is left");
}

#[test]
fn s4a_01_a_reader_older_than_the_last_commit_is_refused() {
    let mut w = Watermark::new(5);
    let err = w.add_txn(4).unwrap_err();
    assert!(err.message.contains("read ts < commit ts"), "{err}");
    assert!(w.add_txn(5).is_ok(), "a reader older than the last commit is refused: expected `w.add_txn(5).is_ok()`");
}

#[test]
fn s4a_01_a_million_transactions_in_either_order() {
    let n = 1_000_000;
    let mut w = Watermark::new(0);
    for i in 0..n {
        w.add_txn(i).unwrap();
        assert_eq!(w.get_watermark(), 0, "a million transactions in either order");
    }
    for i in 0..n {
        w.update_commit_ts(i + 1);
        w.remove_txn(i);
        assert_eq!(w.get_watermark(), i + 1, "a million transactions in either order");
    }
    let mut w = Watermark::new(0);
    for i in 0..n {
        w.add_txn(i).unwrap();
    }
    for i in 0..n {
        w.update_commit_ts(i + 1);
        w.remove_txn(n - i - 1);
        assert_eq!(w.get_watermark(), if i == n - 1 { n } else { 0 }, "a million transactions in either order");
    }
}

// ---- 4a-02: beginning a transaction ------------------------------------------------------------------------------------------------

#[test]
fn s4a_02_transaction_ids_start_at_two_to_the_62_and_count_up() {
    let db = new_db();
    let (t0, t1, t2) = (begin(&db), begin(&db), begin(&db));
    assert_eq!(t0.id(), TXN_START_ID, "transaction ids start at two to the 62 and count up");
    assert_eq!(t1.id(), TXN_START_ID + 1, "transaction ids start at two to the 62 and count up");
    assert_eq!(t2.id(), TXN_START_ID + 2, "transaction ids start at two to the 62 and count up");
    assert_eq!((t0.human_readable_id(), t2.human_readable_id()), (0, 2), "transaction ids start at two to the 62 and count up");
}

#[test]
fn s4a_02_a_new_transaction_is_running_and_has_not_committed() {
    let db = new_db();
    let t = db.txn_manager.begin(IsolationLevel::Serializable).unwrap();
    assert_eq!(t.state(), TransactionState::Running, "a new transaction is running and has not committed");
    assert_eq!(t.commit_ts(), INVALID_TS, "a new transaction is running and has not committed");
    assert_eq!(t.isolation_level(), IsolationLevel::Serializable, "a new transaction is running and has not committed");
    assert_eq!(t.temp_ts(), t.id(), "a new transaction is running and has not committed");
}

#[test]
fn s4a_02_in_a_new_database_everything_reads_at_timestamp_zero() {
    let db = new_db();
    assert_eq!(begin(&db).read_ts(), 0, "in a new database everything reads at timestamp zero");
    assert_eq!(begin(&db).read_ts(), 0, "in a new database everything reads at timestamp zero");
}

#[test]
fn s4a_02_the_transaction_manager_remembers_the_transaction() {
    let db = new_db();
    let t = begin(&db);
    let found = db.txn_manager.get_txn(t.id()).expect("begin puts the transaction in txn_map");
    assert!(Arc::ptr_eq(&t, &found), "the transaction manager remembers the transaction: expected `Arc::ptr_eq(&t, &found)`");
    assert!(db.txn_manager.get_txn(t.id() + 1).is_none(), "the transaction manager remembers the transaction: expected `db.txn_manager.get_txn(t.id() + 1).is_none()`");
}

#[test]
fn s4a_02_a_running_transaction_is_registered_with_the_watermark() {
    let db = new_db();
    assert_eq!(db.txn_manager.get_watermark(), 0, "a running transaction is registered with the watermark");
    let _t = begin(&db);
    // a second transaction cannot move the watermark above the first: both read at 0
    let _u = begin(&db);
    assert_eq!(db.txn_manager.get_watermark(), 0, "a running transaction is registered with the watermark");
}

// ---- 4a-02: committing and aborting ---------------------------------------------------------------------------------------------------

#[test]
fn s4a_02_commit_timestamps_count_up_from_one() {
    let db = new_db();
    for expected in 1..=3 {
        let t = begin(&db);
        commit(&db, &t);
        assert_eq!(t.commit_ts(), expected, "commit timestamps count up from one");
        assert_eq!(t.state(), TransactionState::Committed, "commit timestamps count up from one");
    }
}

#[test]
fn s4a_02_a_transaction_that_begins_after_a_commit_reads_it() {
    let db = new_db();
    let early = begin(&db);
    let t = begin(&db);
    commit(&db, &t);
    let late = begin(&db);
    assert_eq!((early.read_ts(), late.read_ts()), (0, 1), "a transaction that begins after a commit reads it");
}

#[test]
fn s4a_02_commit_stamps_the_tuples_in_the_write_set() {
    let db = new_db();
    let table = maintable(&db);
    let t = begin(&db);
    let r1 = insert(&table, meta(t.temp_ts(), false), &[int(1), dbl(1.0), bool_null()]);
    let r2 = insert(&table, meta(t.temp_ts(), true), &[int(2), dbl(2.0), bool_null()]);
    t.append_write_set(table.oid, r1);
    t.append_write_set(table.oid, r2);
    commit(&db, &t);
    assert_eq!(table.table.get_tuple_meta(r1).unwrap(), meta(1, false), "commit stamps the tuples in the write set");
    assert_eq!(table.table.get_tuple_meta(r2).unwrap(), meta(1, true), "a deleted tuple stays deleted");
}

#[test]
fn s4a_02_commit_leaves_other_tuples_alone() {
    let db = new_db();
    let table = maintable(&db);
    let t = begin(&db);
    let mine = insert(&table, meta(t.temp_ts(), false), &[int(1), dbl(1.0), bool_null()]);
    let other = insert(&table, meta(77, false), &[int(2), dbl(2.0), bool_null()]);
    t.append_write_set(table.oid, mine);
    commit(&db, &t);
    assert_eq!(table.table.get_tuple_meta(other).unwrap().ts, 77, "commit leaves other tuples alone");
}

#[test]
fn s4a_02_a_tainted_transaction_cannot_commit() {
    let db = new_db();
    let t = begin(&db);
    t.set_tainted();
    assert_eq!(db.txn_manager.commit(&t).unwrap(), false, "a tainted transaction cannot commit");
    assert_eq!(t.state(), TransactionState::Tainted, "a tainted transaction cannot commit");
    assert_eq!(db.txn_manager.get_watermark(), 0, "a tainted transaction cannot commit");
}

#[test]
fn s4a_02_committing_twice_is_an_error() {
    let db = new_db();
    let t = begin(&db);
    commit(&db, &t);
    assert!(db.txn_manager.commit(&t).is_err(), "committing twice is an error: expected `db.txn_manager.commit(&t).is_err()`");
}

#[test]
fn s4a_02_commit_moves_the_watermark() {
    let db = new_db();
    let a = begin(&db);
    let b = begin(&db);
    commit(&db, &b);
    assert_eq!(db.txn_manager.get_watermark(), 0, "a still reads at 0");
    commit(&db, &a);
    assert_eq!(db.txn_manager.get_watermark(), 2, "commit moves the watermark");
}

#[test]
fn s4a_02_abort_ends_the_transaction_and_releases_its_read_timestamp() {
    let db = new_db();
    let a = begin(&db);
    let b = begin(&db);
    commit(&db, &b);
    db.txn_manager.abort(&a).unwrap();
    assert_eq!(a.state(), TransactionState::Aborted, "abort ends the transaction and releases its read timestamp");
    assert_eq!(db.txn_manager.get_watermark(), 1, "abort ends the transaction and releases its read timestamp");
}

#[test]
fn s4a_02_a_tainted_transaction_can_be_aborted_but_a_finished_one_cannot() {
    let db = new_db();
    let t = begin(&db);
    t.set_tainted();
    db.txn_manager.abort(&t).unwrap();
    assert_eq!(t.state(), TransactionState::Aborted, "a tainted transaction can be aborted but a finished one cannot");
    assert!(db.txn_manager.abort(&t).is_err(), "a tainted transaction can be aborted but a finished one cannot: expected `db.txn_manager.abort(&t).is_err()`");
    let u = begin(&db);
    commit(&db, &u);
    assert!(db.txn_manager.abort(&u).is_err(), "a tainted transaction can be aborted but a finished one cannot: expected `db.txn_manager.abort(&u).is_err()`");
}

// ---- 4a-03: reconstructing a tuple ----------------------------------------------------------------------------------------------------

#[test]
fn s4a_03_no_logs_gives_the_base_tuple() {
    let s = abc();
    let base = Tuple::new(&[int(0), dbl(1.0), bool_null()], &s);
    let t = reconstruct_tuple(&s, &base, &meta(2333, false), &[]).unwrap();
    verify(&s, &t, &[int(0), dbl(1.0), bool_null()]);
}

#[test]
fn s4a_03_a_deleted_base_with_no_logs_does_not_exist() {
    let s = abc();
    let base = Tuple::new(&[int_null(), dbl_null(), bool_null()], &s);
    assert!(reconstruct_tuple(&s, &base, &meta(2333, true), &[]).is_none(), "a deleted base with no logs does not exist: expected `reconstruct_tuple(&s, &base, &meta(2333, true), &[]).is_none()`");
}

#[test]
fn s4a_03_a_full_log_over_a_deleted_base_brings_the_tuple_back() {
    let s = abc();
    let base = Tuple::new(&[int_null(), dbl_null(), bool_null()], &s);
    let log = undo_log(false, &[true, true, true], &s, &[int(1), dbl(2.0), boolean(false)], 1, UndoLink::default());
    let t = reconstruct_tuple(&s, &base, &meta(2333, true), &[log]).unwrap();
    verify(&s, &t, &[int(1), dbl(2.0), boolean(false)]);
}

#[test]
fn s4a_03_a_tuple_that_did_not_exist_starts_from_nulls_not_from_the_stale_bytes_in_the_table() {
    let s = abc();
    // the slot of a deleted tuple still holds its last values; a log that brings the tuple back must not leak them
    let stale = Tuple::new(&[int(7), dbl(7.5), boolean(true)], &s);
    let only_a = undo_log(false, &[true, false, false], &schema_of(&[("a", TypeId::Integer)]), &[int(1)], 1, UndoLink::default());
    let t = reconstruct_tuple(&s, &stale, &meta(2333, true), &[only_a]).unwrap();
    verify(&s, &t, &[int(1), dbl_null(), bool_null()]);
    // after a deleting log in the middle of a chain the same holds
    let delete = undo_log(true, &[false, false, false], &schema_of(&[]), &[], 2, UndoLink::default());
    let restore_c = undo_log(false, &[false, false, true], &schema_of(&[("c", TypeId::Boolean)]), &[boolean(false)], 1, UndoLink::default());
    let live = Tuple::new(&[int(5), dbl(5.5), boolean(true)], &s);
    let t = reconstruct_tuple(&s, &live, &meta(9, false), &[delete, restore_c]).unwrap();
    verify(&s, &t, &[int_null(), dbl_null(), boolean(false)]);
}

#[test]
fn s4a_03_partial_logs_restore_only_their_columns() {
    let s = abc();
    let base = Tuple::new(&[int(0), dbl(1.0), bool_null()], &s);
    let none = undo_log(false, &[false, false, false], &schema_of(&[]), &[], 1, UndoLink::default());
    let b_only = undo_log(false, &[false, true, false], &schema_of(&[("b", TypeId::Decimal)]), &[dbl(2.0)], 1, UndoLink::default());
    let a_and_c = undo_log(false, &[true, false, true], &schema_of(&[("a", TypeId::Integer), ("c", TypeId::Boolean)]), &[int(3), boolean(false)], 1, UndoLink::default());
    let m = meta(2333, false);
    verify(&s, &reconstruct_tuple(&s, &base, &m, &[none.clone()]).unwrap(), &[int(0), dbl(1.0), bool_null()]);
    verify(&s, &reconstruct_tuple(&s, &base, &m, &[none.clone(), b_only.clone()]).unwrap(), &[int(0), dbl(2.0), bool_null()]);
    verify(&s, &reconstruct_tuple(&s, &base, &m, &[none, b_only, a_and_c]).unwrap(), &[int(3), dbl(2.0), boolean(false)]);
}

#[test]
fn s4a_03_the_logs_are_applied_in_order_so_the_last_one_wins() {
    let s = abc();
    let base = Tuple::new(&[int(0), dbl(1.0), bool_null()], &s);
    let mk = |a: i32| undo_log(false, &[true, true, true], &s, &[int(a), dbl(a as f64), boolean(true)], 1, UndoLink::default());
    let t = reconstruct_tuple(&s, &base, &meta(9, false), &[mk(1), mk(2), mk(4)]).unwrap();
    verify(&s, &t, &[int(4), dbl(4.0), boolean(true)]);
}

#[test]
fn s4a_03_a_deleting_log_makes_the_tuple_not_exist_until_a_later_log_restores_it() {
    let s = abc();
    let base = Tuple::new(&[int(0), dbl(1.0), bool_null()], &s);
    let m = meta(9, false);
    let del = undo_log(true, &[false, false, false], &schema_of(&[]), &[], 1, UndoLink::default());
    let full = undo_log(false, &[true, true, true], &s, &[int(1), dbl(1.0), boolean(false)], 1, UndoLink::default());
    let nulls = undo_log(false, &[true, true, true], &s, &[int_null(), dbl_null(), bool_null()], 1, UndoLink::default());
    assert!(reconstruct_tuple(&s, &base, &m, &[del.clone()]).is_none(), "a deleting log makes the tuple not exist until a later log restores it: expected `reconstruct_tuple(&s, &base, &m, &[del.clone()]).is_none()`");
    verify(&s, &reconstruct_tuple(&s, &base, &m, &[del.clone(), full.clone()]).unwrap(), &[int(1), dbl(1.0), boolean(false)]);
    assert!(reconstruct_tuple(&s, &base, &m, &[del.clone(), full.clone(), del.clone()]).is_none(), "a deleting log makes the tuple not exist until a later log restores it: expected `reconstruct_tuple(&s, &base, &m, &[del.clone(), full.clone(), del.clone()]).is_none()`");
    verify(&s, &reconstruct_tuple(&s, &base, &m, &[del.clone(), full, del, nulls]).unwrap(), &[int_null(), dbl_null(), bool_null()]);
}

// ---- 4a-03: collecting the undo logs a transaction needs ---------------------------------------------------------------------------------

/// A table row at `ts` with an optional chain of (ts, value of a) undo logs, newest first, stored in `owner`.
fn row_with_chain(db: &BusTubInstance, table: &TableInfo<'_>, owner: &Arc<Transaction>, ts: i64, a: i32, chain: &[(i64, i32)]) -> Rid {
    let rid = insert(table, meta(ts, false), &[int(a), dbl(a as f64), bool_null()]);
    let partial = schema_of(&[("a", TypeId::Integer)]);
    let mut link = UndoLink::default();
    for &(log_ts, value) in chain.iter().rev() {
        link = owner.append_undo_log(undo_log(false, &[true, false, false], &partial, &[int(value)], log_ts, link));
    }
    if link.is_valid() {
        db.txn_manager.update_undo_link(rid, Some(link), None);
    }
    rid
}

fn collect(db: &BusTubInstance, table: &TableInfo<'_>, rid: Rid, txn: &Transaction) -> Option<Vec<UndoLog>> {
    let (m, tuple) = table.table.get_tuple(rid).unwrap();
    collect_undo_logs(rid, &m, &tuple, db.txn_manager.get_undo_link(rid), txn, &db.txn_manager)
}

#[test]
fn s4a_03_a_tuple_committed_at_or_before_the_read_timestamp_needs_no_logs() {
    let db = new_db();
    let table = maintable(&db);
    let owner = begin(&db);
    let t1 = begin(&db);
    commit(&db, &t1);
    let reader = begin(&db);
    assert_eq!(reader.read_ts(), 1, "a tuple committed at or before the read timestamp needs no logs");
    let old = row_with_chain(&db, &table, &owner, 0, 1, &[]);
    let exact = row_with_chain(&db, &table, &owner, 1, 2, &[]);
    assert_eq!(collect(&db, &table, old, &reader).unwrap().len(), 0, "a tuple committed at or before the read timestamp needs no logs");
    assert_eq!(collect(&db, &table, exact, &reader).unwrap().len(), 0, "a tuple committed at or before the read timestamp needs no logs");
}

#[test]
fn s4a_03_a_tuple_the_transaction_wrote_itself_needs_no_logs() {
    let db = new_db();
    let table = maintable(&db);
    let _owner = begin(&db);
    let reader = begin(&db);
    let own = row_with_chain(&db, &table, &reader, reader.temp_ts(), 1, &[(0, 5)]);
    assert_eq!(collect(&db, &table, own, &reader).unwrap().len(), 0, "the newest version is its own");
}

#[test]
fn s4a_03_a_newer_tuple_without_a_chain_did_not_exist_yet() {
    let db = new_db();
    let table = maintable(&db);
    let owner = begin(&db);
    let reader = begin(&db);
    let newer = row_with_chain(&db, &table, &owner, 5, 1, &[]);
    assert!(collect(&db, &table, newer, &reader).is_none(), "a newer tuple without a chain did not exist yet: expected `collect(&db, &table, newer, &reader).is_none()`");
}

#[test]
fn s4a_03_another_transactions_uncommitted_tuple_is_not_visible() {
    let db = new_db();
    let table = maintable(&db);
    let writer = begin(&db);
    let reader = begin(&db);
    let theirs = row_with_chain(&db, &table, &writer, writer.temp_ts(), 1, &[]);
    assert!(collect(&db, &table, theirs, &reader).is_none(), "another transactions uncommitted tuple is not visible: expected `collect(&db, &table, theirs, &reader).is_none()`");
}

#[test]
fn s4a_03_the_chain_is_followed_until_a_version_old_enough() {
    let db = new_db();
    let table = maintable(&db);
    let owner = begin(&db);
    for _ in 0..3 {
        let t = begin(&db);
        commit(&db, &t);
    }
    let reader = begin(&db);
    assert_eq!(reader.read_ts(), 3, "the chain is followed until a version old enough");
    // table ts 9, then logs for versions at 7, 3 (visible), 1
    let rid = row_with_chain(&db, &table, &owner, 9, 100, &[(7, 70), (3, 30), (1, 10)]);
    let logs = collect(&db, &table, rid, &reader).unwrap();
    assert_eq!(logs.iter().map(|l| l.ts).collect::<Vec<_>>(), vec![7, 3], "stop at the first log with ts <= 3");
    let s = abc();
    let (m, base) = table.table.get_tuple(rid).unwrap();
    verify(&s, &reconstruct_tuple(&s, &base, &m, &logs).unwrap(), &[int(30), dbl(100.0), bool_null()]);
}

#[test]
fn s4a_03_a_chain_of_only_newer_versions_means_the_tuple_did_not_exist() {
    let db = new_db();
    let table = maintable(&db);
    let owner = begin(&db);
    let reader = begin(&db);
    let rid = row_with_chain(&db, &table, &owner, 9, 1, &[(7, 2), (4, 3)]);
    assert!(collect(&db, &table, rid, &reader).is_none(), "a chain of only newer versions means the tuple did not exist: expected `collect(&db, &table, rid, &reader).is_none()`");
}

#[test]
fn s4a_03_a_log_that_has_been_garbage_collected_ends_the_search() {
    let db = new_db();
    let table = maintable(&db);
    let reader = begin(&db);
    let rid = insert(&table, meta(9, false), &[int(1), dbl(1.0), bool_null()]);
    db.txn_manager.update_undo_link(rid, Some(UndoLink { prev_txn: TXN_START_ID + 999, prev_log_idx: 0 }), None);
    assert!(collect(&db, &table, rid, &reader).is_none(), "a log that has been garbage collected ends the search: expected `collect(&db, &table, rid, &reader).is_none()`");
}

// ---- 4a-04: the undo log for a first change ---------------------------------------------------------------------------------------------

fn tup(s: &Schema, values: &[Value]) -> Tuple {
    Tuple::new(values, s)
}

#[test]
fn s4a_04_changing_some_columns_logs_those_columns_with_their_old_values() {
    let s = abc();
    let base = tup(&s, &[int(1), dbl(2.0), boolean(true)]);
    let target = tup(&s, &[int(1), dbl(5.0), boolean(false)]);
    let log = generate_new_undo_log(&s, Some(&base), Some(&target), 4, UndoLink::default());
    assert!(!log.is_deleted, "changing some columns logs those columns with their old values: expected `!log.is_deleted`");
    assert_eq!(log.modified_fields, vec![false, true, true], "changing some columns logs those columns with their old values");
    verify(&get_undo_log_schema(&s, &log.modified_fields), &log.tuple, &[dbl(2.0), boolean(true)]);
}

#[test]
fn s4a_04_the_log_remembers_the_timestamp_and_the_previous_version() {
    let s = abc();
    let base = tup(&s, &[int(1), dbl(2.0), boolean(true)]);
    let target = tup(&s, &[int(2), dbl(2.0), boolean(true)]);
    let prev = UndoLink { prev_txn: TXN_START_ID + 3, prev_log_idx: 2 };
    let log = generate_new_undo_log(&s, Some(&base), Some(&target), 4, prev);
    assert_eq!((log.ts, log.prev_version), (4, prev), "the log remembers the timestamp and the previous version");
}

#[test]
fn s4a_04_deleting_logs_every_column() {
    let s = abc();
    let base = tup(&s, &[int(1), dbl_null(), boolean(true)]);
    let log = generate_new_undo_log(&s, Some(&base), None, 4, UndoLink::default());
    assert!(!log.is_deleted, "deleting logs every column: expected `!log.is_deleted`");
    assert_eq!(log.modified_fields, vec![true, true, true], "deleting logs every column");
    verify(&s, &log.tuple, &[int(1), dbl_null(), boolean(true)]);
}

#[test]
fn s4a_04_a_tuple_that_did_not_exist_gets_a_deleting_log() {
    let s = abc();
    let target = tup(&s, &[int(1), dbl(2.0), boolean(true)]);
    let log = generate_new_undo_log(&s, None, Some(&target), 4, UndoLink::default());
    assert!(log.is_deleted, "a tuple that did not exist gets a deleting log: expected `log.is_deleted`");
    assert_eq!(log.modified_fields, vec![false, false, false], "a tuple that did not exist gets a deleting log");
    assert_eq!(log.tuple.get_length(), 0, "a tuple that did not exist gets a deleting log");
}

#[test]
fn s4a_04_a_null_that_stays_null_is_not_a_change() {
    let s = abc();
    let base = tup(&s, &[int(1), dbl_null(), boolean(true)]);
    let target = tup(&s, &[int(1), dbl_null(), boolean(true)]);
    let log = generate_new_undo_log(&s, Some(&base), Some(&target), 4, UndoLink::default());
    assert_eq!(log.modified_fields, vec![false, false, false], "a null that stays null is not a change");
}

#[test]
fn s4a_04_going_to_or_from_null_is_a_change() {
    let s = abc();
    let base = tup(&s, &[int_null(), dbl(1.0), bool_null()]);
    let target = tup(&s, &[int(0), dbl(1.0), boolean(false)]);
    let log = generate_new_undo_log(&s, Some(&base), Some(&target), 4, UndoLink::default());
    assert_eq!(log.modified_fields, vec![true, false, true], "going to or from null is a change");
    verify(&get_undo_log_schema(&s, &log.modified_fields), &log.tuple, &[int_null(), bool_null()]);
    let back = generate_new_undo_log(&s, Some(&target), Some(&base), 4, UndoLink::default());
    verify(&get_undo_log_schema(&s, &back.modified_fields), &back.tuple, &[int(0), boolean(false)]);
}

#[test]
fn s4a_04_a_log_reconstructs_the_version_it_was_made_from() {
    let s = abc();
    let base = tup(&s, &[int(1), dbl(2.0), boolean(true)]);
    let target = tup(&s, &[int(9), dbl(2.0), bool_null()]);
    let log = generate_new_undo_log(&s, Some(&base), Some(&target), 4, UndoLink::default());
    let back = reconstruct_tuple(&s, &target, &meta(5, false), &[log]).unwrap();
    verify(&s, &back, &[int(1), dbl(2.0), boolean(true)]);
}

// ---- 4a-04: updating the log of a tuple the transaction changed before -------------------------------------------------------------------

#[test]
fn s4a_04_a_second_change_adds_the_new_columns_and_keeps_the_old_values() {
    let s = abc();
    let v0 = tup(&s, &[int(1), dbl(2.0), boolean(true)]);
    let v1 = tup(&s, &[int(5), dbl(2.0), boolean(true)]); // first change: a
    let v2 = tup(&s, &[int(5), dbl(7.0), boolean(true)]); // second change: b
    let first = generate_new_undo_log(&s, Some(&v0), Some(&v1), 3, UndoLink::default());
    let second = generate_updated_undo_log(&s, Some(&v1), Some(&v2), &first);
    assert_eq!(second.modified_fields, vec![true, true, false], "a second change adds the new columns and keeps the old values");
    verify(&get_undo_log_schema(&s, &second.modified_fields), &second.tuple, &[int(1), dbl(2.0)]);
}

#[test]
fn s4a_04_changing_a_column_again_keeps_its_original_value() {
    let s = abc();
    let v0 = tup(&s, &[int(1), dbl(2.0), boolean(true)]);
    let v1 = tup(&s, &[int(5), dbl(2.0), boolean(true)]);
    let v2 = tup(&s, &[int(6), dbl(2.0), boolean(true)]);
    let first = generate_new_undo_log(&s, Some(&v0), Some(&v1), 3, UndoLink::default());
    let second = generate_updated_undo_log(&s, Some(&v1), Some(&v2), &first);
    assert_eq!(second.modified_fields, vec![true, false, false], "changing a column again keeps its original value");
    verify(&get_undo_log_schema(&s, &second.modified_fields), &second.tuple, &[int(1)]);
}

#[test]
fn s4a_04_the_timestamp_and_previous_version_do_not_change() {
    let s = abc();
    let prev = UndoLink { prev_txn: TXN_START_ID + 1, prev_log_idx: 0 };
    let v0 = tup(&s, &[int(1), dbl(2.0), boolean(true)]);
    let v1 = tup(&s, &[int(5), dbl(2.0), boolean(true)]);
    let v2 = tup(&s, &[int(5), dbl(3.0), boolean(true)]);
    let first = generate_new_undo_log(&s, Some(&v0), Some(&v1), 3, prev);
    let second = generate_updated_undo_log(&s, Some(&v1), Some(&v2), &first);
    assert_eq!((second.ts, second.prev_version), (3, prev), "the timestamp and previous version do not change");
}

#[test]
fn s4a_04_deleting_after_a_partial_change_makes_the_log_cover_every_column() {
    let s = abc();
    let v0 = tup(&s, &[int(1), dbl(2.0), boolean(true)]);
    let v1 = tup(&s, &[int(5), dbl(2.0), boolean(true)]);
    let first = generate_new_undo_log(&s, Some(&v0), Some(&v1), 3, UndoLink::default());
    let second = generate_updated_undo_log(&s, Some(&v1), None, &first);
    assert_eq!(second.modified_fields, vec![true, true, true], "deleting after a partial change makes the log cover every column");
    verify(&s, &second.tuple, &[int(1), dbl(2.0), boolean(true)]);
}

#[test]
fn s4a_04_a_log_that_says_did_not_exist_stays() {
    let s = abc();
    let v1 = tup(&s, &[int(5), dbl(2.0), boolean(true)]);
    let v2 = tup(&s, &[int(6), dbl(2.0), boolean(true)]);
    let first = generate_new_undo_log(&s, None, Some(&v1), 3, UndoLink::default());
    let second = generate_updated_undo_log(&s, Some(&v1), Some(&v2), &first);
    assert!(second.is_deleted, "a log that says did not exist stays: expected `second.is_deleted`");
    assert_eq!(second.modified_fields, vec![false, false, false], "a log that says did not exist stays");
}

#[test]
fn s4a_04_changing_a_tuple_this_transaction_deleted_leaves_the_full_log() {
    let s = abc();
    let v0 = tup(&s, &[int(1), dbl(2.0), boolean(true)]);
    let first = generate_new_undo_log(&s, Some(&v0), None, 3, UndoLink::default());
    let v2 = tup(&s, &[int(8), dbl(8.0), boolean(false)]);
    let second = generate_updated_undo_log(&s, None, Some(&v2), &first);
    assert_eq!(second.modified_fields, vec![true, true, true], "changing a tuple this transaction deleted leaves the full log");
    verify(&s, &second.tuple, &[int(1), dbl(2.0), boolean(true)]);
}

// ---- 4a-05: the sequential scan reads versions --------------------------------------------------------------------------------------------

#[test]
fn s4a_05_a_transaction_sees_tuples_committed_before_it_began() {
    let db = new_db();
    let table = maintable(&db);
    let w = begin(&db);
    let rid = insert(&table, meta(w.temp_ts(), false), &[int(1), dbl(1.0), bool_null()]);
    w.append_write_set(table.oid, rid);
    commit(&db, &w);
    let r = begin(&db);
    assert_eq!(query(&db, &r, "SELECT a FROM maintable"), vec!["1"], "a transaction sees tuples committed before it began");
}

#[test]
fn s4a_05_a_transaction_does_not_see_tuples_committed_after_it_began() {
    let db = new_db();
    let table = maintable(&db);
    let early = begin(&db);
    let w = begin(&db);
    let rid = insert(&table, meta(w.temp_ts(), false), &[int(1), dbl(1.0), bool_null()]);
    w.append_write_set(table.oid, rid);
    commit(&db, &w);
    assert!(query(&db, &early, "SELECT a FROM maintable").is_empty(), "a transaction does not see tuples committed after it began: expected `query(&db, &early, \"SELECT a FROM maintable\").is_empty()`");
    assert_eq!(query(&db, &begin(&db), "SELECT a FROM maintable"), vec!["1"], "a transaction does not see tuples committed after it began");
}

#[test]
fn s4a_05_uncommitted_tuples_are_visible_only_to_their_writer() {
    let db = new_db();
    let table = maintable(&db);
    let w = begin(&db);
    let other = begin(&db);
    insert(&table, meta(w.temp_ts(), false), &[int(1), dbl(1.0), bool_null()]);
    assert_eq!(query(&db, &w, "SELECT a FROM maintable"), vec!["1"], "uncommitted tuples are visible only to their writer");
    assert!(query(&db, &other, "SELECT a FROM maintable").is_empty(), "uncommitted tuples are visible only to their writer: expected `query(&db, &other, \"SELECT a FROM maintable\").is_empty()`");
}

#[test]
fn s4a_05_an_older_version_is_rebuilt_from_the_undo_logs() {
    let db = new_db();
    let table = maintable(&db);
    let owner = begin(&db);
    let t = begin(&db);
    commit(&db, &t); // ts 1
    let old_reader = db.txn_manager.begin(SI).unwrap();
    assert_eq!(old_reader.read_ts(), 1, "an older version is rebuilt from the undo logs");
    let t = begin(&db);
    commit(&db, &t); // ts 2
    let new_reader = begin(&db);
    row_with_chain(&db, &table, &owner, 2, 20, &[(1, 10)]);
    assert_eq!(query(&db, &old_reader, "SELECT a FROM maintable"), vec!["10"], "an older version is rebuilt from the undo logs");
    assert_eq!(query(&db, &new_reader, "SELECT a FROM maintable"), vec!["20"], "an older version is rebuilt from the undo logs");
}

#[test]
fn s4a_05_a_deleted_tuple_is_gone_only_for_those_who_see_the_delete() {
    let db = new_db();
    let table = maintable(&db);
    let owner = begin(&db);
    let t = begin(&db);
    commit(&db, &t); // ts 1
    let before = begin(&db);
    let t = begin(&db);
    commit(&db, &t); // ts 2
    let after = begin(&db);
    let rid = insert(&table, meta(2, true), &[int(7), dbl(7.0), bool_null()]);
    let full = undo_log(false, &[true, true, true], &abc(), &[int(7), dbl(7.0), bool_null()], 1, UndoLink::default());
    let link = owner.append_undo_log(full);
    db.txn_manager.update_undo_link(rid, Some(link), None);
    assert_eq!(query(&db, &before, "SELECT a FROM maintable"), vec!["7"], "a deleted tuple is gone only for those who see the delete");
    assert!(query(&db, &after, "SELECT a FROM maintable").is_empty(), "a deleted tuple is gone only for those who see the delete: expected `query(&db, &after, \"SELECT a FROM maintable\").is_empty()`");
}

#[test]
fn s4a_05_the_filter_sees_the_rebuilt_values() {
    let db = new_db();
    let table = maintable(&db);
    let owner = begin(&db);
    let t = begin(&db);
    commit(&db, &t);
    let reader = begin(&db);
    let t = begin(&db);
    commit(&db, &t);
    row_with_chain(&db, &table, &owner, 2, 20, &[(1, 10)]);
    assert_eq!(query(&db, &reader, "SELECT a FROM maintable WHERE a = 10"), vec!["10"], "the filter sees the rebuilt values");
    assert!(query(&db, &reader, "SELECT a FROM maintable WHERE a = 20").is_empty(), "the filter sees the rebuilt values: expected `query(&db, &reader, \"SELECT a FROM maintable WHERE a = 20\").is_empty()`");
}

#[test]
fn s4a_05_each_tuple_is_judged_on_its_own_chain() {
    let db = new_db();
    let table = maintable(&db);
    let owner = begin(&db);
    let t = begin(&db);
    commit(&db, &t);
    let reader = begin(&db);
    let t = begin(&db);
    commit(&db, &t);
    row_with_chain(&db, &table, &owner, 0, 1, &[]); // old enough
    row_with_chain(&db, &table, &owner, 2, 2, &[(1, 22)]); // rebuilt
    row_with_chain(&db, &table, &owner, 2, 3, &[]); // too new
    assert_eq!(query(&db, &reader, "SELECT a FROM maintable"), vec!["1", "22"], "each tuple is judged on its own chain");
}

// ---- 4a-06: BusTub's tests ----------------------------------------------------------------------------------------------------------------

#[test]
fn s4a_06_timestamp_tracking() {
    let db = new_db();
    let m = &db.txn_manager;
    let txn0 = begin(&db);
    assert_eq!((txn0.read_ts(), m.get_watermark()), (0, 0), "timestamp tracking");
    let store = |expect_read: i64, expect_commit: i64| {
        let t = begin(&db);
        assert_eq!(t.read_ts(), expect_read, "timestamp tracking");
        commit(&db, &t);
        assert_eq!(t.commit_ts(), expect_commit, "timestamp tracking");
    };
    store(0, 1);
    assert_eq!(m.get_watermark(), 0, "timestamp tracking");
    let txn1 = begin(&db);
    assert_eq!(txn1.read_ts(), 1, "timestamp tracking");
    assert_eq!(m.get_watermark(), 0, "timestamp tracking");
    store(1, 2);
    assert_eq!(m.get_watermark(), 0, "timestamp tracking");
    let txn2 = begin(&db);
    assert_eq!(txn2.read_ts(), 2, "timestamp tracking");
    assert_eq!(m.get_watermark(), 0, "timestamp tracking");
    m.abort(&txn0).unwrap();
    assert_eq!(m.get_watermark(), 1, "timestamp tracking");
    store(2, 3);
    assert_eq!(m.get_watermark(), 1, "timestamp tracking");
    let txn3 = begin(&db);
    assert_eq!(txn3.read_ts(), 3, "timestamp tracking");
    assert_eq!(m.get_watermark(), 1, "timestamp tracking");
    m.abort(&txn1).unwrap();
    assert_eq!(m.get_watermark(), 2, "timestamp tracking");
    m.abort(&txn2).unwrap();
    assert_eq!(m.get_watermark(), 3, "timestamp tracking");
    store(3, 4);
    assert_eq!(m.get_watermark(), 3, "timestamp tracking");
    let txn4 = begin(&db);
    assert_eq!(txn4.read_ts(), 4, "timestamp tracking");
    assert_eq!(m.get_watermark(), 3, "timestamp tracking");
    m.abort(&txn3).unwrap();
    assert_eq!(m.get_watermark(), 4, "timestamp tracking");
    m.abort(&txn4).unwrap();
    assert_eq!(m.get_watermark(), 4, "timestamp tracking");
    let t5 = begin(&db);
    assert_eq!((t5.state(), t5.read_ts()), (TransactionState::Running, 4), "timestamp tracking");
    commit(&db, &t5);
    assert_eq!(t5.state(), TransactionState::Committed, "timestamp tracking");
    assert_eq!(m.get_watermark(), 5, "timestamp tracking");
    let txn5 = begin(&db);
    assert_eq!((txn5.state(), txn5.read_ts(), m.get_watermark()), (TransactionState::Running, 5, 5), "timestamp tracking");
    m.abort(&txn5).unwrap();
    assert_eq!(txn5.state(), TransactionState::Aborted, "timestamp tracking");
    assert_eq!(m.get_watermark(), 5, "timestamp tracking");
}

#[test]
fn s4a_06_tuple_reconstruct() {
    // BusTub's TxnScanTest.TupleReconstructTest, in the pieces of the earlier stages: here the combined chains of its cases C and D
    let s = abc();
    let base = Tuple::new(&[int(0), dbl(1.0), bool_null()], &s);
    let m = meta(2333, false);
    let l1 = undo_log(false, &[false, false, false], &schema_of(&[]), &[], 0, UndoLink::default());
    let l2 = undo_log(false, &[false, true, false], &schema_of(&[("b", TypeId::Decimal)]), &[dbl(2.0)], 0, UndoLink::default());
    let l3 = undo_log(false, &[true, false, true], &schema_of(&[("a", TypeId::Integer), ("c", TypeId::Boolean)]), &[int(3), boolean(false)], 0, UndoLink::default());
    let l4 = undo_log(false, &[true, true, true], &s, &[int(4), dbl(4.0), boolean(true)], 0, UndoLink::default());
    verify(&s, &reconstruct_tuple(&s, &base, &m, &[l1.clone()]).unwrap(), &[int(0), dbl(1.0), bool_null()]);
    verify(&s, &reconstruct_tuple(&s, &base, &m, &[l1.clone(), l2.clone()]).unwrap(), &[int(0), dbl(2.0), bool_null()]);
    verify(&s, &reconstruct_tuple(&s, &base, &m, &[l1.clone(), l2.clone(), l3.clone()]).unwrap(), &[int(3), dbl(2.0), boolean(false)]);
    verify(&s, &reconstruct_tuple(&s, &base, &m, &[l1, l2, l3, l4]).unwrap(), &[int(4), dbl(4.0), boolean(true)]);
    let del = undo_log(true, &[false, false, false], &schema_of(&[]), &[], 0, UndoLink::default());
    let one = undo_log(false, &[true, true, true], &s, &[int(1), dbl(1.0), boolean(false)], 0, UndoLink::default());
    let nulls = undo_log(false, &[true, true, true], &s, &[int_null(), dbl_null(), bool_null()], 0, UndoLink::default());
    assert!(reconstruct_tuple(&s, &base, &m, &[del.clone()]).is_none(), "tuple reconstruct: expected `reconstruct_tuple(&s, &base, &m, &[del.clone()]).is_none()`");
    verify(&s, &reconstruct_tuple(&s, &base, &m, &[del.clone(), one.clone()]).unwrap(), &[int(1), dbl(1.0), boolean(false)]);
    assert!(reconstruct_tuple(&s, &base, &m, &[del.clone(), one.clone(), del.clone()]).is_none(), "tuple reconstruct: expected `reconstruct_tuple(&s, &base, &m, &[del.clone(), one.clone(), del.clone()]).is_none()`");
    verify(&s, &reconstruct_tuple(&s, &base, &m, &[del.clone(), one, del, nulls]).unwrap(), &[int_null(), dbl_null(), bool_null()]);
}

#[test]
fn s4a_06_collect_undo_log_test() {
    // BusTub's TxnScanTest.CollectUndoLogTest
    let db = new_db();
    let s = abc();
    let modify = schema_of(&[("a", TypeId::Integer), ("b", TypeId::Decimal)]);
    let table = maintable(&db);
    let m = &db.txn_manager;
    let txn0 = begin(&db);
    assert_eq!(txn0.read_ts(), 0, "collect undo log test");
    commit(&db, &txn0);
    let rid0 = insert(&table, meta(txn0.commit_ts(), false), &[int(1), dbl(1.0), bool_null()]);
    let txn1 = begin(&db);
    assert_eq!(txn1.read_ts(), 1, "collect undo log test");
    commit(&db, &txn1);
    let txn_to_inspect = begin(&db);
    assert_eq!(txn_to_inspect.read_ts(), 2, "collect undo log test");
    let rid1 = insert(&table, meta(txn1.commit_ts(), false), &[int(2), dbl(2.0), bool_null()]);
    let link1 = txn1.append_undo_log(undo_log(false, &[true, true, false], &modify, &[int(1), dbl(1.0)], txn0.commit_ts(), UndoLink::default()));
    m.update_undo_link(rid1, Some(link1), None);
    let txn2 = begin(&db);
    assert_eq!(txn2.read_ts(), 2, "collect undo log test");
    commit(&db, &txn2);
    let rid2 = insert(&table, meta(txn2.commit_ts(), false), &[int(3), dbl(3.0), bool_null()]);
    let link2 = txn1.append_undo_log(undo_log(false, &[true, true, false], &modify, &[int(1), dbl(1.0)], txn0.commit_ts(), UndoLink::default()));
    let link3 = txn2.append_undo_log(undo_log(false, &[true, true, false], &modify, &[int(2), dbl(2.0)], txn1.commit_ts(), link2));
    m.update_undo_link(rid2, Some(link3), None);
    let rid3 = insert(&table, meta(txn2.commit_ts(), false), &[int(3), dbl(3.0), bool_null()]);
    let txn3 = begin(&db);
    assert_eq!(txn3.read_ts(), 3, "collect undo log test");
    commit(&db, &txn3);
    let rid4 = insert(&table, meta(txn3.commit_ts(), false), &[int(4), dbl(4.0), bool_null()]);
    let link4 = txn3.append_undo_log(undo_log(false, &[true, true, false], &modify, &[int(3), dbl(3.0)], txn2.commit_ts(), UndoLink::default()));
    m.update_undo_link(rid4, Some(link4), None);
    let rid5 = insert(&table, meta(txn3.commit_ts(), true), &[int(2), dbl(2.0), bool_null()]);
    let link5 = txn3.append_undo_log(undo_log(false, &[true, true, false], &modify, &[int(2), dbl(2.0)], txn1.commit_ts(), UndoLink::default()));
    m.update_undo_link(rid5, Some(link5), None);
    let rid6 = insert(&table, meta(txn3.commit_ts(), false), &[int(4), dbl(4.0), bool_null()]);
    let link6 = txn1.append_undo_log(undo_log(false, &[true, true, true], &s, &[int(1), dbl(1.0), bool_null()], txn0.commit_ts(), UndoLink::default()));
    let del = UndoLog { is_deleted: true, modified_fields: vec![false, false, false], tuple: Tuple::empty(), ts: txn1.commit_ts(), prev_version: link6 };
    let link7 = txn3.append_undo_log(del);
    m.update_undo_link(rid6, Some(link7), None);
    let rid7 = insert(&table, meta(txn_to_inspect.temp_ts(), false), &[int(100), dbl(100.0), bool_null()]);
    let rid8 = insert(&table, meta(txn_to_inspect.temp_ts(), false), &[int(100), dbl(100.0), bool_null()]);
    let link8 = txn_to_inspect.append_undo_log(undo_log(false, &[true, true, false], &modify, &[int(1), dbl(1.0)], txn0.commit_ts(), UndoLink::default()));
    m.update_undo_link(rid8, Some(link8), None);
    let txn4 = begin(&db);
    assert_eq!(txn4.read_ts(), 4, "collect undo log test");
    let rid9 = insert(&table, meta(txn4.temp_ts(), false), &[int(400), dbl(400.0), bool_null()]);
    let link9 = txn1.append_undo_log(undo_log(false, &[true, true, false], &modify, &[int(1), dbl(1.0)], txn0.commit_ts(), UndoLink::default()));
    let link10 = txn4.append_undo_log(undo_log(false, &[true, true, false], &modify, &[int(4), dbl(4.0)], txn3.commit_ts(), link9));
    m.update_undo_link(rid9, Some(link10), None);

    let rebuilt = |rid: Rid| -> Option<Option<Tuple>> {
        let (bm, bt) = table.table.get_tuple(rid).unwrap();
        let logs = collect_undo_logs(rid, &bm, &bt, m.get_undo_link(rid), &txn_to_inspect, m)?;
        Some(reconstruct_tuple(&s, &bt, &bm, &logs))
    };
    verify(&s, &rebuilt(rid0).unwrap().unwrap(), &[int(1), dbl(1.0), bool_null()]);
    verify(&s, &rebuilt(rid1).unwrap().unwrap(), &[int(2), dbl(2.0), bool_null()]);
    verify(&s, &rebuilt(rid2).unwrap().unwrap(), &[int(2), dbl(2.0), bool_null()]);
    assert!(rebuilt(rid3).is_none(), "collect undo log test: expected `rebuilt(rid3).is_none()`");
    assert!(rebuilt(rid4).is_none(), "collect undo log test: expected `rebuilt(rid4).is_none()`");
    verify(&s, &rebuilt(rid5).unwrap().unwrap(), &[int(2), dbl(2.0), bool_null()]);
    assert!(rebuilt(rid6).unwrap().is_none(), "the version at ts 2 is a deleted tuple");
    verify(&s, &rebuilt(rid7).unwrap().unwrap(), &[int(100), dbl(100.0), bool_null()]);
    verify(&s, &rebuilt(rid8).unwrap().unwrap(), &[int(100), dbl(100.0), bool_null()]);
    verify(&s, &rebuilt(rid9).unwrap().unwrap(), &[int(1), dbl(1.0), bool_null()]);
}

#[test]
fn s4a_06_scan_test() {
    // BusTub's TxnScanTest.ScanTest. record1: txn4 (val=1) -> ts=1 in txn4 (val=2); record2: ts=3 (val=3) -> ts=2 in txn_store_3 (delete)
    // -> ts=1 in txn_store_2 (val=4); record3: ts=4 (delete) -> ts=3 in txn_store_4 (val=5); record4: txn3 (delete) -> ts=2 in txn3
    // (val=6) -> ts=1 in txn_store_2 (val=7)
    let db = new_db();
    let s = abc();
    let only_a = schema_of(&[("a", TypeId::Integer)]);
    let table = maintable(&db);
    let m = &db.txn_manager;
    let txn0 = begin(&db);
    let t = begin(&db);
    commit(&db, &t);
    let txn1 = begin(&db);
    assert_eq!(txn1.read_ts(), 1, "scan test");
    let store2 = begin(&db);
    let prev_log_3 = store2.append_undo_log(undo_log(false, &[true, true, true], &s, &[int(4), dbl(4.0), boolean(true)], 1, UndoLink::default()));
    let prev_log_6 = store2.append_undo_log(undo_log(false, &[true, false, false], &only_a, &[int(7)], 1, UndoLink::default()));
    commit(&db, &store2);
    let txn2 = begin(&db);
    assert_eq!(txn2.read_ts(), 2, "scan test");
    let store3 = begin(&db);
    let prev_log_2 = store3.append_undo_log(UndoLog { is_deleted: true, modified_fields: vec![false; 3], tuple: Tuple::empty(), ts: 2, prev_version: prev_log_3 });
    commit(&db, &store3);
    let txn3 = begin(&db);
    assert_eq!(txn3.read_ts(), 3, "scan test");
    let prev_log_5 = txn3.append_undo_log(undo_log(false, &[true, true, true], &s, &[int(6), dbl_null(), bool_null()], 2, prev_log_6));
    let store4 = begin(&db);
    let prev_log_4 = store4.append_undo_log(undo_log(false, &[true, true, true], &s, &[int(5), dbl(3.0), boolean(false)], 3, UndoLink::default()));
    commit(&db, &store4);
    let txn4 = begin(&db);
    assert_eq!(txn4.read_ts(), 4, "scan test");
    let store5 = begin(&db);
    commit(&db, &store5);
    let txn5 = begin(&db);
    assert_eq!(txn5.read_ts(), 5, "scan test");
    let prev_log_1 = txn4.append_undo_log(undo_log(false, &[true, false, false], &only_a, &[int(2)], 1, UndoLink::default()));
    let rid1 = insert(&table, meta(txn4.temp_ts(), false), &[int(1), dbl_null(), bool_null()]);
    m.update_undo_link(rid1, Some(prev_log_1), None);
    let rid2 = insert(&table, meta(txn3.read_ts(), false), &[int(3), dbl_null(), bool_null()]);
    m.update_undo_link(rid2, Some(prev_log_2), None);
    let rid3 = insert(&table, meta(txn4.read_ts(), true), &[int_null(), dbl_null(), bool_null()]);
    m.update_undo_link(rid3, Some(prev_log_4), None);
    let rid4 = insert(&table, meta(txn3.temp_ts(), true), &[int_null(), dbl_null(), bool_null()]);
    m.update_undo_link(rid4, Some(prev_log_5), None);

    txn_mgr_dbg("before verify scan", &db.txn_manager, &table);
    assert!(query(&db, &txn0, "SELECT * FROM maintable").is_empty(), "scan test: expected `query(&db, &txn0, \"SELECT * FROM maintable\").is_empty()`");
    let a_of = |txn: &Arc<Transaction>| query(&db, txn, "SELECT a FROM maintable");
    assert_eq!(a_of(&txn1), vec!["2", "4", "7"], "scan test");
    // txn2 (read ts 2): record1 is txn4's uncommitted tuple, its log at ts 1 gives 2; record2 (ts 3) -> log ts 2 is a delete;
    // record3 (ts 4 deleted) -> log ts 3 > 2, nothing older; record4 (txn3's) -> its log at ts 2 gives 6
    assert_eq!(a_of(&txn2), vec!["2", "6"], "scan test");
    // txn3 (read ts 3): record1 as before -> 2; record2 is at ts 3: 3; record3: deleted at 4 -> log ts 3 gives 5; record4 is txn3's delete
    assert_eq!(a_of(&txn3), vec!["2", "3", "5"], "scan test");
    // txn4: record1 is its own: 1; record2: 3; record3 is deleted at ts 4: gone; record4: txn3's delete is uncommitted, log ts 2 gives 6
    assert_eq!(a_of(&txn4), vec!["1", "3", "6"], "scan test");
    // txn5: record1 uncommitted by txn4 -> 2; record2: 3; record3: deleted; record4: 6
    assert_eq!(a_of(&txn5), vec!["2", "3", "6"], "scan test");
}

#[test]
fn s4a_06_generate_undo_log_test() {
    // BusTub's TxnExecutorTest.GenerateUndoLogTest: the logs the executors of module 4b will make, applied back with reconstruct_tuple
    let s = abc();
    let none = UndoLink::default();
    let t = |a: i32, b: f64, c: bool| Tuple::new(&[int(a), dbl(b), boolean(c)], &s);
    let same = |x: &Tuple, y: &Tuple| x.data() == y.data();
    let live = meta(0, false);
    // simple update
    let (base, target) = (t(0, 0.0, true), t(0, 1.0, false));
    let log = generate_new_undo_log(&s, Some(&base), Some(&target), 0, none);
    assert!(same(&reconstruct_tuple(&s, &target, &live, &[log]).unwrap(), &base), "generate undo log test: expected `same(&reconstruct_tuple(&s, &target, &live, &[log]).unwrap(), &base)`");
    // simple delete
    let log = generate_new_undo_log(&s, Some(&base), None, 0, none);
    assert!(same(&reconstruct_tuple(&s, &base, &meta(0, true), &[log]).unwrap(), &base), "generate undo log test: expected `same(&reconstruct_tuple(&s, &base, &meta(0, true), &[log]).unwrap(), &base)`");
    // simple insert (an insert over a tombstone)
    let target = t(0, 1.0, false);
    let log = generate_new_undo_log(&s, None, Some(&target), 0, none);
    assert!(reconstruct_tuple(&s, &target, &live, &[log]).is_none(), "generate undo log test: expected `reconstruct_tuple(&s, &target, &live, &[log]).is_none()`");
    // update twice in a txn
    let (base, mid, target) = (t(0, 0.0, true), t(0, 0.0, false), t(0, 1.0, false));
    let log = generate_new_undo_log(&s, Some(&base), Some(&mid), 0, none);
    let log = generate_updated_undo_log(&s, Some(&mid), Some(&target), &log);
    assert!(same(&reconstruct_tuple(&s, &target, &live, &[log]).unwrap(), &base), "generate undo log test: expected `same(&reconstruct_tuple(&s, &target, &live, &[log]).unwrap(), &base)`");
    // update then delete in a txn
    let (base, target) = (t(0, 0.0, true), t(0, 1.0, false));
    let log = generate_new_undo_log(&s, Some(&base), Some(&target), 0, none);
    let log = generate_updated_undo_log(&s, Some(&target), None, &log);
    assert!(same(&reconstruct_tuple(&s, &target, &meta(0, true), &[log]).unwrap(), &base), "generate undo log test: expected `same(&reconstruct_tuple(&s, &target, &meta(0, true), &[log]).unwrap(), &base)`");
    // insert then update in a txn
    let (mid, target) = (t(0, 0.0, false), t(0, 1.0, false));
    let log = generate_new_undo_log(&s, None, Some(&mid), 0, none);
    let log = generate_updated_undo_log(&s, Some(&mid), Some(&target), &log);
    assert!(reconstruct_tuple(&s, &target, &live, &[log]).is_none(), "generate undo log test: expected `reconstruct_tuple(&s, &target, &live, &[log]).is_none()`");
    // insert then delete in a txn
    let log = generate_new_undo_log(&s, None, Some(&mid), 0, none);
    let log = generate_updated_undo_log(&s, Some(&mid), None, &log);
    assert!(reconstruct_tuple(&s, &mid, &meta(0, true), &[log]).is_none(), "generate undo log test: expected `reconstruct_tuple(&s, &mid, &meta(0, true), &[log]).is_none()`");
    // delete then insert in a txn
    let (base, target) = (t(0, 0.0, true), t(0, 1.0, false));
    let log = generate_new_undo_log(&s, Some(&base), None, 0, none);
    let log = generate_updated_undo_log(&s, None, Some(&target), &log);
    assert!(same(&reconstruct_tuple(&s, &target, &live, &[log]).unwrap(), &base), "generate undo log test: expected `same(&reconstruct_tuple(&s, &target, &live, &[log]).unwrap(), &base)`");
}

// ---- properties: timestamps, chains and snapshots against a model of versions ------------------------------------------------------

fn pconfig() -> ProptestConfig {
    ProptestConfig { cases: 40, max_shrink_iters: 1000, failure_persistence: None, ..ProptestConfig::default() }
}

/// What a watermark operation does to the model: a multiset of live read timestamps and the last commit timestamp.
#[derive(Clone, Debug)]
enum WmOp {
    /// A reader at the last commit timestamp plus this much (an older one would be refused).
    Add(i64),
    /// Removes the n-th live reader, wrapped around.
    Remove(usize),
    /// A commit: the last commit timestamp goes up by one.
    Commit,
}

fn wm_op() -> impl Strategy<Value = WmOp> {
    prop_oneof![4 => (0i64..4).prop_map(WmOp::Add), 3 => any::<usize>().prop_map(WmOp::Remove), 3 => Just(WmOp::Commit)]
}

proptest! {
    #![proptest_config(pconfig())]

    /// After any sequence of readers arriving, leaving and commits, the watermark is the smallest live read timestamp, or the last
    /// commit timestamp when nobody reads.
    #[test]
    fn s4a_01_the_watermark_is_the_minimum_of_the_live_readers(ops in prop::collection::vec(wm_op(), 0..200)) {
        let mut w = Watermark::new(0);
        let (mut live, mut last): (Vec<i64>, i64) = (vec![], 0);
        for op in ops {
            match op {
                WmOp::Add(ahead) => { w.add_txn(last + ahead).unwrap(); live.push(last + ahead); }
                WmOp::Remove(n) => { if !live.is_empty() { let ts = live.remove(n % live.len()); w.remove_txn(ts); } }
                WmOp::Commit => { last += 1; w.update_commit_ts(last); }
            }
            prop_assert_eq!(w.get_watermark(), live.iter().copied().min().unwrap_or(last), "after {:?}", op);
        }
    }

    /// Transactions begun, committed and aborted in any order: every id is new and counts up from 2^62, a transaction reads at the last
    /// commit timestamp of the moment it began, commit timestamps count 1, 2, 3 in commit order, aborts take none, and the manager's
    /// watermark is the smallest read timestamp of the running transactions.
    #[test]
    fn s4a_02_begin_commit_and_abort_keep_ids_timestamps_and_the_watermark_straight(ops in prop::collection::vec((0u8..3, any::<usize>()), 0..60)) {
        let db = new_db();
        let (mut running, mut last_commit): (Vec<Arc<Transaction>>, i64) = (vec![], 0);
        let mut ids = vec![];
        for (op, pick) in ops {
            match op {
                0 => {
                    let t = begin(&db);
                    prop_assert_eq!(t.read_ts(), last_commit, "a transaction reads at the last commit timestamp");
                    prop_assert!(t.id() >= TXN_START_ID, "ids start at 2^62");
                    prop_assert!(!ids.contains(&t.id()), "every id is new");
                    prop_assert_eq!(t.state(), TransactionState::Running);
                    ids.push(t.id());
                    running.push(t);
                }
                1 if !running.is_empty() => {
                    let t = running.remove(pick % running.len());
                    commit(&db, &t);
                    last_commit += 1;
                    prop_assert_eq!(t.commit_ts(), last_commit, "commit timestamps count up in commit order");
                    prop_assert_eq!(t.state(), TransactionState::Committed);
                }
                2 if !running.is_empty() => {
                    let t = running.remove(pick % running.len());
                    db.txn_manager.abort(&t).unwrap();
                    prop_assert_eq!(t.state(), TransactionState::Aborted);
                }
                _ => {}
            }
            prop_assert_eq!(db.txn_manager.last_commit_ts(), last_commit);
            prop_assert_eq!(db.txn_manager.get_watermark(), running.iter().map(|t| t.read_ts()).min().unwrap_or(last_commit), "the watermark");
        }
    }
}

type Cells = [Option<i32>; 2];
/// A version of a row: its cells, or `None` for "deleted" (or not yet there).
type Version = Option<Cells>;

fn two() -> Schema {
    schema_of(&[("a", TypeId::Integer), ("c", TypeId::Integer)])
}

fn cell_value(c: Option<i32>) -> Value {
    c.map_or_else(int_null, int)
}

fn tuple_of(s: &Schema, v: &Cells) -> Tuple {
    Tuple::new(&[cell_value(v[0]), cell_value(v[1])], s)
}

fn cells_of(s: &Schema, t: &Tuple) -> Cells {
    [0, 1].map(|i| t.get_value(s, i).as_i64().map(|x| x as i32))
}

fn cells_text(v: &Version) -> Option<String> {
    v.map(|c| c.iter().map(|x| x.map_or("integer_null".to_string(), |v| v.to_string())).collect::<Vec<_>>().join(" "))
}

fn version_strategy() -> impl Strategy<Value = Version> {
    let cell = || prop_oneof![1 => Just(None), 4 => (0i32..3).prop_map(Some)];
    prop_oneof![1 => Just(None), 5 => (cell(), cell()).prop_map(|(a, c)| Some([a, c]))]
}

/// A row with a history: committed versions (timestamp, version) oldest first with increasing timestamps (the first one exists),
/// then optionally one uncommitted version written by `writer`. The newest version is in the table, the older ones in undo logs owned by
/// `writer` that restore exactly the columns that differ, chained newest first.
fn build_row(db: &BusTubInstance, table: &TableInfo<'_>, writer: &Arc<Transaction>, history: &[(i64, Version)], uncommitted: Option<Version>) -> Rid {
    let s = &table.schema;
    let mut all: Vec<(i64, Version)> = history.to_vec();
    if let Some(v) = uncommitted {
        all.push((writer.temp_ts(), v));
    }
    let (ts, newest) = all.last().cloned().unwrap();
    let cells = newest.unwrap_or([None, None]);
    let rid = table.table.insert_tuple(&meta(ts, newest.is_none()), &tuple_of(s, &cells)).unwrap();
    let mut link = UndoLink::default();
    for i in 0..all.len() - 1 {
        let ((older_ts, older), (_, newer)) = (all[i], all[i + 1]);
        let log = match older {
            None => UndoLog { is_deleted: true, modified_fields: vec![false, false], tuple: Tuple::empty(), ts: older_ts, prev_version: link },
            Some(o) => {
                let modified: Vec<bool> = (0..2).map(|c| newer.is_none_or(|n| n[c] != o[c])).collect();
                let values: Vec<Value> = (0..2).filter(|c| modified[*c]).map(|c| cell_value(o[c])).collect();
                UndoLog { is_deleted: false, modified_fields: modified.clone(), tuple: Tuple::new(&values, &get_undo_log_schema(s, &modified)), ts: older_ts, prev_version: link }
            }
        };
        link = writer.append_undo_log(log);
    }
    if link.is_valid() {
        db.txn_manager.update_undo_link(rid, Some(link), None);
    }
    rid
}

/// Increasing timestamps for a history: `gaps` are the steps between consecutive versions.
fn history_strategy() -> impl Strategy<Value = Vec<(i64, Version)>> {
    (prop::collection::vec((1i64..3, version_strategy()), 1..6), (1i64..3, version_strategy().prop_filter("a row starts to exist", |v| v.is_some()))).prop_map(|(rest, (g0, first))| {
        let mut ts = g0;
        let mut out = vec![(ts, first)];
        for (gap, v) in rest {
            ts += gap;
            out.push((ts, v));
        }
        out
    })
}

/// What a reader at `read_ts` sees of a history, if it did not write anything itself.
fn visible(history: &[(i64, Version)], read_ts: i64) -> Version {
    history.iter().rev().find(|(ts, _)| *ts <= read_ts).and_then(|(_, v)| *v)
}

/// A transaction for every read timestamp 0..=top (committing empty transactions to move the clock).
fn readers_up_to(db: &BusTubInstance, top: i64) -> Vec<Arc<Transaction>> {
    let mut out = vec![];
    for r in 0..=top {
        while db.txn_manager.last_commit_ts() < r {
            let t = begin(db);
            commit(db, &t);
        }
        out.push(begin(db));
    }
    out
}

proptest! {
    #![proptest_config(pconfig())]

    /// For a row with a random history (updates, deletes, re-inserts, columns going to and from NULL), collecting the undo logs and
    /// reconstructing the tuple gives, for a reader at every timestamp, the newest version at or before it; a reader before the first
    /// version sees nothing; and a writer sees its own uncommitted version while everybody else sees the history.
    #[test]
    fn s4a_03_a_reader_sees_exactly_the_committed_prefix_of_a_row_at_its_timestamp(history in history_strategy(), uncommitted in prop::option::of(version_strategy())) {
        let db = new_db();
        let table = db.catalog.write().unwrap().create_table("p", &two()).unwrap();
        let writer = begin(&db);
        let top = history.last().unwrap().0 + 1;
        let readers = readers_up_to(&db, top);
        let rid = build_row(&db, &table, &writer, &history, uncommitted);
        let s = &table.schema;
        let see = |txn: &Transaction| {
            let (m, base) = table.table.get_tuple(rid).unwrap();
            let logs = collect_undo_logs(rid, &m, &base, db.txn_manager.get_undo_link(rid), txn, &db.txn_manager)?;
            reconstruct_tuple(s, &base, &m, &logs).map(|t| cells_of(s, &t))
        };
        for reader in &readers {
            prop_assert_eq!(see(reader), visible(&history, reader.read_ts()), "reader at {}", reader.read_ts());
        }
        if let Some(own) = uncommitted {
            prop_assert_eq!(see(&writer), own, "the writer sees its own version");
        }
    }

    /// The log of a change restores the version it was made from, whichever columns changed, whether or not the old version existed
    /// or the new one does; and a chain of changes by one transaction, folded into the first log, still restores the original.
    #[test]
    fn s4a_04_a_log_restores_the_version_it_was_made_from_and_successive_changes_keep_the_original(versions in prop::collection::vec(version_strategy(), 2..6)) {
        let s = two();
        let as_tuple = |v: &Version| v.map(|c| tuple_of(&s, &c));
        let base_meta = |v: &Version| meta(9, v.is_none());
        let restore = |current: &Version, log: &UndoLog| {
            let base = as_tuple(current).unwrap_or_else(|| tuple_of(&s, &[None, None]));
            reconstruct_tuple(&s, &base, &base_meta(current), std::slice::from_ref(log)).map(|t| cells_of(&s, &t))
        };
        // one change
        let first = generate_new_undo_log(&s, as_tuple(&versions[0]).as_ref(), as_tuple(&versions[1]).as_ref(), 3, UndoLink::default());
        prop_assert_eq!(restore(&versions[1], &first), versions[0], "one change");
        prop_assert_eq!(first.ts, 3);
        // the same transaction goes on changing the row
        let mut log = first;
        for pair in versions.windows(2).skip(1) {
            log = generate_updated_undo_log(&s, as_tuple(&pair[0]).as_ref(), as_tuple(&pair[1]).as_ref(), &log);
            prop_assert_eq!((log.ts, log.prev_version), (3, UndoLink::default()), "the version the log restores does not change");
        }
        prop_assert_eq!(restore(versions.last().unwrap(), &log), versions[0], "after {} changes", versions.len() - 1);
    }

    /// A snapshot scan: several rows, each with its own random history and sometimes an uncommitted version, and a reader at every
    /// timestamp scans them with SQL; it gets exactly the rows that exist in its snapshot (and, for the writer, its own changes).
    #[test]
    fn s4a_05_a_scan_returns_the_rows_that_exist_in_the_snapshot(rows in prop::collection::vec((history_strategy(), prop::option::of(version_strategy())), 1..6)) {
        let db = new_db();
        let table = db.catalog.write().unwrap().create_table("p", &two()).unwrap();
        let writer = begin(&db);
        let top = rows.iter().map(|(h, _)| h.last().unwrap().0).max().unwrap() + 1;
        let readers = readers_up_to(&db, top);
        for (history, uncommitted) in &rows {
            build_row(&db, &table, &writer, history, *uncommitted);
        }
        for reader in &readers {
            let mut want: Vec<String> = rows.iter().filter_map(|(h, _)| cells_text(&visible(h, reader.read_ts()))).collect();
            want.sort();
            prop_assert_eq!(query(&db, reader, "SELECT a, c FROM p"), want, "reader at {}", reader.read_ts());
        }
        let mut own: Vec<String> = rows.iter().filter_map(|(h, u)| cells_text(&match u { Some(v) => *v, None => visible(h, writer.read_ts()) })).collect();
        own.sort();
        prop_assert_eq!(query(&db, &writer, "SELECT a, c FROM p"), own, "the writer sees its own changes");
    }
}

// ---- 4a-06 · boss: a hand-run MVCC session against a model -------------------------------------------------------------------------

#[derive(Clone, Debug)]
enum Step {
    Begin,
    /// Transaction `t` (wrapped around the live ones) inserts a row.
    Insert(usize, [Option<i32>; 2]),
    /// Transaction `t` writes the row `r` (wrapped around the existing ones): new cells, or a delete.
    Write(usize, usize, Version),
    Commit(usize),
}

fn step_strategy() -> impl Strategy<Value = Step> {
    let cell = || prop_oneof![1 => Just(None), 4 => (0i32..3).prop_map(Some)];
    prop_oneof![
        2 => Just(Step::Begin),
        2 => (any::<usize>(), cell(), cell()).prop_map(|(t, a, c)| Step::Insert(t, [a, c])),
        5 => (any::<usize>(), any::<usize>(), version_strategy()).prop_map(|(t, r, v)| Step::Write(t, r, v)),
        2 => any::<usize>().prop_map(Step::Commit),
    ]
}

/// The model of a row: committed versions and the uncommitted one (owner id, version).
#[derive(Clone, Debug, Default)]
struct RowModel {
    committed: Vec<(i64, Version)>,
    pending: Option<(usize, Version)>,
}

/// What `txn` (number `ti`) must see: for every row its own pending version, else the committed one at its read timestamp.
fn expected_rows(model: &[RowModel], ti: usize, txn: &Transaction) -> Vec<String> {
    let mut want: Vec<String> = model.iter().filter_map(|row| {
        let mine = row.pending.filter(|(owner, _)| *owner == ti).map(|(_, v)| v);
        cells_text(&mine.unwrap_or_else(|| visible(&row.committed, txn.read_ts())))
    }).collect();
    want.sort();
    want
}

proptest! {
    #![proptest_config(ProptestConfig { cases: 40, max_shrink_iters: 1500, failure_persistence: None, ..ProptestConfig::default() })]

    /// A random session, run by hand the way module 4b's executors will run it: transactions begin, insert rows, write rows (the first
    /// write adds a new undo log with `generate_new_undo_log`, later ones fold into it with `generate_updated_undo_log`) and commit. At
    /// the end, every transaction (committed or not) scans the table with SQL and sees exactly the committed state at its read
    /// timestamp plus its own pending writes (a transaction that committed reads at its begin timestamp and no longer sees its own writes).
    #[test]
    fn s4a_06_a_session_of_hand_run_transactions_is_seen_exactly_as_the_model_says(steps in prop::collection::vec(step_strategy(), 1..40)) {
        let db = new_db();
        let table = db.catalog.write().unwrap().create_table("p", &two()).unwrap();
        let s = table.schema.clone();
        let mut txns: Vec<Arc<Transaction>> = vec![];
        let mut done: Vec<bool> = vec![];
        let mut model: Vec<RowModel> = vec![];
        let mut rids: Vec<Rid> = vec![];
        for step in &steps {
            let live: Vec<usize> = (0..txns.len()).filter(|i| !done[*i]).collect();
            match step {
                Step::Begin => { txns.push(begin(&db)); done.push(false); }
                Step::Insert(t, cells) if !live.is_empty() => {
                    let ti = live[t % live.len()];
                    let txn = &txns[ti];
                    let rid = table.table.insert_tuple(&meta(txn.temp_ts(), false), &tuple_of(&s, cells)).unwrap();
                    txn.append_write_set(table.oid, rid);
                    rids.push(rid);
                    model.push(RowModel { committed: vec![], pending: Some((ti, Some(*cells))) });
                }
                Step::Write(t, r, version) if !live.is_empty() && !rids.is_empty() => {
                    let ti = live[t % live.len()];
                    let ri = r % rids.len();
                    let txn = &txns[ti];
                    let (rid, row) = (rids[ri], &mut model[ri]);
                    match row.pending {
                        // somebody else is writing it: the engine would refuse; the session skips
                        Some((owner, _)) if owner != ti => continue,
                        Some(_) => {
                            // a second change by the same transaction: fold into the existing log, if the row started before it
                            let (m, base) = table.table.get_tuple(rid).unwrap();
                            let base_tuple = if m.is_deleted { None } else { Some(base.clone()) };
                            let target = version.map(|c| tuple_of(&s, &c));
                            let link = db.txn_manager.get_undo_link(rid);
                            if let Some(l) = link.filter(|l| l.is_valid() && l.prev_txn == txn.id()) {
                                let old = txn.get_undo_log(l.prev_log_idx as usize);
                                txn.modify_undo_log(l.prev_log_idx as usize, generate_updated_undo_log(&s, base_tuple.as_ref(), target.as_ref(), &old));
                            }
                            let cells = version.unwrap_or([None, None]);
                            bustub::concurrency::transaction_manager::update_tuple_and_undo_link(&db.txn_manager, &table, rid, link, &meta(txn.temp_ts(), version.is_none()), &tuple_of(&s, &cells), None).unwrap();
                            row.pending = Some((ti, *version));
                        }
                        None => {
                            // the first change of a committed row: the old version goes into a new log
                            let (m, base) = table.table.get_tuple(rid).unwrap();
                            let base_tuple = if m.is_deleted { None } else { Some(base.clone()) };
                            let target = version.map(|c| tuple_of(&s, &c));
                            let prev = db.txn_manager.get_undo_link(rid).unwrap_or_default();
                            let log = generate_new_undo_log(&s, base_tuple.as_ref(), target.as_ref(), m.ts, prev);
                            let link = txn.append_undo_log(log);
                            let cells = version.unwrap_or([None, None]);
                            bustub::concurrency::transaction_manager::update_tuple_and_undo_link(&db.txn_manager, &table, rid, Some(link), &meta(txn.temp_ts(), version.is_none()), &tuple_of(&s, &cells), None).unwrap();
                            txn.append_write_set(table.oid, rid);
                            row.pending = Some((ti, *version));
                        }
                    }
                }
                Step::Commit(t) if !live.is_empty() => {
                    let ti = live[t % live.len()];
                    prop_assert_eq!(query(&db, &txns[ti], "SELECT a, c FROM p"), expected_rows(&model, ti, &txns[ti]), "transaction {} just before it commits", ti);
                    commit(&db, &txns[ti]);
                    done[ti] = true;
                    let ts = txns[ti].commit_ts();
                    for row in model.iter_mut() {
                        if let Some((owner, v)) = row.pending {
                            if owner == ti {
                                row.committed.push((ts, v));
                                row.pending = None;
                            }
                        }
                    }
                }
                _ => {}
            }
        }
        for (ti, txn) in txns.iter().enumerate().filter(|(ti, _)| !done[*ti]) {
            prop_assert_eq!(query(&db, txn, "SELECT a, c FROM p"), expected_rows(&model, ti, txn), "transaction {} (read ts {})", ti, txn.read_ts());
        }
        // a transaction that begins now sees every committed version and no pending one
        let fresh = begin(&db);
        let mut latest: Vec<String> = model.iter().filter_map(|row| cells_text(&row.committed.last().and_then(|(_, v)| *v))).collect();
        latest.sort();
        prop_assert_eq!(query(&db, &fresh, "SELECT a, c FROM p"), latest, "a new transaction sees the latest committed state");
    }
}

// @@ challenge 4a-c1 begin
mod ch_4a_c1 {
    use proptest::prelude::*;

    use bustub::concurrency::read_view::ReadView;
    use std::collections::BTreeSet;

    #[test]
    fn s4a_c1_the_three_rules_on_a_small_view() {
        let v = ReadView::new(5, &[3, 5, 7], 9);
        assert!(v.visible(2), "committed before the snapshot");
        assert!(!v.visible(3), "was active");
        assert!(v.visible(4), "this check: expected `v.visible(4)`");
        assert!(v.visible(5), "my own writes");
        assert!(!v.visible(7), "this check: expected `!v.visible(7)`");
        assert!(!v.visible(9), "began after the snapshot");
        assert!(!v.visible(100), "this check: expected `!v.visible(100)`");
    }

    #[test]
    fn s4a_c1_an_empty_active_set_sees_every_older_transaction() {
        let v = ReadView::new(10, &[], 10);
        assert!((0..10).all(|w| v.visible(w)), "this check: expected `(0..10).all(|w| v.visible(w))`");
        assert!(v.visible(10), "this check: expected `v.visible(10)`");
        assert!(!v.visible(11), "this check: expected `!v.visible(11)`");
    }

    #[test]
    fn s4a_c1_own_writes_are_visible_even_when_listed_active() {
        let v = ReadView::new(3, &[3], 4);
        assert!(v.visible(3), "this check: expected `v.visible(3)`");
    }

    #[test]
    fn s4a_c1_the_boundary_id_is_exclusive() {
        let v = ReadView::new(1, &[], 6);
        assert!(v.visible(5), "this check: expected `v.visible(5)`");
        assert!(!v.visible(6), "this check: expected `!v.visible(6)`");
    }

    proptest! {
        #![proptest_config(ProptestConfig { cases: 256, failure_persistence: None, ..ProptestConfig::default() })]

        /// Property: against the definition with sets, and adding an active id only removes visibility.
        #[test]
        fn s4a_c1_property_visibility_follows_the_definition(own in 0u64..12, active in proptest::collection::btree_set(0u64..12, 0..6), next in 0u64..14, extra in 0u64..12, w in 0u64..16) {
            let act: Vec<u64> = active.iter().copied().collect();
            let v = ReadView::new(own, &act, next);
            let want = w == own || (w < next && !active.contains(&w));
            prop_assert_eq!(v.visible(w), want);
            let mut more: BTreeSet<u64> = active.clone();
            more.insert(extra);
            let v2 = ReadView::new(own, &more.iter().copied().collect::<Vec<_>>(), next);
            if v2.visible(w) { prop_assert!(v.visible(w)); }
        }
    }
}
// @@ challenge 4a-c1 end

// @@ challenge 4a-c2 begin
mod ch_4a_c2 {
    use proptest::prelude::*;

    use bustub::concurrency::version_read::{read_version, Stamp::*, Version};

    fn v(writer: u64, stamp: bustub::concurrency::version_read::Stamp, value: Option<i64>) -> Version {
        Version { writer, stamp, value }
    }

    fn chain() -> Vec<Version> {
        vec![v(7, Pending, Some(30)), v(2, Commit(5), Some(20)), v(1, Commit(2), Some(10))]
    }

    #[test]
    fn s4a_c2_a_reader_sees_the_newest_committed_version_at_its_timestamp() {
        assert_eq!(read_version(&chain(), 1, 9), None, "nothing was committed yet at ts 1");
        assert_eq!(read_version(&chain(), 4, 9), Some(10), "this check");
        assert_eq!(read_version(&chain(), 5, 9), Some(20), "this check");
        assert_eq!(read_version(&chain(), 100, 9), Some(20), "this check");
    }

    #[test]
    fn s4a_c2_the_writer_sees_its_own_pending_version() {
        assert_eq!(read_version(&chain(), 5, 7), Some(30), "this check");
        assert_eq!(read_version(&chain(), 0, 7), Some(30), "its own write is visible whatever the timestamp");
    }

    #[test]
    fn s4a_c2_a_committed_delete_hides_the_row_after_its_timestamp() {
        let c = vec![v(3, Commit(5), None), v(1, Commit(2), Some(10))];
        assert_eq!(read_version(&c, 3, 9), Some(10), "this check");
        assert_eq!(read_version(&c, 6, 9), None, "this check");
    }

    #[test]
    fn s4a_c2_a_pending_delete_by_someone_else_is_invisible() {
        let c = vec![v(7, Pending, None), v(1, Commit(2), Some(10))];
        assert_eq!(read_version(&c, 5, 9), Some(10), "this check");
        assert_eq!(read_version(&c, 5, 7), None, "but the deleter sees its own delete");
    }

    #[test]
    fn s4a_c2_an_empty_chain_is_no_row() {
        assert_eq!(read_version(&[], 10, 1), None, "this check");
    }

    proptest! {
        #![proptest_config(ProptestConfig { cases: 256, failure_persistence: None, ..ProptestConfig::default() })]

        /// Property: equals a scan for the newest visible version; raising the timestamp never reads an older version.
        #[test]
        fn s4a_c2_property_reading_equals_a_scan(ts_list in proptest::collection::btree_set(1u64..20, 0..6), pending in proptest::option::of((0u64..4, proptest::option::of(0i64..50))), read_ts in 0u64..22, reader in 0u64..4) {
            let mut stamps: Vec<u64> = ts_list.into_iter().collect();
            stamps.reverse(); // newest first
            let mut chain: Vec<Version> = Vec::new();
            if let Some((w, val)) = pending { chain.push(v(w, Pending, val)); }
            for (i, ts) in stamps.iter().enumerate() { chain.push(v(9, Commit(*ts), if i % 3 == 2 { None } else { Some(*ts as i64) })); }
            let want = chain.iter().find(|x| match x.stamp { Commit(t) => t <= read_ts, Pending => x.writer == reader }).and_then(|x| x.value);
            prop_assert_eq!(read_version(&chain, read_ts, reader), want);
            // a later reader never sees an older committed version: compare the committed-only chains
            let committed: Vec<Version> = chain.iter().filter(|x| x.stamp != Pending).cloned().collect();
            let early = committed.iter().position(|x| matches!(x.stamp, Commit(t) if t <= read_ts));
            let late = committed.iter().position(|x| matches!(x.stamp, Commit(t) if t <= read_ts + 5));
            if let (Some(e), Some(l)) = (early, late) { prop_assert!(l <= e, "a later timestamp must not pick an older version"); }
        }
    }
}
// @@ challenge 4a-c2 end

// @@ challenge 4a-c3 begin
mod ch_4a_c3 {
    use proptest::prelude::*;

    use bustub::concurrency::ts_oracle::TsOracle;

    #[test]
    fn s4a_c3_a_commit_that_finished_early_does_not_pull_the_snapshot_over_a_gap() {
        let mut o = TsOracle::new();
        assert_eq!((o.reserve(), o.reserve(), o.reserve()), (1, 2, 3), "this check");
        assert_eq!(o.begin(), 0, "this check");
        o.finish(2);
        assert_eq!(o.begin(), 0, "commit 1 is not finished: a snapshot at 2 would show commit 2 without commit 1");
        o.finish(1);
        assert_eq!(o.begin(), 2, "this check");
        o.finish(3);
        assert_eq!(o.begin(), 3, "this check");
    }

    #[test]
    fn s4a_c3_nothing_reserved_means_timestamp_zero() {
        assert_eq!(TsOracle::new().begin(), 0, "this check");
    }

    #[test]
    fn s4a_c3_finishing_in_order_advances_one_at_a_time() {
        let mut o = TsOracle::new();
        for i in 1..=5 {
            o.reserve();
            o.finish(i);
            assert_eq!(o.begin(), i, "this check");
        }
    }

    #[test]
    fn s4a_c3_odd_calls_are_ignored() {
        let mut o = TsOracle::new();
        o.reserve();
        o.finish(5);
        o.finish(0);
        assert_eq!(o.begin(), 0, "this check");
        o.finish(1);
        o.finish(1);
        assert_eq!(o.begin(), 1, "this check");
    }

    proptest! {
        #![proptest_config(ProptestConfig { cases: 256, failure_persistence: None, ..ProptestConfig::default() })]

        /// Property: `begin` is the largest t with every reserved ts <= t finished, and never decreases.
        #[test]
        fn s4a_c3_property_the_read_timestamp_is_a_finished_prefix(n in 1u64..8, order in proptest::collection::vec(0usize..8, 0..16)) {
            let mut o = TsOracle::new();
            for _ in 0..n { o.reserve(); }
            let mut done = std::collections::BTreeSet::new();
            let mut last = 0;
            for k in order {
                let ts = (k as u64 % n) + 1;
                o.finish(ts);
                done.insert(ts);
                let want = (0..n).take_while(|i| done.contains(&(i + 1))).count() as u64;
                let got = o.begin();
                prop_assert_eq!(got, want);
                prop_assert!(got >= last);
                last = got;
            }
        }
    }
}
// @@ challenge 4a-c3 end

// @@ challenge 4a-c4 begin
mod ch_4a_c4 {
    use proptest::prelude::*;

    use bustub::concurrency::undo::{apply_undo, make_undo, UndoLog};

    #[test]
    fn s4a_c4_an_update_records_only_the_changed_columns() {
        let log = make_undo(Some(&[1, 2, 3]), Some(&[1, 9, 3]));
        assert_eq!(log, UndoLog::Columns { mask: vec![false, true, false], values: vec![2] }, "this check");
        assert_eq!(apply_undo(Some(&[1, 9, 3]), &log), Some(vec![1, 2, 3]), "this check");
    }

    #[test]
    fn s4a_c4_an_insert_is_undone_by_removing_and_a_delete_by_restoring() {
        assert_eq!(make_undo(None, Some(&[4])), UndoLog::Remove, "this check");
        assert_eq!(apply_undo(Some(&[4]), &UndoLog::Remove), None, "this check");
        let del = make_undo(Some(&[4, 5]), None);
        assert_eq!(del, UndoLog::Restore(vec![4, 5]), "this check");
        assert_eq!(apply_undo(None, &del), Some(vec![4, 5]), "this check");
    }

    #[test]
    fn s4a_c4_an_unchanged_row_has_an_empty_mask_and_changes_nothing() {
        let log = make_undo(Some(&[7, 8]), Some(&[7, 8]));
        assert_eq!(log, UndoLog::Columns { mask: vec![false, false], values: vec![] }, "this check");
        assert_eq!(apply_undo(Some(&[7, 8]), &log), Some(vec![7, 8]), "this check");
    }

    #[test]
    fn s4a_c4_a_chain_of_updates_is_walked_back_newest_first() {
        let versions = [vec![1, 1, 1], vec![1, 2, 1], vec![3, 2, 1], vec![3, 2, 9]];
        let logs: Vec<UndoLog> = (1..versions.len()).map(|i| make_undo(Some(&versions[i - 1]), Some(&versions[i]))).collect();
        let mut cur = Some(versions[3].clone());
        for i in (0..3).rev() {
            cur = apply_undo(cur.as_deref(), &logs[i]);
            assert_eq!(cur.as_ref(), Some(&versions[i]), "undoing back to version {i}");
        }
    }

    proptest! {
        #![proptest_config(ProptestConfig { cases: 256, failure_persistence: None, ..ProptestConfig::default() })]

        /// Property: undoing a change gives back the old version, for any pair of versions and any chain.
        #[test]
        fn s4a_c4_property_undo_restores_the_old_version(cols in 1usize..5, a in proptest::collection::vec(-3i64..3, 5), b in proptest::collection::vec(-3i64..3, 5), c in proptest::collection::vec(-3i64..3, 5), kinds in (any::<bool>(), any::<bool>())) {
            let (a, b, c) = (a[..cols].to_vec(), b[..cols].to_vec(), c[..cols].to_vec());
            let old = if kinds.0 { Some(a.as_slice()) } else { None };
            let new = if kinds.1 { Some(b.as_slice()) } else { None };
            let log = make_undo(old, new);
            prop_assert_eq!(apply_undo(new, &log), old.map(|r| r.to_vec()));
            if let UndoLog::Columns { mask, values } = &log {
                prop_assert_eq!(values.len(), mask.iter().filter(|m| **m).count());
                for (i, m) in mask.iter().enumerate() { prop_assert_eq!(*m, a[i] != b[i]); }
            }
            let l1 = make_undo(Some(&a), Some(&b));
            let l2 = make_undo(Some(&b), Some(&c));
            let back_b = apply_undo(Some(&c), &l2);
            prop_assert_eq!(back_b.as_ref(), Some(&b));
            prop_assert_eq!(apply_undo(back_b.as_deref(), &l1), Some(a));
        }
    }
}
// @@ challenge 4a-c4 end

// @@ challenge 4a-c5 begin
mod ch_4a_c5 {
    use proptest::prelude::*;

    use bustub::concurrency::timestamp_ordering::{Abort, ToItem};

    #[test]
    fn s4a_c5_a_read_that_arrives_after_a_younger_write_aborts() {
        let mut i = ToItem::new(0);
        assert_eq!(i.write(5, 1), Ok(()), "this check");
        assert_eq!(i.read(3), Err(Abort), "this check");
        assert_eq!(i.read(5), Ok(1), "this check");
        assert_eq!(i.read(7), Ok(1), "this check");
    }

    #[test]
    fn s4a_c5_a_write_that_arrives_after_a_younger_read_aborts() {
        let mut i = ToItem::new(0);
        i.read(7).unwrap();
        assert_eq!(i.write(6, 2), Err(Abort), "this check");
        assert_eq!(i.write(8, 3), Ok(()), "this check");
        assert_eq!(i.read(9), Ok(3), "this check");
    }

    #[test]
    fn s4a_c5_a_write_older_than_the_last_write_aborts() {
        let mut i = ToItem::new(0);
        i.write(5, 1).unwrap();
        assert_eq!(i.write(4, 2), Err(Abort), "this check");
        assert_eq!(i.read(6), Ok(1), "this check");
    }

    #[test]
    fn s4a_c5_a_refused_operation_changes_nothing() {
        let mut i = ToItem::new(9);
        i.write(5, 1).unwrap();
        let before = i.timestamps();
        let _ = i.read(2);
        let _ = i.write(3, 77);
        assert_eq!(i.timestamps(), before, "this check");
        assert_eq!(i.read(5), Ok(1), "this check");
    }

    #[test]
    fn s4a_c5_the_largest_timestamp_is_never_refused() {
        let mut i = ToItem::new(0);
        for ts in 1..20 {
            assert!(i.write(ts, ts as i64).is_ok(), "this check: expected `i.write(ts, ts as i64).is_ok()`");
            assert_eq!(i.read(ts), Ok(ts as i64), "this check");
        }
    }

    proptest! {
        #![proptest_config(ProptestConfig { cases: 256, failure_persistence: None, ..ProptestConfig::default() })]

        /// Property: every accepted read returned the value of the accepted write with the largest timestamp not above the reader's.
        #[test]
        fn s4a_c5_property_accepted_reads_match_a_serial_replay(ops in proptest::collection::vec((any::<bool>(), 1u64..8, 0i64..50), 0..40)) {
            let mut item = ToItem::new(-1);
            let mut accepted_writes: Vec<(u64, i64)> = vec![(0, -1)];
            for (is_write, ts, v) in ops {
                if is_write {
                    if item.write(ts, v).is_ok() { accepted_writes.push((ts, v)); }
                } else if let Ok(got) = item.read(ts) {
                    let want = accepted_writes.iter().filter(|w| w.0 <= ts).max_by_key(|w| w.0).map(|w| w.1);
                    prop_assert_eq!(Some(got), want);
                }
                let (r, w) = item.timestamps();
                prop_assert!(accepted_writes.iter().all(|a| a.0 <= w));
                let _ = r;
            }
        }
    }
}
// @@ challenge 4a-c5 end
