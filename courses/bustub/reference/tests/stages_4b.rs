//! Tests for module 4b: MVCC writes, abort, garbage collection, the primary-key index and serializable validation.

use std::collections::BTreeMap;
use std::sync::Arc;

use proptest::prelude::*;

use bustub::catalog::catalog::TableInfo;
use bustub::common::bustub_instance::BusTubInstance;
use bustub::common::result_writer::SimpleStreamWriter;
use bustub::concurrency::transaction::{IsolationLevel, Transaction, TransactionState, TXN_START_ID};
use bustub::execution::execution_common::is_write_write_conflict;
use bustub::storage::table::tuple::TupleMeta;

fn new_db() -> BusTubInstance {
    BusTubInstance::new(256)
}

/// A statement outside any transaction (DDL).
fn exec(db: &BusTubInstance, sql: &str) {
    let mut out = String::new();
    assert!(db.execute_sql(sql, &mut SimpleStreamWriter::new(&mut out, true, " "), None).unwrap_or_else(|e| panic!("{sql}: {e}")), "{sql} failed");
}

fn begin(db: &BusTubInstance) -> Arc<Transaction> {
    db.txn_manager.begin(IsolationLevel::SnapshotIsolation).unwrap()
}

fn begin_serializable(db: &BusTubInstance) -> Arc<Transaction> {
    db.txn_manager.begin(IsolationLevel::Serializable).unwrap()
}

fn commit(db: &BusTubInstance, txn: &Arc<Transaction>) {
    assert!(db.txn_manager.commit(txn).unwrap(), "commit failed");
    assert_eq!(txn.state(), TransactionState::Committed, "in helper `commit`");
}

fn abort(db: &BusTubInstance, txn: &Arc<Transaction>) {
    db.txn_manager.abort(txn).unwrap();
    assert_eq!(txn.state(), TransactionState::Aborted, "in helper `abort`");
}

/// Runs a statement in a transaction; its success.
fn try_run(db: &BusTubInstance, txn: &Arc<Transaction>, sql: &str) -> (bool, String) {
    let mut out = String::new();
    let ok = db.execute_sql_txn(sql, &mut SimpleStreamWriter::new(&mut out, true, " "), txn).unwrap_or_else(|e| panic!("{sql}: {e}"));
    (ok, out)
}

fn run(db: &BusTubInstance, txn: &Arc<Transaction>, sql: &str) {
    let (ok, _) = try_run(db, txn, sql);
    assert!(ok, "{sql} failed");
}

/// The statement must fail with a write-write conflict that taints the transaction.
fn run_tainted(db: &BusTubInstance, txn: &Arc<Transaction>, sql: &str) {
    let (ok, _) = try_run(db, txn, sql);
    assert!(!ok, "{sql} should have failed");
    assert_eq!(txn.state(), TransactionState::Tainted, "{sql}");
}

/// The rows of a query, one string per row, sorted.
fn rows(db: &BusTubInstance, txn: &Arc<Transaction>, sql: &str) -> Vec<String> {
    let (ok, out) = try_run(db, txn, sql);
    assert!(ok, "{sql} failed");
    let mut rows: Vec<String> = out.lines().map(|l| l.trim_end().to_string()).filter(|l| !l.is_empty()).collect();
    rows.sort();
    rows
}

fn expect(db: &BusTubInstance, txn: &Arc<Transaction>, sql: &str, expected: &[&str]) {
    let mut want: Vec<String> = expected.iter().map(|s| s.to_string()).collect();
    want.sort();
    assert_eq!(rows(db, txn, sql), want, "{sql} in txn{}", txn.human_readable_id());
}

fn table(db: &BusTubInstance, name: &str) -> Arc<TableInfo<'static>> {
    db.catalog.read().unwrap().get_table(name).unwrap()
}

/// How many tuples (live or dead) the heap holds.
fn heap_entries(table: &TableInfo<'_>) -> usize {
    let mut iter = table.table.make_eager_iterator();
    let mut n = 0;
    while !iter.is_end() {
        iter.advance();
        n += 1;
    }
    n
}

fn undo_log_num(txn: &Transaction) -> usize {
    txn.undo_log_num()
}

/// The number of columns the transaction's only undo log restores.
fn undo_log_columns(txn: &Transaction) -> usize {
    assert_eq!(txn.undo_log_num(), 1, "one transaction holds at most one undo log for a tuple");
    txn.get_undo_log(0).modified_fields.iter().filter(|m| **m).count()
}

fn bump_commit_ts(db: &BusTubInstance, by: usize) {
    for _ in 0..by {
        let t = begin(db);
        commit(db, &t);
    }
}

fn txn_exists(db: &BusTubInstance, txn: &Transaction) -> bool {
    db.txn_manager.txn_map.read().unwrap().contains_key(&txn.id())
}

fn ensure_index_scan(db: &BusTubInstance) {
    exec(db, "CREATE TABLE pk_test_table_n(pk int primary key)");
    exec(db, "set force_optimizer_starter_rule=yes");
    let t = begin(db);
    let (_, plan) = try_run(db, &t, "EXPLAIN SELECT * FROM pk_test_table_n WHERE pk = 1");
    assert!(plan.contains("IndexScan"), "the point lookup should plan as an index scan:\n{plan}");
    abort(db, &t);
}

// ---- 4b-01: insert ----------------------------------------------------------------------------------------------------------------------

#[test]
fn s4b_01_an_inserted_tuple_is_visible_only_to_its_transaction() {
    let db = new_db();
    exec(&db, "CREATE TABLE maintable(a int)");
    let (t1, t2, t_ref) = (begin(&db), begin(&db), begin(&db));
    run(&db, &t1, "INSERT INTO maintable VALUES (1)");
    run(&db, &t2, "INSERT INTO maintable VALUES (2)");
    expect(&db, &t1, "SELECT a FROM maintable", &["1"]);
    expect(&db, &t2, "SELECT a FROM maintable", &["2"]);
    let t3 = begin(&db);
    expect(&db, &t3, "SELECT a FROM maintable", &[]);
    expect(&db, &t_ref, "SELECT a FROM maintable", &[]);
}

#[test]
fn s4b_01_insert_reports_how_many_rows_it_inserted() {
    let db = new_db();
    exec(&db, "CREATE TABLE maintable(a int)");
    let t = begin(&db);
    let (ok, out) = try_run(&db, &t, "INSERT INTO maintable VALUES (1), (2), (3)");
    assert!(ok, "insert reports how many rows it inserted: expected `ok`");
    assert_eq!(out.trim(), "3", "insert reports how many rows it inserted");
}

#[test]
fn s4b_01_the_tuple_carries_the_temporary_timestamp_until_commit() {
    let db = new_db();
    exec(&db, "CREATE TABLE maintable(a int)");
    let info = table(&db, "maintable");
    let t = begin(&db);
    run(&db, &t, "INSERT INTO maintable VALUES (1)");
    let rid = *t.write_sets().get(&info.oid).unwrap().iter().next().expect("the insert joined the write set");
    assert_eq!(info.table.get_tuple_meta(rid).unwrap(), TupleMeta { ts: t.temp_ts(), is_deleted: false }, "the tuple carries the temporary timestamp until commit");
    assert_eq!(undo_log_num(&t), 0, "a new tuple needs no undo log");
    commit(&db, &t);
    assert_eq!(info.table.get_tuple_meta(rid).unwrap(), TupleMeta { ts: t.commit_ts(), is_deleted: false }, "the tuple carries the temporary timestamp until commit");
}

#[test]
fn s4b_01_a_commit_makes_the_tuple_visible_to_later_transactions_only() {
    let db = new_db();
    exec(&db, "CREATE TABLE maintable(a int)");
    let (t1, t2) = (begin(&db), begin(&db));
    run(&db, &t1, "INSERT INTO maintable VALUES (1)");
    run(&db, &t2, "INSERT INTO maintable VALUES (2)");
    commit(&db, &t1);
    let t_ref = begin(&db);
    let t3 = begin(&db);
    expect(&db, &t3, "SELECT a FROM maintable", &["1"]);
    expect(&db, &t2, "SELECT a FROM maintable", &["2"]);
    run(&db, &t3, "INSERT INTO maintable VALUES (3)");
    expect(&db, &t3, "SELECT a FROM maintable", &["1", "3"]);
    expect(&db, &t2, "SELECT a FROM maintable", &["2"]);
    commit(&db, &t3);
    commit(&db, &t2);
    let t4 = begin(&db);
    expect(&db, &t4, "SELECT a FROM maintable", &["1", "2", "3"]);
    expect(&db, &t_ref, "SELECT a FROM maintable", &["1"]);
}

#[test]
fn s4b_01_an_insert_select_in_a_transaction_does_not_see_its_own_new_rows() {
    let db = new_db();
    exec(&db, "CREATE TABLE maintable(a int)");
    let t = begin(&db);
    run(&db, &t, "INSERT INTO maintable VALUES (1), (2)");
    run(&db, &t, "INSERT INTO maintable SELECT a + 10 FROM maintable");
    expect(&db, &t, "SELECT a FROM maintable", &["1", "2", "11", "12"]);
}

// ---- 4b-02: delete and write-write conflicts -------------------------------------------------------------------------------------------

fn meta(ts: i64) -> TupleMeta {
    TupleMeta { ts, is_deleted: false }
}

#[test]
fn s4b_02_a_tuple_is_in_conflict_if_it_is_newer_than_the_reader_and_not_its_own() {
    let db = new_db();
    bump_commit_ts(&db, 2);
    let t = begin(&db); // read ts 2
    assert!(!is_write_write_conflict(&meta(0), &t), "a tuple is in conflict if it is newer than the reader and not its own: expected `!is_write_write_conflict(&meta(0), &t)`");
    assert!(!is_write_write_conflict(&meta(2), &t), "committed at the read timestamp: seen");
    assert!(is_write_write_conflict(&meta(3), &t), "committed after the transaction began");
    assert!(is_write_write_conflict(&meta(TXN_START_ID + 99), &t), "another transaction's uncommitted write");
    assert!(!is_write_write_conflict(&meta(t.temp_ts()), &t), "its own write");
}

#[test]
fn s4b_02_delete_hides_the_tuple_from_the_deleter_only() {
    let db = new_db();
    exec(&db, "CREATE TABLE maintable(a int)");
    let t0 = begin(&db);
    run(&db, &t0, "INSERT INTO maintable VALUES (1), (2), (3)");
    commit(&db, &t0);
    let (t1, t2) = (begin(&db), begin(&db));
    let (_, out) = try_run(&db, &t1, "DELETE FROM maintable WHERE a = 2");
    assert_eq!(out.trim(), "1", "delete hides the tuple from the deleter only");
    expect(&db, &t1, "SELECT a FROM maintable", &["1", "3"]);
    expect(&db, &t2, "SELECT a FROM maintable", &["1", "2", "3"]);
    commit(&db, &t1);
    expect(&db, &t2, "SELECT a FROM maintable", &["1", "2", "3"]);
    expect(&db, &begin(&db), "SELECT a FROM maintable", &["1", "3"]);
}

#[test]
fn s4b_02_deleting_a_committed_tuple_leaves_one_full_undo_log() {
    let db = new_db();
    exec(&db, "CREATE TABLE maintable(a int, b int)");
    let info = table(&db, "maintable");
    let t0 = begin(&db);
    run(&db, &t0, "INSERT INTO maintable VALUES (1, 10)");
    commit(&db, &t0);
    let t1 = begin(&db);
    run(&db, &t1, "DELETE FROM maintable");
    assert_eq!(undo_log_columns(&t1), 2, "a delete logs every column");
    let rid = *t1.write_sets().get(&info.oid).unwrap().iter().next().unwrap();
    assert!(info.table.get_tuple_meta(rid).unwrap().is_deleted, "deleting a committed tuple leaves one full undo log: expected `info.table.get_tuple_meta(rid).unwrap().is_deleted`");
    assert_eq!(db.txn_manager.get_undo_link(rid).unwrap().prev_txn, t1.id(), "deleting a committed tuple leaves one full undo log");
    assert_eq!(t1.get_undo_log(0).ts, t0.commit_ts(), "the log restores the version committed by t0");
}

#[test]
fn s4b_02_deleting_a_tuple_the_transaction_inserted_needs_no_log() {
    let db = new_db();
    exec(&db, "CREATE TABLE maintable(a int)");
    let t = begin(&db);
    run(&db, &t, "INSERT INTO maintable VALUES (1)");
    run(&db, &t, "INSERT INTO maintable VALUES (2)");
    run(&db, &t, "INSERT INTO maintable VALUES (3)");
    run(&db, &t, "DELETE FROM maintable WHERE a = 3");
    assert_eq!(undo_log_num(&t), 0, "deleting a tuple the transaction inserted needs no log");
    expect(&db, &t, "SELECT a FROM maintable", &["1", "2"]);
    commit(&db, &t);
    expect(&db, &begin(&db), "SELECT a FROM maintable", &["1", "2"]);
}

#[test]
fn s4b_02_a_second_deleter_conflicts_and_is_tainted() {
    let db = new_db();
    exec(&db, "CREATE TABLE maintable(a int)");
    let t0 = begin(&db);
    run(&db, &t0, "INSERT INTO maintable VALUES (1), (2)");
    commit(&db, &t0);
    let (t1, t2) = (begin(&db), begin(&db));
    run(&db, &t1, "DELETE FROM maintable WHERE a = 2");
    run_tainted(&db, &t2, "DELETE FROM maintable WHERE a = 2");
    expect(&db, &t1, "SELECT a FROM maintable", &["1"]);
}

#[test]
fn s4b_02_a_delete_of_a_tuple_committed_after_the_transaction_began_conflicts() {
    let db = new_db();
    exec(&db, "CREATE TABLE maintable(a int)");
    let t0 = begin(&db);
    run(&db, &t0, "INSERT INTO maintable VALUES (1), (2)");
    commit(&db, &t0);
    let (t1, t2) = (begin(&db), begin(&db));
    run(&db, &t1, "DELETE FROM maintable WHERE a = 2");
    commit(&db, &t1);
    // t2 still sees (2) but the tuple was deleted by a commit after its read timestamp
    expect(&db, &t2, "SELECT a FROM maintable", &["1", "2"]);
    run_tainted(&db, &t2, "DELETE FROM maintable WHERE a = 2");
}

#[test]
fn s4b_02_a_tainted_transaction_can_not_commit_or_run_more_statements() {
    let db = new_db();
    exec(&db, "CREATE TABLE maintable(a int)");
    let t0 = begin(&db);
    run(&db, &t0, "INSERT INTO maintable VALUES (1)");
    commit(&db, &t0);
    let (t1, t2) = (begin(&db), begin(&db));
    run(&db, &t1, "DELETE FROM maintable");
    run_tainted(&db, &t2, "DELETE FROM maintable");
    assert!(!db.txn_manager.commit(&t2).unwrap(), "a tainted transaction can not commit or run more statements: expected `!db.txn_manager.commit(&t2).unwrap()`");
    assert_eq!(t2.state(), TransactionState::Tainted, "a tainted transaction can not commit or run more statements");
    let mut out = String::new();
    assert!(db.execute_sql_txn("SELECT a FROM maintable", &mut SimpleStreamWriter::new(&mut out, true, " "), &t2).is_err(), "a tainted transaction can not commit or run more statements: expected `db.execute_sql_txn(\"SELECT a FROM maintable\", &mut SimpleStreamWriter::new(&mut out, true, \" \"), &t2...`");
}

// ---- 4b-03: update ---------------------------------------------------------------------------------------------------------------------

#[test]
fn s4b_03_an_update_is_in_place_and_the_old_version_stays_for_older_readers() {
    let db = new_db();
    exec(&db, "CREATE TABLE table2(a int, b int, c int)");
    let info = table(&db, "table2");
    let t0 = begin(&db);
    run(&db, &t0, "INSERT INTO table2 VALUES (1, 1, 1)");
    commit(&db, &t0);
    let (t1, t_ref) = (begin(&db), begin(&db));
    run(&db, &t1, "UPDATE table2 SET b = 2");
    expect(&db, &t1, "SELECT * FROM table2", &["1 2 1"]);
    expect(&db, &t_ref, "SELECT * FROM table2", &["1 1 1"]);
    assert_eq!(undo_log_columns(&t1), 1, "an update is in place and the old version stays for older readers");
    assert_eq!(heap_entries(&info), 1, "the tuple was updated in place, not re-inserted");
}

#[test]
fn s4b_03_a_tuple_the_transaction_inserted_is_updated_without_any_log() {
    let db = new_db();
    exec(&db, "CREATE TABLE table1(a int, b int, c int)");
    let t_ref = begin(&db);
    let t1 = begin(&db);
    run(&db, &t1, "INSERT INTO table1 VALUES (1, 1, 1)");
    run(&db, &t1, "UPDATE table1 SET b = 2");
    run(&db, &t1, "UPDATE table1 SET a = 4, b = 4, c = 4");
    expect(&db, &t1, "SELECT * FROM table1", &["4 4 4"]);
    expect(&db, &t_ref, "SELECT * FROM table1", &[]);
    assert_eq!(undo_log_num(&t1), 0, "a tuple the transaction inserted is updated without any log");
}

#[test]
fn s4b_03_changing_the_same_tuple_again_widens_its_log_instead_of_adding_one() {
    let db = new_db();
    exec(&db, "CREATE TABLE table2(a int, b int, c int)");
    let t0 = begin(&db);
    run(&db, &t0, "INSERT INTO table2 VALUES (1, 1, 1)");
    commit(&db, &t0);
    let (t1, t_ref) = (begin(&db), begin(&db));
    run(&db, &t1, "UPDATE table2 SET b = 2");
    assert_eq!(undo_log_columns(&t1), 1, "changing the same tuple again widens its log instead of adding one");
    run(&db, &t1, "UPDATE table2 SET b = 3");
    assert_eq!(undo_log_columns(&t1), 1, "the same column again");
    run(&db, &t1, "UPDATE table2 SET a = 1");
    assert_eq!(undo_log_columns(&t1), 1, "not a real change");
    run(&db, &t1, "UPDATE table2 SET a = 2");
    assert_eq!(undo_log_columns(&t1), 2, "changing the same tuple again widens its log instead of adding one");
    run(&db, &t1, "UPDATE table2 SET a = 4, b = 4, c = 4");
    assert_eq!(undo_log_columns(&t1), 3, "changing the same tuple again widens its log instead of adding one");
    expect(&db, &t1, "SELECT * FROM table2", &["4 4 4"]);
    expect(&db, &t_ref, "SELECT * FROM table2", &["1 1 1"]);
}

#[test]
fn s4b_03_a_delete_after_updates_makes_the_log_cover_every_column() {
    let db = new_db();
    exec(&db, "CREATE TABLE table2(a int, b int, c int)");
    let t0 = begin(&db);
    run(&db, &t0, "INSERT INTO table2 VALUES (1, 1, 1)");
    commit(&db, &t0);
    let (t1, t_ref) = (begin(&db), begin(&db));
    run(&db, &t1, "UPDATE table2 SET b = 2");
    run(&db, &t1, "DELETE FROM table2");
    assert_eq!(undo_log_columns(&t1), 3, "a delete after updates makes the log cover every column");
    expect(&db, &t1, "SELECT * FROM table2", &[]);
    expect(&db, &t_ref, "SELECT * FROM table2", &["1 1 1"]);
    commit(&db, &t1);
    expect(&db, &begin(&db), "SELECT * FROM table2", &[]);
    expect(&db, &t_ref, "SELECT * FROM table2", &["1 1 1"]);
}

#[test]
fn s4b_03_updates_on_top_of_a_version_chain_keep_every_older_snapshot() {
    let db = new_db();
    exec(&db, "CREATE TABLE table2(a int, b int, c int)");
    let info = table(&db, "table2");
    let t00 = begin(&db);
    run(&db, &t00, "INSERT INTO table2 VALUES (0, 0, 0)");
    commit(&db, &t00);
    let ref0 = begin(&db);
    let t01 = begin(&db);
    run(&db, &t01, "UPDATE table2 SET a = 1, b = 1, c = 1");
    commit(&db, &t01);
    let (t1, ref1) = (begin(&db), begin(&db));
    run(&db, &t1, "UPDATE table2 SET b = 2");
    run(&db, &t1, "UPDATE table2 SET a = 2");
    run(&db, &t1, "DELETE FROM table2");
    commit(&db, &t1);
    expect(&db, &ref0, "SELECT * FROM table2", &["0 0 0"]);
    expect(&db, &ref1, "SELECT * FROM table2", &["1 1 1"]);
    expect(&db, &begin(&db), "SELECT * FROM table2", &[]);
    assert_eq!(heap_entries(&info), 1, "updates on top of a version chain keep every older snapshot");
}

#[test]
fn s4b_03_a_conflicting_updater_is_tainted_and_the_winner_commits() {
    let db = new_db();
    exec(&db, "CREATE TABLE table1(a int, b int, c int)");
    let info = table(&db, "table1");
    let t0 = begin(&db);
    run(&db, &t0, "INSERT INTO table1 VALUES (0, 0, 0)");
    commit(&db, &t0);
    let t_ref = begin(&db);
    let (t1, t2) = (begin(&db), begin(&db));
    run(&db, &t1, "UPDATE table1 SET a = 1");
    run_tainted(&db, &t2, "UPDATE table1 SET b = 2");
    commit(&db, &t1);
    expect(&db, &t_ref, "SELECT * FROM table1", &["0 0 0"]);
    assert_eq!(heap_entries(&info), 1, "a conflicting updater is tainted and the winner commits");
}

// ---- 4b-04: abort ----------------------------------------------------------------------------------------------------------------------

#[test]
fn s4b_04_aborting_an_insert_leaves_nothing_behind() {
    let db = new_db();
    exec(&db, "CREATE TABLE maintable(a int, b int)");
    let t1 = begin(&db);
    run(&db, &t1, "INSERT INTO maintable VALUES (1, 10), (1, 11), (2, 20)");
    expect(&db, &t1, "SELECT a, b FROM maintable ORDER BY a, b", &["1 10", "1 11", "2 20"]);
    abort(&db, &t1);
    let check = begin(&db);
    expect(&db, &check, "SELECT a, b FROM maintable ORDER BY a, b", &[]);
}

#[test]
fn s4b_04_aborting_an_update_restores_the_values_and_the_timestamp() {
    let db = new_db();
    exec(&db, "CREATE TABLE maintable(a int, b int)");
    let info = table(&db, "maintable");
    let t0 = begin(&db);
    run(&db, &t0, "INSERT INTO maintable VALUES (1, 10), (2, 200), (3, 300)");
    commit(&db, &t0);
    let t3 = begin(&db);
    run(&db, &t3, "UPDATE maintable SET b = 0 WHERE a >= 2");
    expect(&db, &t3, "SELECT a, b FROM maintable", &["1 10", "2 0", "3 0"]);
    abort(&db, &t3);
    expect(&db, &begin(&db), "SELECT a, b FROM maintable", &["1 10", "2 200", "3 300"]);
    let mut iter = info.table.make_eager_iterator();
    while !iter.is_end() {
        let rid = iter.get_rid();
        iter.advance();
        assert_eq!(info.table.get_tuple_meta(rid).unwrap().ts, t0.commit_ts(), "the committed timestamp is back");
        assert!(db.txn_manager.get_undo_link(rid).is_none(), "the chain is as it was");
    }
}

#[test]
fn s4b_04_aborting_a_delete_brings_the_rows_back() {
    let db = new_db();
    exec(&db, "CREATE TABLE maintable(a int, b int)");
    let t0 = begin(&db);
    run(&db, &t0, "INSERT INTO maintable VALUES (1, 10), (2, 200)");
    commit(&db, &t0);
    let t4 = begin(&db);
    run(&db, &t4, "DELETE FROM maintable");
    expect(&db, &t4, "SELECT a, b FROM maintable", &[]);
    abort(&db, &t4);
    expect(&db, &begin(&db), "SELECT a, b FROM maintable", &["1 10", "2 200"]);
}

#[test]
fn s4b_04_an_abort_after_several_changes_restores_the_version_before_the_transaction() {
    let db = new_db();
    exec(&db, "CREATE TABLE maintable(a int, b int)");
    let t0 = begin(&db);
    run(&db, &t0, "INSERT INTO maintable VALUES (1, 10)");
    commit(&db, &t0);
    let reader = begin(&db);
    let t = begin(&db);
    run(&db, &t, "UPDATE maintable SET a = 5");
    run(&db, &t, "UPDATE maintable SET b = 6");
    run(&db, &t, "DELETE FROM maintable");
    abort(&db, &t);
    expect(&db, &reader, "SELECT a, b FROM maintable", &["1 10"]);
    expect(&db, &begin(&db), "SELECT a, b FROM maintable", &["1 10"]);
}

#[test]
fn s4b_04_a_tainted_transaction_is_aborted_like_any_other_and_commits_nothing() {
    let db = new_db();
    exec(&db, "CREATE TABLE maintable(a int, b int)");
    let t0 = begin(&db);
    run(&db, &t0, "INSERT INTO maintable VALUES (1, 10), (2, 20)");
    commit(&db, &t0);
    let (t1, t2) = (begin(&db), begin(&db));
    run(&db, &t2, "UPDATE maintable SET b = 0 WHERE a = 1");
    run(&db, &t1, "UPDATE maintable SET b = 1 WHERE a = 2");
    run_tainted(&db, &t1, "UPDATE maintable SET b = 1 WHERE a = 1");
    abort(&db, &t1);
    commit(&db, &t2);
    expect(&db, &begin(&db), "SELECT a, b FROM maintable", &["1 0", "2 20"]);
}

#[test]
fn s4b_04_after_an_abort_another_transaction_may_write_the_tuple() {
    let db = new_db();
    exec(&db, "CREATE TABLE maintable(a int, b int)");
    let t0 = begin(&db);
    run(&db, &t0, "INSERT INTO maintable VALUES (1, 10)");
    commit(&db, &t0);
    let (t1, t2) = (begin(&db), begin(&db));
    run(&db, &t1, "UPDATE maintable SET b = 1");
    abort(&db, &t1);
    run(&db, &t2, "UPDATE maintable SET b = 2");
    commit(&db, &t2);
    expect(&db, &begin(&db), "SELECT a, b FROM maintable", &["1 2"]);
}

// ---- 4b-05: garbage collection ------------------------------------------------------------------------------------------------------------

#[test]
fn s4b_05_a_finished_transaction_without_undo_logs_is_forgotten() {
    let db = new_db();
    exec(&db, "CREATE TABLE table1(a int, b int, c int)");
    let t_a = begin(&db);
    run(&db, &t_a, "INSERT INTO table1 VALUES (0, 0, 0)");
    commit(&db, &t_a);
    let running = begin(&db);
    db.txn_manager.garbage_collection();
    assert!(!txn_exists(&db, &t_a), "committed, no logs");
    assert!(txn_exists(&db, &running), "still running");
}

#[test]
fn s4b_05_a_transaction_whose_logs_a_reader_may_still_need_stays() {
    let db = new_db();
    exec(&db, "CREATE TABLE table1(a int, b int, c int)");
    let t_a = begin(&db);
    run(&db, &t_a, "INSERT INTO table1 VALUES (0, 0, 0)");
    commit(&db, &t_a);
    let old_reader = begin(&db);
    let t_b = begin(&db);
    run(&db, &t_b, "UPDATE table1 SET a = 1");
    commit(&db, &t_b);
    db.txn_manager.garbage_collection();
    assert!(txn_exists(&db, &t_b), "old_reader may need its undo log");
    expect(&db, &old_reader, "SELECT * FROM table1", &["0 0 0"]);
    commit(&db, &old_reader);
    db.txn_manager.garbage_collection();
    assert!(!txn_exists(&db, &t_b), "nobody reads at a timestamp before its commit now");
    expect(&db, &begin(&db), "SELECT * FROM table1", &["1 0 0"]);
}

#[test]
fn s4b_05_only_the_logs_below_the_oldest_needed_version_go() {
    let db = new_db();
    exec(&db, "CREATE TABLE table1(a int, b int, c int)");
    let t0 = begin(&db);
    run(&db, &t0, "INSERT INTO table1 VALUES (0, 0, 0)");
    commit(&db, &t0);
    let t1 = begin(&db);
    run(&db, &t1, "UPDATE table1 SET a = 1");
    commit(&db, &t1);
    let mid_reader = begin(&db); // reads the version of t1
    let t2 = begin(&db);
    run(&db, &t2, "UPDATE table1 SET a = 2");
    commit(&db, &t2);
    db.txn_manager.garbage_collection();
    assert!(txn_exists(&db, &t2), "mid_reader needs t2's log to see a = 1");
    assert!(!txn_exists(&db, &t1), "t1's log restores a = 0, older than anything a reader can ask for");
    expect(&db, &mid_reader, "SELECT * FROM table1", &["1 0 0"]);
}

#[test]
fn s4b_05_tainted_and_running_transactions_are_never_collected() {
    let db = new_db();
    exec(&db, "CREATE TABLE table1(a int, b int, c int)");
    let t0 = begin(&db);
    run(&db, &t0, "INSERT INTO table1 VALUES (0, 0, 0)");
    commit(&db, &t0);
    let (t5, t6) = (begin(&db), begin(&db));
    run(&db, &t5, "DELETE FROM table1");
    run_tainted(&db, &t6, "DELETE FROM table1");
    db.txn_manager.garbage_collection();
    assert!(txn_exists(&db, &t5) && txn_exists(&db, &t6), "tainted and running transactions are never collected: expected `txn_exists(&db, &t5) && txn_exists(&db, &t6)`");
    abort(&db, &t6);
    db.txn_manager.garbage_collection();
    assert!(!txn_exists(&db, &t6), "aborted, and it left no logs in any chain");
}

#[test]
fn s4b_05_collecting_twice_changes_nothing_and_every_snapshot_still_reads_right() {
    let db = new_db();
    exec(&db, "CREATE TABLE table1(a int, b int, c int)");
    let t_a = begin(&db);
    run(&db, &t_a, "INSERT INTO table1 VALUES (0, 0, 0), (1, 1, 1)");
    commit(&db, &t_a);
    let r1 = begin(&db);
    let t2 = begin(&db);
    run(&db, &t2, "UPDATE table1 SET a = a + 10");
    commit(&db, &t2);
    let r2 = begin(&db);
    db.txn_manager.garbage_collection();
    db.txn_manager.garbage_collection();
    expect(&db, &r1, "SELECT * FROM table1", &["0 0 0", "1 1 1"]);
    expect(&db, &r2, "SELECT * FROM table1", &["10 0 0", "11 1 1"]);
}

// ---- 4b-06: the primary-key index ---------------------------------------------------------------------------------------------------------

#[test]
fn s4b_06_a_duplicate_primary_key_fails_and_taints_the_transaction() {
    let db = new_db();
    exec(&db, "CREATE TABLE maintable(a int primary key, b int)");
    let info = table(&db, "maintable");
    let t1 = begin(&db);
    run(&db, &t1, "INSERT INTO maintable VALUES (1, 0)");
    expect(&db, &t1, "SELECT * FROM maintable", &["1 0"]);
    run_tainted(&db, &t1, "INSERT INTO maintable VALUES (1, 1)");
    assert_eq!(heap_entries(&info), 1, "a duplicate primary key fails and taints the transaction");
}

#[test]
fn s4b_06_a_key_committed_by_somebody_else_is_a_duplicate_too() {
    let db = new_db();
    exec(&db, "CREATE TABLE maintable(a int primary key, b int)");
    let t2 = begin(&db);
    run(&db, &t2, "INSERT INTO maintable VALUES (2, 2)");
    commit(&db, &t2);
    let (t3, t4) = (begin(&db), begin(&db));
    run(&db, &t3, "INSERT INTO maintable VALUES (3, 3)");
    commit(&db, &t3);
    expect(&db, &t4, "SELECT * FROM maintable", &["2 2"]);
    run_tainted(&db, &t4, "INSERT INTO maintable VALUES (3, 4)");
    let t5 = begin(&db);
    run(&db, &t5, "INSERT INTO maintable VALUES (4, 4)");
    expect(&db, &t5, "SELECT * FROM maintable", &["2 2", "3 3", "4 4"]);
    expect(&db, &begin(&db), "SELECT * FROM maintable", &["2 2", "3 3"]);
}

#[test]
fn s4b_06_a_deleted_key_is_reused_without_a_second_tuple() {
    let db = new_db();
    ensure_index_scan(&db);
    exec(&db, "CREATE TABLE maintable(col1 int primary key, col2 int)");
    let info = table(&db, "maintable");
    let t1 = begin(&db);
    run(&db, &t1, "INSERT INTO maintable VALUES (1, 0), (2, 0), (3, 0), (4, 0)");
    commit(&db, &t1);
    let reverify = begin(&db);
    let t2 = begin(&db);
    run(&db, &t2, "DELETE FROM maintable");
    expect(&db, &t2, "SELECT * FROM maintable", &[]);
    commit(&db, &t2);
    let t3 = begin(&db);
    run(&db, &t3, "INSERT INTO maintable VALUES (2, 9)");
    expect(&db, &t3, "SELECT * FROM maintable", &["2 9"]);
    assert_eq!(heap_entries(&info), 4, "the tombstone of key 2 was reused");
    expect(&db, &reverify, "SELECT * FROM maintable", &["1 0", "2 0", "3 0", "4 0"]);
}

#[test]
fn s4b_06_index_scans_see_the_version_of_their_snapshot() {
    let db = new_db();
    ensure_index_scan(&db);
    exec(&db, "CREATE TABLE maintable(col1 int primary key, col2 int)");
    let t1 = begin(&db);
    run(&db, &t1, "INSERT INTO maintable VALUES (1, 0), (2, 0), (3, 0), (4, 0)");
    for k in 1..=4 {
        expect(&db, &t1, &format!("SELECT * FROM maintable WHERE col1 = {k}"), &[&format!("{k} 0")]);
    }
    commit(&db, &t1);
    let reverify = begin(&db);
    let t2 = begin(&db);
    run(&db, &t2, "DELETE FROM maintable");
    commit(&db, &t2);
    for k in 1..=4 {
        expect(&db, &reverify, &format!("SELECT * FROM maintable WHERE col1 = {k}"), &[&format!("{k} 0")]);
        expect(&db, &begin(&db), &format!("SELECT * FROM maintable WHERE col1 = {k}"), &[]);
    }
}

#[test]
fn s4b_06_aborting_an_insert_keeps_the_entry_and_the_next_insert_reuses_it() {
    let db = new_db();
    ensure_index_scan(&db);
    exec(&db, "CREATE TABLE maintable(a int primary key, b int)");
    let info = table(&db, "maintable");
    let t1 = begin(&db);
    run(&db, &t1, "INSERT INTO maintable VALUES (1, 233), (2, 2333)");
    abort(&db, &t1);
    let t2 = begin(&db);
    run(&db, &t2, "INSERT INTO maintable VALUES (1, 2333), (2, 23333), (3, 233)");
    expect(&db, &t2, "SELECT * FROM maintable", &["1 2333", "2 23333", "3 233"]);
    commit(&db, &t2);
    expect(&db, &begin(&db), "SELECT * FROM maintable", &["1 2333", "2 23333", "3 233"]);
    assert_eq!(heap_entries(&info), 3, "aborting an insert keeps the entry and the next insert reuses it");
}

#[test]
fn s4b_06_delete_then_insert_in_one_transaction_reuses_the_tuple() {
    let db = new_db();
    exec(&db, "CREATE TABLE maintable(a int primary key, b int)");
    let info = table(&db, "maintable");
    let t0 = begin(&db);
    run(&db, &t0, "INSERT INTO maintable VALUES (1, 1)");
    commit(&db, &t0);
    let reader = begin(&db);
    let t = begin(&db);
    run(&db, &t, "DELETE FROM maintable WHERE a = 1");
    run(&db, &t, "INSERT INTO maintable VALUES (1, 2)");
    expect(&db, &t, "SELECT * FROM maintable", &["1 2"]);
    assert_eq!(undo_log_columns(&t), 2, "one log, from the delete");
    commit(&db, &t);
    expect(&db, &reader, "SELECT * FROM maintable", &["1 1"]);
    expect(&db, &begin(&db), "SELECT * FROM maintable", &["1 2"]);
    assert_eq!(heap_entries(&info), 1, "delete then insert in one transaction reuses the tuple");
}

// ---- 4b-06: updating a primary key ---------------------------------------------------------------------------------------------------------

#[test]
fn s4b_06_updating_the_key_moves_the_row_to_the_new_key() {
    let db = new_db();
    ensure_index_scan(&db);
    exec(&db, "CREATE TABLE maintable(col1 int primary key, col2 int)");
    let t1 = begin(&db);
    run(&db, &t1, "INSERT INTO maintable VALUES (1, 0), (2, 0), (3, 0), (4, 0)");
    commit(&db, &t1);
    let t2 = begin(&db);
    run(&db, &t2, "UPDATE maintable SET col1 = col1 + 1");
    expect(&db, &t2, "SELECT * FROM maintable", &["2 0", "3 0", "4 0", "5 0"]);
    expect(&db, &t2, "SELECT * FROM maintable WHERE col1 = 1", &[]);
    expect(&db, &t2, "SELECT * FROM maintable WHERE col1 = 5", &["5 0"]);
    commit(&db, &t2);
}

#[test]
fn s4b_06_shifting_every_key_reuses_the_tombstones_it_just_made() {
    let db = new_db();
    exec(&db, "CREATE TABLE maintable(col1 int primary key, col2 int)");
    let info = table(&db, "maintable");
    let t1 = begin(&db);
    run(&db, &t1, "INSERT INTO maintable VALUES (1, 0), (2, 0), (3, 0), (4, 0)");
    commit(&db, &t1);
    let t2 = begin(&db);
    run(&db, &t2, "UPDATE maintable SET col1 = col1 + 1");
    commit(&db, &t2);
    assert_eq!(heap_entries(&info), 5, "four rows plus one new key");
    let t3 = begin(&db);
    run(&db, &t3, "UPDATE maintable SET col1 = col1 - 2");
    expect(&db, &t3, "SELECT * FROM maintable", &["0 0", "1 0", "2 0", "3 0"]);
    commit(&db, &t3);
    expect(&db, &begin(&db), "SELECT * FROM maintable", &["0 0", "1 0", "2 0", "3 0"]);
}

#[test]
fn s4b_06_older_snapshots_still_see_the_old_keys() {
    let db = new_db();
    ensure_index_scan(&db);
    exec(&db, "CREATE TABLE maintable(col1 int primary key, col2 int)");
    let t1 = begin(&db);
    run(&db, &t1, "INSERT INTO maintable VALUES (1, 0), (2, 0), (3, 0), (4, 0)");
    commit(&db, &t1);
    let old = begin(&db);
    let t2 = begin(&db);
    run(&db, &t2, "UPDATE maintable SET col1 = col1 + 1");
    commit(&db, &t2);
    expect(&db, &old, "SELECT * FROM maintable", &["1 0", "2 0", "3 0", "4 0"]);
    for k in 1..=4 {
        expect(&db, &old, &format!("SELECT * FROM maintable WHERE col1 = {k}"), &[&format!("{k} 0")]);
    }
    expect(&db, &old, "SELECT * FROM maintable WHERE col1 = 5", &[]);
}

#[test]
fn s4b_06_a_non_key_update_of_a_table_with_a_key_stays_in_place() {
    let db = new_db();
    exec(&db, "CREATE TABLE maintable(col1 int primary key, col2 int)");
    let info = table(&db, "maintable");
    let t1 = begin(&db);
    run(&db, &t1, "INSERT INTO maintable VALUES (1, 0), (2, 0)");
    commit(&db, &t1);
    let t2 = begin(&db);
    run(&db, &t2, "UPDATE maintable SET col2 = col2 + 10");
    assert_eq!(undo_log_num(&t2), 2, "one log per tuple");
    commit(&db, &t2);
    assert_eq!(heap_entries(&info), 2, "a non key update of a table with a key stays in place");
    expect(&db, &begin(&db), "SELECT * FROM maintable", &["1 10", "2 10"]);
}

#[test]
fn s4b_06_updating_a_key_onto_an_existing_live_key_fails() {
    let db = new_db();
    exec(&db, "CREATE TABLE maintable(col1 int primary key, col2 int)");
    let t1 = begin(&db);
    run(&db, &t1, "INSERT INTO maintable VALUES (1, 0), (2, 0)");
    commit(&db, &t1);
    let t2 = begin(&db);
    run_tainted(&db, &t2, "UPDATE maintable SET col1 = 2 WHERE col1 = 1");
}

// ---- 4b-07: serializable -------------------------------------------------------------------------------------------------------------------

#[test]
fn s4b_07_a_row_deleted_after_it_was_read_still_fails_the_reader() {
    // after the delete the row no longer matches anything; the version just before it did, and that is the one that counts
    let db = new_db();
    exec(&db, "CREATE TABLE s(k int, v int)");
    exec(&db, "CREATE TABLE other(x int)");
    let setup = begin_serializable(&db);
    run(&db, &setup, "INSERT INTO s VALUES (1, 10), (2, 20)");
    commit(&db, &setup);
    let (reader, deleter) = (begin_serializable(&db), begin_serializable(&db));
    expect(&db, &reader, "SELECT k FROM s WHERE v = 20", &["2"]);
    run(&db, &reader, "INSERT INTO other VALUES (1)");
    run(&db, &deleter, "DELETE FROM s WHERE k = 2");
    commit(&db, &deleter);
    assert!(!db.txn_manager.commit(&reader).unwrap(), "the reader saw the row that was deleted since");
    assert_eq!(reader.state(), TransactionState::Aborted, "a row deleted after it was read still fails the reader");
}

#[test]
fn s4b_07_a_serializable_transaction_whose_reads_changed_fails_to_commit() {
    let db = new_db();
    ensure_index_scan(&db);
    exec(&db, "CREATE TABLE maintable(a int, b int primary key)");
    let t1 = db.txn_manager.begin(IsolationLevel::Serializable).unwrap();
    run(&db, &t1, "INSERT INTO maintable VALUES (1, 100), (1, 101), (0, 102), (0, 103)");
    commit(&db, &t1);
    let (t2, t3, t_read) = (begin_serializable(&db), begin_serializable(&db), begin_serializable(&db));
    run(&db, &t2, "UPDATE maintable SET a = 0 WHERE a = 1");
    run(&db, &t3, "UPDATE maintable SET a = 1 WHERE a = 0");
    run(&db, &t_read, "SELECT * FROM maintable WHERE a = 0");
    commit(&db, &t2);
    assert!(!db.txn_manager.commit(&t3).unwrap(), "t3 scanned a = 0, which t2 changed");
    assert_eq!(t3.state(), TransactionState::Aborted, "a serializable transaction whose reads changed fails to commit");
    commit(&db, &t_read); // read-only: serialised at its read timestamp
}

#[test]
fn s4b_07_a_failed_validation_undoes_the_transactions_writes() {
    let db = new_db();
    exec(&db, "CREATE TABLE maintable(a int, b int)");
    let t1 = begin_serializable(&db);
    run(&db, &t1, "INSERT INTO maintable VALUES (1, 1), (0, 2)");
    commit(&db, &t1);
    let (t2, t3) = (begin_serializable(&db), begin_serializable(&db));
    run(&db, &t2, "UPDATE maintable SET a = 0 WHERE a = 1");
    run(&db, &t3, "UPDATE maintable SET a = 1 WHERE a = 0");
    commit(&db, &t2);
    assert!(!db.txn_manager.commit(&t3).unwrap(), "a failed validation undoes the transactions writes: expected `!db.txn_manager.commit(&t3).unwrap()`");
    expect(&db, &begin(&db), "SELECT a, b FROM maintable", &["0 1", "0 2"]);
}

#[test]
fn s4b_07_garbage_collection_keeps_what_a_running_serializable_transaction_validates_against() {
    // Write skew through inserts: each reads an empty predicate and inserts what the other read. t2 only inserted, so it owns no undo
    // log; a collection that forgot it would leave t1 nothing to validate against, and both would commit.
    let db = new_db();
    exec(&db, "CREATE TABLE t(v int)");
    let (t1, t2) = (begin_serializable(&db), begin_serializable(&db));
    expect(&db, &t1, "SELECT * FROM t WHERE v = 5", &[]);
    expect(&db, &t2, "SELECT * FROM t WHERE v = 6", &[]);
    run(&db, &t1, "INSERT INTO t VALUES (6)");
    run(&db, &t2, "INSERT INTO t VALUES (5)");
    commit(&db, &t2);
    db.txn_manager.garbage_collection();
    assert!(txn_exists(&db, &t2), "t2 committed after t1's read timestamp, and t1 (serializable) is still running: it must stay");
    assert!(!db.txn_manager.commit(&t1).unwrap(), "t1 read v = 5 and found nothing, t2 then inserted v = 5: t1 must fail validation, collection or not");
}

#[test]
fn s4b_07_transactions_that_read_different_things_both_commit() {
    let db = new_db();
    exec(&db, "CREATE TABLE maintable(a int, b int)");
    let t1 = begin_serializable(&db);
    run(&db, &t1, "INSERT INTO maintable VALUES (1, 1), (0, 2), (5, 3)");
    commit(&db, &t1);
    let (t2, t3) = (begin_serializable(&db), begin_serializable(&db));
    run(&db, &t2, "UPDATE maintable SET b = 10 WHERE a = 1");
    run(&db, &t3, "UPDATE maintable SET b = 20 WHERE a = 5");
    commit(&db, &t2);
    commit(&db, &t3);
}

#[test]
fn s4b_07_snapshot_isolation_does_not_validate() {
    let db = new_db();
    exec(&db, "CREATE TABLE maintable(a int, b int)");
    let t1 = begin(&db);
    run(&db, &t1, "INSERT INTO maintable VALUES (1, 1), (0, 2)");
    commit(&db, &t1);
    let (t2, t3) = (begin(&db), begin(&db));
    run(&db, &t2, "UPDATE maintable SET a = 0 WHERE a = 1");
    run(&db, &t3, "UPDATE maintable SET a = 1 WHERE a = 0");
    commit(&db, &t2);
    // different rows, no write-write conflict: write skew is allowed under snapshot isolation
    commit(&db, &t3);
}

#[test]
fn s4b_07_a_full_scan_conflicts_with_any_change_in_the_table() {
    let db = new_db();
    exec(&db, "CREATE TABLE maintable(a int, b int)");
    let t1 = begin_serializable(&db);
    run(&db, &t1, "INSERT INTO maintable VALUES (1, 1)");
    commit(&db, &t1);
    exec(&db, "CREATE TABLE other(a int)");
    let (t2, t3) = (begin_serializable(&db), begin_serializable(&db));
    run(&db, &t2, "SELECT * FROM maintable");
    run(&db, &t2, "INSERT INTO other VALUES (1)");
    run(&db, &t3, "INSERT INTO maintable VALUES (2, 2)");
    commit(&db, &t3);
    assert!(!db.txn_manager.commit(&t2).unwrap(), "t2 scanned everything, t3 added a row");
}

#[test]
fn s4b_07_of_two_concurrent_swaps_exactly_one_commits() {
    for _ in 0..10 {
        let db = new_db();
        exec(&db, "CREATE TABLE maintable(a int, b int primary key)");
        let t1 = begin_serializable(&db);
        let values: Vec<String> = (0..100).flat_map(|i| [format!("(1, {})", 1000 + i), format!("(0, {})", 2000 + i)]).collect();
        run(&db, &t1, &format!("INSERT INTO maintable VALUES {}", values.join(", ")));
        commit(&db, &t1);
        let (t2, t3) = (begin_serializable(&db), begin_serializable(&db));
        run(&db, &t3, "UPDATE maintable SET a = 1 WHERE a = 0");
        run(&db, &t2, "UPDATE maintable SET a = 0 WHERE a = 1");
        let db = Arc::new(db);
        let handles: Vec<_> = [t2, t3]
            .into_iter()
            .map(|t| {
                let db = db.clone();
                std::thread::spawn(move || db.txn_manager.commit(&t).unwrap())
            })
            .collect();
        let committed = handles.into_iter().map(|h| h.join().unwrap()).filter(|ok| *ok).count();
        assert_eq!(committed, 1, "of two concurrent swaps exactly one commits");
    }
}

// ---- 4b-08: BusTub's tests ------------------------------------------------------------------------------------------------------------------

#[test]
fn s4b_08_insert_delete_conflict_test() {
    let db = new_db();
    exec(&db, "CREATE TABLE maintable(a int)");
    let q = "SELECT a FROM maintable";
    let txn1 = begin(&db);
    for v in 1..=3 {
        run(&db, &txn1, &format!("INSERT INTO maintable VALUES ({v})"));
    }
    run(&db, &txn1, "DELETE FROM maintable WHERE a = 3");
    expect(&db, &txn1, q, &["1", "2"]);
    commit(&db, &txn1);
    let txn2 = begin(&db);
    expect(&db, &txn2, q, &["1", "2"]);
    run(&db, &txn2, "DELETE FROM maintable WHERE a = 2");
    expect(&db, &txn2, q, &["1"]);
    let txn3 = begin(&db);
    expect(&db, &txn3, q, &["1", "2"]);
    run_tainted(&db, &txn3, "DELETE FROM maintable WHERE a = 2");
    let txn4 = begin(&db);
    expect(&db, &txn4, q, &["1", "2"]);
    for v in 4..=6 {
        run(&db, &txn4, &format!("INSERT INTO maintable VALUES ({v})"));
    }
    run(&db, &txn4, "DELETE FROM maintable WHERE a = 6");
    expect(&db, &txn4, q, &["1", "2", "4", "5"]);
    expect(&db, &txn2, q, &["1"]);
    run(&db, &txn2, "DELETE FROM maintable WHERE a = 5");
    expect(&db, &txn2, q, &["1"]);
    commit(&db, &txn2);
    commit(&db, &txn4);
    let txn5 = begin(&db);
    expect(&db, &txn5, q, &["1", "4", "5"]);
    let txn6 = begin(&db);
    run(&db, &txn6, "DELETE FROM maintable WHERE a = 5");
    commit(&db, &txn6);
    run_tainted(&db, &txn5, "DELETE FROM maintable WHERE a = 5");
    let txn7 = begin(&db);
    expect(&db, &txn7, q, &["1", "4"]);
    run(&db, &txn7, "DELETE FROM maintable");
    expect(&db, &txn7, q, &[]);
    commit(&db, &txn7);
}

#[test]
fn s4b_08_garbage_collection() {
    let db = new_db();
    exec(&db, "CREATE TABLE table1(a int, b int, c int)");
    let q = "SELECT * FROM table1";
    let all0 = ["0 0 0", "1 1 1", "2 2 2", "3 3 3"];
    let all10 = ["10 0 0", "11 1 1", "12 2 2", "13 3 3"];
    let after3 = ["20 0 0", "12 2 2", "13 3 3"];
    let w0 = begin(&db);
    bump_commit_ts(&db, 2);
    let txn_a = begin(&db);
    run(&db, &txn_a, "INSERT INTO table1 VALUES (0, 0, 0), (1, 1, 1)");
    commit(&db, &txn_a);
    let txn_b = begin(&db);
    run(&db, &txn_b, "INSERT INTO table1 VALUES (2, 2, 2), (3, 3, 3)");
    commit(&db, &txn_b);
    bump_commit_ts(&db, 2);
    let w1 = begin(&db);
    bump_commit_ts(&db, 2);
    let txn2 = begin(&db);
    run(&db, &txn2, "UPDATE table1 SET a = a + 10");
    expect(&db, &txn2, q, &all10);
    commit(&db, &txn2);
    bump_commit_ts(&db, 2);
    let w2 = begin(&db);
    bump_commit_ts(&db, 2);
    let txn3 = begin(&db);
    run(&db, &txn3, "UPDATE table1 SET a = a + 10 WHERE a < 12");
    run(&db, &txn3, "DELETE FROM table1 WHERE a = 21");
    expect(&db, &txn3, q, &after3);
    commit(&db, &txn3);
    bump_commit_ts(&db, 2);
    let w3 = begin(&db);
    bump_commit_ts(&db, 2);

    expect(&db, &w0, q, &[]);
    expect(&db, &w1, q, &all0);
    expect(&db, &w2, q, &all10);
    expect(&db, &w3, q, &after3);

    // A, B: two collections with every watermark transaction still running
    db.txn_manager.garbage_collection();
    db.txn_manager.garbage_collection();
    for w in [&w0, &w1, &w2, &w3, &txn2, &txn3] {
        assert!(txn_exists(&db, w), "txn{} should exist", w.human_readable_id());
    }
    assert!(!txn_exists(&db, &txn_a) && !txn_exists(&db, &txn_b), "garbage collection: expected `!txn_exists(&db, &txn_a) && !txn_exists(&db, &txn_b)`");
    expect(&db, &w0, q, &[]);
    expect(&db, &w1, q, &all0);
    expect(&db, &w2, q, &all10);
    expect(&db, &w3, q, &after3);

    // C: the oldest reader finishes
    commit(&db, &w0);
    db.txn_manager.garbage_collection();
    assert!(!txn_exists(&db, &w0), "garbage collection: expected `!txn_exists(&db, &w0)`");
    for w in [&w1, &w2, &w3, &txn2, &txn3] {
        assert!(txn_exists(&db, w), "garbage collection: expected `txn_exists(&db, w)`");
    }
    expect(&db, &w1, q, &all0);
    expect(&db, &w2, q, &all10);
    expect(&db, &w3, q, &after3);

    // D: the next one; txn2's logs are older than anything w2 can ask for
    commit(&db, &w1);
    db.txn_manager.garbage_collection();
    assert!(!txn_exists(&db, &w1) && !txn_exists(&db, &txn2), "garbage collection: expected `!txn_exists(&db, &w1) && !txn_exists(&db, &txn2)`");
    assert!(txn_exists(&db, &w2) && txn_exists(&db, &w3) && txn_exists(&db, &txn3), "garbage collection: expected `txn_exists(&db, &w2) && txn_exists(&db, &w3) && txn_exists(&db, &txn3)`");
    expect(&db, &w2, q, &all10);
    expect(&db, &w3, q, &after3);

    // E
    commit(&db, &w2);
    db.txn_manager.garbage_collection();
    assert!(!txn_exists(&db, &w2) && !txn_exists(&db, &txn3), "garbage collection: expected `!txn_exists(&db, &w2) && !txn_exists(&db, &txn3)`");
    assert!(txn_exists(&db, &w3), "garbage collection: expected `txn_exists(&db, &w3)`");
    expect(&db, &w3, q, &after3);

    // F: nobody is left
    commit(&db, &w3);
    db.txn_manager.garbage_collection();
    for t in [&w0, &w1, &w2, &w3, &txn_a, &txn_b, &txn2, &txn3] {
        assert!(!txn_exists(&db, t), "txn{} should be collected", t.human_readable_id());
    }
}

#[test]
fn s4b_08_garbage_collection_with_tainted_transactions() {
    let db = new_db();
    exec(&db, "CREATE TABLE table1(a int, b int, c int)");
    let q = "SELECT * FROM table1";
    let all0 = ["0 0 0", "1 1 1", "2 2 2", "3 3 3"];
    let all10 = ["10 0 0", "11 1 1", "12 2 2", "13 3 3"];
    let after3 = ["20 0 0", "12 2 2", "13 3 3"];
    let w0 = begin(&db);
    bump_commit_ts(&db, 2);
    let txn_a = begin(&db);
    run(&db, &txn_a, "INSERT INTO table1 VALUES (0, 0, 0), (1, 1, 1)");
    commit(&db, &txn_a);
    let txn_b = begin(&db);
    run(&db, &txn_b, "INSERT INTO table1 VALUES (2, 2, 2), (3, 3, 3)");
    commit(&db, &txn_b);
    bump_commit_ts(&db, 2);
    let w1 = begin(&db);
    bump_commit_ts(&db, 2);
    let txn2 = begin(&db);
    run(&db, &txn2, "UPDATE table1 SET a = a + 10");
    commit(&db, &txn2);
    bump_commit_ts(&db, 2);
    let w2 = begin(&db);
    bump_commit_ts(&db, 2);
    let txn3 = begin(&db);
    run(&db, &txn3, "UPDATE table1 SET a = a + 10 WHERE a < 12");
    run(&db, &txn3, "DELETE FROM table1 WHERE a = 21");
    let (txn5, txn6) = (begin(&db), begin(&db));
    commit(&db, &txn3);
    bump_commit_ts(&db, 2);
    let w3 = begin(&db);
    bump_commit_ts(&db, 2);

    db.txn_manager.garbage_collection();
    db.txn_manager.garbage_collection();
    assert!(txn_exists(&db, &txn2) && txn_exists(&db, &txn3) && txn_exists(&db, &txn5), "garbage collection with tainted transactions: expected `txn_exists(&db, &txn2) && txn_exists(&db, &txn3) && txn_exists(&db, &txn5)`");
    assert!(!txn_exists(&db, &txn_a) && !txn_exists(&db, &txn_b), "garbage collection with tainted transactions: expected `!txn_exists(&db, &txn_a) && !txn_exists(&db, &txn_b)`");

    // C: txn5 and txn6 conflict with the committed deletes and become tainted; they keep the watermark low
    run(&db, &txn5, "DELETE FROM table1 WHERE a = 12");
    run_tainted(&db, &txn5, "DELETE FROM table1 WHERE a = 11");
    run_tainted(&db, &txn6, "DELETE FROM table1 WHERE a = 11");
    db.txn_manager.garbage_collection();
    for t in [&w0, &w1, &w2, &w3, &txn2, &txn3, &txn5, &txn6] {
        assert!(txn_exists(&db, t), "txn{} should exist", t.human_readable_id());
    }
    expect(&db, &w0, q, &[]);
    expect(&db, &w1, q, &all0);
    expect(&db, &w2, q, &all10);
    expect(&db, &w3, q, &after3);

    // D, E, F, G: the watermark transactions finish one by one; txn5 and txn6 (still tainted) hold txn3 and everything it needs
    commit(&db, &w0);
    db.txn_manager.garbage_collection();
    assert!(!txn_exists(&db, &w0) && txn_exists(&db, &txn2) && txn_exists(&db, &txn3), "garbage collection with tainted transactions: expected `!txn_exists(&db, &w0) && txn_exists(&db, &txn2) && txn_exists(&db, &txn3)`");
    commit(&db, &w1);
    db.txn_manager.garbage_collection();
    assert!(!txn_exists(&db, &w1) && !txn_exists(&db, &txn2), "garbage collection with tainted transactions: expected `!txn_exists(&db, &w1) && !txn_exists(&db, &txn2)`");
    expect(&db, &w2, q, &all10);
    commit(&db, &w2);
    db.txn_manager.garbage_collection();
    assert!(!txn_exists(&db, &w2) && txn_exists(&db, &txn3) && txn_exists(&db, &txn5) && txn_exists(&db, &txn6), "garbage collection with tainted transactions: expected `!txn_exists(&db, &w2) && txn_exists(&db, &txn3) && txn_exists(&db, &txn5) && txn_exists(&db, &txn6)`");
    commit(&db, &w3);
    db.txn_manager.garbage_collection();
    assert!(!txn_exists(&db, &w3), "garbage collection with tainted transactions: expected `!txn_exists(&db, &w3)`");
    assert!(txn_exists(&db, &txn3) && txn_exists(&db, &txn5) && txn_exists(&db, &txn6), "the tainted transactions still run");
}

#[test]
fn s4b_08_index_update_conflict_test() {
    let db = new_db();
    ensure_index_scan(&db);
    exec(&db, "CREATE TABLE maintable(col1 int primary key, col2 int)");
    let txn1 = begin(&db);
    run(&db, &txn1, "INSERT INTO maintable VALUES (1, 0), (2, 0), (3, 0)");
    run(&db, &txn1, "DELETE FROM maintable WHERE col1 = 2");
    commit(&db, &txn1);
    let (txn2, txn3) = (begin(&db), begin(&db));
    run(&db, &txn2, "INSERT INTO maintable VALUES (4, 0)");
    run(&db, &txn2, "DELETE FROM maintable WHERE col1 = 1");
    run(&db, &txn2, "DELETE FROM maintable WHERE col1 = 3");
    commit(&db, &txn2);
    run_tainted(&db, &txn3, "UPDATE maintable SET col2 = 2 WHERE col1 = 1");
}

#[test]
fn s4b_08_index_concurrent_insert_test() {
    for _ in 0..10 {
        let db = Arc::new(new_db());
        exec(&db, "CREATE TABLE maintable(a int primary key, b int)");
        let (threads, numbers) = (8usize, 40usize);
        let handles: Vec<_> = (0..threads)
            .map(|thread| {
                let db = db.clone();
                std::thread::spawn(move || {
                    let mut won = vec![];
                    for n in 0..numbers {
                        let txn = begin(&db);
                        let mut out = String::new();
                        if db.execute_sql_txn(&format!("INSERT INTO maintable VALUES ({n}, {thread})"), &mut SimpleStreamWriter::new(&mut out, true, " "), &txn).unwrap() {
                            assert!(db.txn_manager.commit(&txn).unwrap(), "cannot commit??");
                            won.push(true);
                        } else {
                            won.push(false);
                        }
                    }
                    won
                })
            })
            .collect();
        let results: Vec<Vec<bool>> = handles.into_iter().map(|h| h.join().unwrap()).collect();
        let mut expected = vec![];
        for n in 0..numbers {
            let winners: Vec<usize> = (0..threads).filter(|t| results[*t][n]).collect();
            assert_eq!(winners.len(), 1, "exactly one thread inserts {n}: {winners:?}");
            expected.push(format!("{n} {}", winners[0]));
        }
        let query = begin(&db);
        let mut want = expected;
        want.sort();
        assert_eq!(rows(&db, &query, "SELECT * FROM maintable"), want, "index concurrent insert test");
    }
}

#[test]
fn s4b_08_index_concurrent_update_test() {
    for trial in 0..10 {
        let db = Arc::new(new_db());
        ensure_index_scan(&db);
        exec(&db, "CREATE TABLE maintable(a int primary key, b int)");
        let (threads, numbers) = (8usize, 20usize);
        let values: Vec<String> = (0..numbers).map(|i| format!("({i}, 0)")).collect();
        exec(&db, &format!("INSERT INTO maintable VALUES {}", values.join(",")));
        let table_info = table(&db, "maintable");
        assert!(heap_entries(&table_info) <= numbers, "index concurrent update test: expected `heap_entries(&table_info) <= numbers`");
        let add_delete_insert = trial % 2 == 1;
        let handles: Vec<_> = (0..threads)
            .map(|thread| {
                let db = db.clone();
                std::thread::spawn(move || {
                    let mut won = vec![];
                    for i in 0..numbers {
                        let txn = begin(&db);
                        let mut out = String::new();
                        let mut w = SimpleStreamWriter::new(&mut out, true, " ");
                        if !db.execute_sql_txn(&format!("UPDATE maintable SET b = b + {} WHERE a = {i}", 1 << thread), &mut w, &txn).unwrap() {
                            won.push(false);
                            continue;
                        }
                        if add_delete_insert {
                            let mut out = String::new();
                            assert!(db.execute_sql_txn(&format!("SELECT b FROM maintable WHERE a = {i}"), &mut SimpleStreamWriter::new(&mut out, true, " "), &txn).unwrap(), "index concurrent update test: expected `db.execute_sql_txn(&format!(\"SELECT b FROM maintable WHERE a = {{i}}\"), &mut SimpleStreamWriter::new(&...`");
                            let b: i32 = out.trim().parse().expect("one row with b");
                            let mut sink = String::new();
                            assert!(db.execute_sql_txn(&format!("DELETE FROM maintable WHERE a = {i}"), &mut SimpleStreamWriter::new(&mut sink, true, " "), &txn).unwrap(), "index concurrent update test: expected `db.execute_sql_txn(&format!(\"DELETE FROM maintable WHERE a = {{i}}\"), &mut SimpleStreamWriter::new(&mu...`");
                            assert!(db.execute_sql_txn(&format!("INSERT INTO maintable VALUES ({i}, {b})"), &mut SimpleStreamWriter::new(&mut sink, true, " "), &txn).unwrap(), "index concurrent update test: expected `db.execute_sql_txn(&format!(\"INSERT INTO maintable VALUES ({{i}}, {{b}})\"), &mut SimpleStreamWriter::new...`");
                        }
                        assert!(db.txn_manager.commit(&txn).unwrap(), "cannot commit??");
                        won.push(true);
                    }
                    won
                })
            })
            .collect();
        let results: Vec<Vec<bool>> = handles.into_iter().map(|h| h.join().unwrap()).collect();
        let mut want: Vec<String> = (0..numbers).map(|i| format!("{i} {}", (0..threads).filter(|j| results[*j][i]).map(|j| 1 << j).sum::<i32>())).collect();
        want.sort();
        let query = begin(&db);
        assert_eq!(rows(&db, &query, "SELECT * FROM maintable"), want, "index concurrent update test");
        assert!(heap_entries(&table_info) <= numbers, "updates and delete + insert reuse their tuples");
    }
}

#[test]
fn s4b_08_index_concurrent_update_abort_test() {
    for _ in 0..5 {
        let db = Arc::new(new_db());
        ensure_index_scan(&db);
        exec(&db, "CREATE TABLE maintable(a int primary key, b int)");
        let (threads, numbers, operations) = (8usize, 5usize, 60usize);
        let values: Vec<String> = (0..numbers).map(|i| format!("({i}, 0)")).collect();
        exec(&db, &format!("INSERT INTO maintable VALUES {}", values.join(",")));
        let handles: Vec<_> = (0..threads)
            .map(|seed| {
                let db = db.clone();
                std::thread::spawn(move || {
                    let mut counts = vec![0i32; numbers];
                    let mut state = 0x9E37_79B9_7F4A_7C15u64 ^ (seed as u64 + 1).wrapping_mul(0xA24B_AED4_963E_E407);
                    let mut next = move || {
                        state ^= state << 13;
                        state ^= state >> 7;
                        state ^= state << 17;
                        state
                    };
                    for _ in 0..operations {
                        let (x, mut y) = ((next() % numbers as u64) as usize, (next() % numbers as u64) as usize);
                        while y == x {
                            y = (next() % numbers as u64) as usize;
                        }
                        let txn = begin(&db);
                        let mut sink = String::new();
                        let ok = [x, y].iter().all(|n| db.execute_sql_txn(&format!("UPDATE maintable SET b = b + 1 WHERE a = {n}"), &mut SimpleStreamWriter::new(&mut sink, true, " "), &txn).unwrap());
                        if !ok {
                            db.txn_manager.abort(&txn).unwrap();
                            std::thread::sleep(std::time::Duration::from_millis(1));
                            continue;
                        }
                        assert!(db.txn_manager.commit(&txn).unwrap(), "cannot commit??");
                        counts[x] += 1;
                        counts[y] += 1;
                    }
                    counts
                })
            })
            .collect();
        let results: Vec<Vec<i32>> = handles.into_iter().map(|h| h.join().unwrap()).collect();
        let mut want: Vec<String> = (0..numbers).map(|i| format!("{i} {}", results.iter().map(|c| c[i]).sum::<i32>())).collect();
        want.sort();
        let query = begin(&db);
        assert_eq!(rows(&db, &query, "SELECT * FROM maintable"), want, "every committed increment is there, no aborted one is");
        assert!(heap_entries(&table(&db, "maintable")) <= numbers, "index concurrent update abort test: expected `heap_entries(&table(&db, \"maintable\")) <= numbers`");
    }
}

#[test]
fn s4b_08_simple_abort_with_ordered_queries_and_a_duplicate_heavy_commit() {
    let db = new_db();
    exec(&db, "CREATE TABLE maintable(a int, b int)");
    let q = "SELECT a, b FROM maintable ORDER BY a, b";
    let txn2 = begin(&db);
    run(&db, &txn2, "INSERT INTO maintable VALUES (1,10), (2,200), (3,300)");
    commit(&db, &txn2);
    let txn3 = begin(&db);
    run(&db, &txn3, "UPDATE maintable SET b = 0 WHERE a >= 2");
    abort(&db, &txn3);
    let txn4 = begin(&db);
    run(&db, &txn4, "DELETE FROM maintable");
    abort(&db, &txn4);
    let txn5 = begin(&db);
    run(&db, &txn5, "INSERT INTO maintable VALUES (1,999), (1,1000)");
    run(&db, &txn5, "UPDATE maintable SET b = b + 1 WHERE a = 3");
    let (_, out) = try_run(&db, &txn5, q);
    assert_eq!(out.lines().map(|l| l.trim_end()).collect::<Vec<_>>(), vec!["1 10", "1 999", "1 1000", "2 200", "3 301"], "simple abort with ordered queries and a duplicate heavy commit");
    commit(&db, &txn5);
    let fin = begin(&db);
    let (_, out) = try_run(&db, &fin, q);
    assert_eq!(out.lines().map(|l| l.trim_end()).collect::<Vec<_>>(), vec!["1 10", "1 999", "1 1000", "2 200", "3 301"], "simple abort with ordered queries and a duplicate heavy commit");
}

// ---- properties: sessions of interleaved transactions against a model of versions --------------------------------------------------

#[derive(Clone, Debug)]
enum Step {
    Begin,
    Insert(usize),
    Update(usize, usize, i32),
    Delete(usize, usize),
    Commit(usize),
    Abort(usize),
    Collect,
}

#[derive(Clone, Copy, Debug, PartialEq)]
enum Phase {
    Running,
    Tainted,
    Done,
}

/// A row of the model: its key, the committed versions (commit timestamp, value; `None` is a delete) and the one uncommitted version.
#[derive(Clone, Debug)]
struct MRow {
    key: i32,
    committed: Vec<(i64, Option<i32>)>,
    pending: Option<(usize, Option<i32>)>,
}

impl MRow {
    /// What transaction number `t` (reading at `read_ts`) sees of the row: its own pending version, else the newest committed one at its timestamp.
    fn seen_by(&self, t: usize, read_ts: i64) -> Option<i32> {
        match self.pending {
            Some((owner, v)) if owner == t => v,
            _ => self.committed.iter().rev().find(|(ts, _)| *ts <= read_ts).and_then(|(_, v)| *v),
        }
    }

    /// Writing the row conflicts if somebody else has an uncommitted version of it or a version newer than the writer's snapshot is committed.
    fn conflicts_for(&self, t: usize, read_ts: i64) -> bool {
        match self.pending {
            Some((owner, _)) if owner == t => false,
            Some(_) => true,
            None => self.committed.last().is_some_and(|(ts, _)| *ts > read_ts),
        }
    }
}

fn step_strategy(aborts: bool, collects: bool) -> impl Strategy<Value = Step> {
    prop_oneof![
        3 => Just(Step::Begin),
        3 => any::<usize>().prop_map(Step::Insert),
        5 => (any::<usize>(), any::<usize>(), 1..4i32).prop_map(|(t, r, d)| Step::Update(t, r, d)),
        3 => (any::<usize>(), any::<usize>()).prop_map(|(t, r)| Step::Delete(t, r)),
        3 => any::<usize>().prop_map(Step::Commit),
        if aborts { 2 } else { 0 } => any::<usize>().prop_map(Step::Abort),
        if collects { 2 } else { 0 } => Just(Step::Collect),
    ]
}

/// Runs a session of interleaved snapshot-isolation transactions (inserts, updates that add a number, deletes, commits, aborts, garbage
/// collections) and checks, after every step, that every running transaction sees exactly what the model says.
fn run_session(steps: &[Step]) -> Result<(), TestCaseError> {
    let db = new_db();
    exec(&db, "CREATE TABLE t(k int, v int)");
    let (mut txns, mut phase): (Vec<Arc<Transaction>>, Vec<Phase>) = (vec![], vec![]);
    let (mut model, mut next_key): (Vec<MRow>, i32) = (vec![], 0);
    for (n, step) in steps.iter().enumerate() {
        let open: Vec<usize> = (0..txns.len()).filter(|i| phase[*i] == Phase::Running).collect();
        let live: Vec<usize> = (0..txns.len()).filter(|i| phase[*i] != Phase::Done).collect();
        match step {
            Step::Begin => {
                txns.push(begin(&db));
                phase.push(Phase::Running);
            }
            Step::Insert(t) if !open.is_empty() => {
                let ti = open[t % open.len()];
                let (k, v) = (next_key, next_key * 10);
                next_key += 1;
                run(&db, &txns[ti], &format!("INSERT INTO t VALUES ({k}, {v})"));
                model.push(MRow { key: k, committed: vec![], pending: Some((ti, Some(v))) });
            }
            Step::Update(t, r, delta) if !open.is_empty() && !model.is_empty() => {
                let (ti, ri) = (open[t % open.len()], r % model.len());
                let txn = &txns[ti];
                let Some(v) = model[ri].seen_by(ti, txn.read_ts()) else { continue };
                let sql = format!("UPDATE t SET v = v + {delta} WHERE k = {}", model[ri].key);
                if model[ri].conflicts_for(ti, txn.read_ts()) {
                    run_tainted(&db, txn, &sql);
                    phase[ti] = Phase::Tainted;
                } else {
                    run(&db, txn, &sql);
                    model[ri].pending = Some((ti, Some(v + delta)));
                }
            }
            Step::Delete(t, r) if !open.is_empty() && !model.is_empty() => {
                let (ti, ri) = (open[t % open.len()], r % model.len());
                let txn = &txns[ti];
                if model[ri].seen_by(ti, txn.read_ts()).is_none() {
                    continue;
                }
                let sql = format!("DELETE FROM t WHERE k = {}", model[ri].key);
                if model[ri].conflicts_for(ti, txn.read_ts()) {
                    run_tainted(&db, txn, &sql);
                    phase[ti] = Phase::Tainted;
                } else {
                    run(&db, txn, &sql);
                    model[ri].pending = Some((ti, None));
                }
            }
            Step::Commit(t) if !open.is_empty() => {
                let ti = open[t % open.len()];
                commit(&db, &txns[ti]);
                phase[ti] = Phase::Done;
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
            Step::Abort(t) if !live.is_empty() => {
                let ti = live[t % live.len()];
                abort(&db, &txns[ti]);
                phase[ti] = Phase::Done;
                for row in model.iter_mut() {
                    if row.pending.is_some_and(|(owner, _)| owner == ti) {
                        row.pending = None;
                    }
                }
            }
            Step::Collect => db.txn_manager.garbage_collection(),
            _ => continue,
        }
        for &ti in &open.iter().copied().chain(txns.len().checked_sub(1).filter(|_| matches!(step, Step::Begin))).collect::<Vec<_>>() {
            if phase[ti] != Phase::Running {
                continue;
            }
            let mut want: Vec<String> = model.iter().filter_map(|row| row.seen_by(ti, txns[ti].read_ts()).map(|v| format!("{} {v}", row.key))).collect();
            want.sort();
            prop_assert_eq!(rows(&db, &txns[ti], "SELECT k, v FROM t"), want, "transaction {} (read ts {}) after step {}: {:?}", ti, txns[ti].read_ts(), n, step);
        }
    }
    // a transaction that begins at the end sees every committed version at the latest timestamp
    let last = begin(&db);
    let mut want: Vec<String> = model.iter().filter_map(|row| row.committed.last().and_then(|(_, v)| *v).map(|v| format!("{} {v}", row.key))).collect();
    want.sort();
    prop_assert_eq!(rows(&db, &last, "SELECT k, v FROM t"), want, "a new transaction sees the latest committed state");
    Ok(())
}

proptest! {
    #![proptest_config(ProptestConfig { cases: 40, max_shrink_iters: 1500, failure_persistence: None, ..ProptestConfig::default() })]

    /// Interleaved transactions insert, update and delete rows and commit: each sees its own writes and its snapshot, and a writer that
    /// meets a newer or uncommitted version of a row is tainted (no lost updates).
    #[test]
    fn s4b_03_sessions_of_writers_match_a_model_of_versions(steps in prop::collection::vec(step_strategy(false, false), 1..40)) {
        run_session(&steps)?;
    }

    /// The same sessions with aborts: an aborted transaction leaves no trace, whatever it did, and the tuples it touched can be written
    /// again by others.
    #[test]
    fn s4b_04_sessions_with_aborts_match_the_model(steps in prop::collection::vec(step_strategy(true, false), 1..50)) {
        run_session(&steps)?;
    }

    /// And with garbage collection at random moments: collecting never changes what any transaction sees.
    #[test]
    fn s4b_05_sessions_with_garbage_collection_match_the_model(steps in prop::collection::vec(step_strategy(true, true), 1..60)) {
        run_session(&steps)?;
    }
}

proptest! {
    #![proptest_config(ProptestConfig { cases: 30, max_shrink_iters: 2000, failure_persistence: None, ..ProptestConfig::default() })]

    /// A long session with everything at once: writers, conflicts, aborts and garbage collections.
    #[test]
    fn s4b_08_a_long_session_with_every_feature_matches_the_model(steps in prop::collection::vec(step_strategy(true, true), 40..90)) {
        run_session(&steps)?;
    }
}

// ---- 4b-06 · the primary key against a map ---------------------------------------------------------------------------------------

#[derive(Clone, Debug)]
enum KeyOp {
    Insert(i32, i32),
    Delete(i32),
    SetValue(i32, i32),
    MoveKey(i32, i32),
}

fn key_op() -> impl Strategy<Value = KeyOp> {
    prop_oneof![
        4 => (0..6i32, 0..100i32).prop_map(|(k, v)| KeyOp::Insert(k, v)),
        2 => (0..6i32).prop_map(KeyOp::Delete),
        2 => (0..6i32, 0..100i32).prop_map(|(k, v)| KeyOp::SetValue(k, v)),
        2 => (0..6i32, 0..6i32).prop_map(|(a, b)| KeyOp::MoveKey(a, b)),
    ]
}

proptest! {
    #![proptest_config(ProptestConfig { cases: 40, max_shrink_iters: 1500, failure_persistence: None, ..ProptestConfig::default() })]

    /// Transactions one after another on a table with a primary key: a duplicate key fails and taints, deletes make the key free again
    /// (a tombstone is reused, never a second tuple), updating the key moves the row, and aborts leave the map unchanged. After every
    /// transaction the table, every point lookup and every snapshot taken before it equal the model, and the heap never holds more tuples
    /// than there are different keys.
    #[test]
    fn s4b_06_a_primary_key_table_behaves_like_a_map_and_reuses_its_tombstones(txns in prop::collection::vec((prop::collection::vec(key_op(), 1..6), any::<bool>()), 1..14)) {
        let db = new_db();
        exec(&db, "CREATE TABLE p(pk int primary key, v int)");
        exec(&db, "set force_optimizer_starter_rule=yes");
        let info = table(&db, "p");
        let mut committed: BTreeMap<i32, i32> = BTreeMap::new();
        let mut snapshots: Vec<(Arc<Transaction>, BTreeMap<i32, i32>)> = vec![];
        for (ops, commit_it) in &txns {
            let txn = begin(&db);
            let mut working = committed.clone();
            let mut failed = false;
            for op in ops {
                let (sql, ok) = match op {
                    KeyOp::Insert(k, v) => (format!("INSERT INTO p VALUES ({k}, {v})"), !working.contains_key(k)),
                    KeyOp::Delete(k) => (format!("DELETE FROM p WHERE pk = {k}"), true),
                    KeyOp::SetValue(k, v) => (format!("UPDATE p SET v = {v} WHERE pk = {k}"), true),
                    KeyOp::MoveKey(a, b) => (format!("UPDATE p SET pk = {b} WHERE pk = {a}"), a == b || !working.contains_key(a) || !working.contains_key(b)),
                };
                let (done, _) = try_run(&db, &txn, &sql);
                prop_assert_eq!(done, ok, "{} on {:?}", sql, working);
                if !ok {
                    prop_assert_eq!(txn.state(), TransactionState::Tainted, "a duplicate key taints");
                    failed = true;
                    break;
                }
                match op {
                    KeyOp::Insert(k, v) => { working.insert(*k, *v); }
                    KeyOp::Delete(k) => { working.remove(k); }
                    KeyOp::SetValue(k, v) => { if let Some(x) = working.get_mut(k) { *x = *v; } }
                    KeyOp::MoveKey(a, b) => { if a != b { if let Some(v) = working.remove(a) { working.insert(*b, v); } } }
                }
                let want: Vec<String> = working.iter().map(|(k, v)| format!("{k} {v}")).collect();
                prop_assert_eq!(rows(&db, &txn, "SELECT pk, v FROM p"), want, "inside the transaction after {}", sql);
            }
            if failed || !commit_it {
                abort(&db, &txn);
            } else {
                commit(&db, &txn);
                committed = working;
            }
            let fresh = begin(&db);
            let want: Vec<String> = committed.iter().map(|(k, v)| format!("{k} {v}")).collect();
            prop_assert_eq!(rows(&db, &fresh, "SELECT pk, v FROM p"), want);
            for k in 0..6 {
                let want: Vec<String> = committed.get(&k).map(|v| format!("{k} {v}")).into_iter().collect();
                prop_assert_eq!(rows(&db, &fresh, &format!("SELECT pk, v FROM p WHERE pk = {k}")), want, "point lookup of {}", k);
            }
            prop_assert!(heap_entries(&info) <= 6, "{} tuples for at most 6 different keys: a deleted key's tuple is reused", heap_entries(&info));
            snapshots.push((fresh, committed.clone()));
        }
        // every snapshot still reads the state it was taken in
        for (txn, state) in &snapshots {
            let want: Vec<String> = state.iter().map(|(k, v)| format!("{k} {v}")).collect();
            prop_assert_eq!(rows(&db, txn, "SELECT pk, v FROM p"), want, "snapshot at {}", txn.read_ts());
        }
    }
}

// ---- 4b-07 · serializable against every serial order ----------------------------------------------------------------------------------

/// A transaction program: read the value of key `read`, then write `value read + add` to key `write`, then commit.
#[derive(Clone, Copy, Debug)]
struct Prog {
    read: i32,
    write: i32,
    add: i32,
}

fn prog() -> impl Strategy<Value = Prog> {
    (0..3i32, 0..3i32, 1..4i32).prop_map(|(read, write, add)| Prog { read, write, add })
}

fn serial_ok(initial: &BTreeMap<i32, i32>, order: &[(Prog, i32)], last: &BTreeMap<i32, i32>) -> bool {
    let mut state = initial.clone();
    for (p, seen) in order {
        if state[&p.read] != *seen {
            return false;
        }
        state.insert(p.write, seen + p.add);
    }
    state == *last
}

fn permutations(items: &[(Prog, i32)]) -> Vec<Vec<(Prog, i32)>> {
    if items.len() <= 1 {
        return vec![items.to_vec()];
    }
    let mut out = vec![];
    for i in 0..items.len() {
        let mut rest = items.to_vec();
        let first = rest.remove(i);
        for mut p in permutations(&rest) {
            p.insert(0, first);
            out.push(p);
        }
    }
    out
}

proptest! {
    #![proptest_config(ProptestConfig { cases: 60, max_shrink_iters: 1500, failure_persistence: None, ..ProptestConfig::default() })]

    /// Serializable transactions that read a key and write another, interleaved at random: whichever ones commit (the others fail on a
    /// write-write conflict or at validation) read what they read and leave the table in the state of **some serial order** of just
    /// those transactions. The oracle tries every order.
    #[test]
    fn s4b_07_committed_serializable_transactions_are_equivalent_to_a_serial_order(progs in prop::collection::vec(prog(), 2..5), schedule in prop::collection::vec(any::<usize>(), 0..16)) {
        let db = new_db();
        exec(&db, "CREATE TABLE s(k int, v int)");
        exec(&db, "INSERT INTO s VALUES (0, 10), (1, 20), (2, 30)");
        let initial: BTreeMap<i32, i32> = BTreeMap::from([(0, 10), (1, 20), (2, 30)]);
        let txns: Vec<Arc<Transaction>> = progs.iter().map(|_| begin_serializable(&db)).collect();
        let mut next_step = vec![0usize; progs.len()];
        let mut seen: Vec<Option<i32>> = vec![None; progs.len()];
        let mut alive = vec![true; progs.len()];
        let mut committed: Vec<usize> = vec![];
        let order: Vec<usize> = schedule.iter().map(|i| i % progs.len()).chain((0..progs.len()).cycle().take(progs.len() * 3)).collect();
        for i in order {
            if !alive[i] || next_step[i] >= 3 {
                continue;
            }
            let (p, txn) = (progs[i], &txns[i]);
            match next_step[i] {
                0 => {
                    let r = rows(&db, txn, &format!("SELECT v FROM s WHERE k = {}", p.read));
                    seen[i] = Some(r[0].trim().parse().unwrap());
                }
                1 => {
                    let (ok, _) = try_run(&db, txn, &format!("UPDATE s SET v = {} WHERE k = {}", seen[i].unwrap() + p.add, p.write));
                    if !ok {
                        abort(&db, txn);
                        alive[i] = false;
                    }
                }
                _ => {
                    if db.txn_manager.commit(txn).unwrap() {
                        committed.push(i);
                    } else {
                        // a failed validation aborts the transaction itself; a tainted one is aborted by the caller
                        if txn.state() != TransactionState::Aborted { abort(&db, txn); }
                    }
                    alive[i] = false;
                }
            }
            next_step[i] += 1;
        }
        let fresh = begin(&db);
        let last: BTreeMap<i32, i32> = rows(&db, &fresh, "SELECT k, v FROM s").iter().map(|l| { let mut it = l.split(' ').map(|x| x.parse::<i32>().unwrap()); (it.next().unwrap(), it.next().unwrap()) }).collect();
        let done: Vec<(Prog, i32)> = committed.iter().map(|i| (progs[*i], seen[*i].unwrap())).collect();
        prop_assert!(
            permutations(&done).iter().any(|order| serial_ok(&initial, order, &last)),
            "committed {:?} left {:?}, which no serial order of them gives",
            done, last
        );
    }
}

// @@ challenge 4b-c1 begin
mod ch_4b_c1 {
    use proptest::prelude::*;

    use bustub::concurrency::write_claims::{Conflict, WriteClaims};
    use std::collections::HashMap;

    #[test]
    fn s4b_c1_the_first_claimant_wins_and_the_second_is_told_who() {
        let mut c = WriteClaims::new();
        assert_eq!(c.claim(1, 10), Ok(()));
        assert_eq!(c.claim(2, 10), Err(Conflict { owner: 1 }));
        assert_eq!(c.owner_of(10), Some(1));
    }

    #[test]
    fn s4b_c1_claiming_your_own_key_again_is_fine() {
        let mut c = WriteClaims::new();
        c.claim(1, 10).unwrap();
        assert_eq!(c.claim(1, 10), Ok(()));
        assert_eq!(c.release_all(1), 1);
    }

    #[test]
    fn s4b_c1_release_frees_everything_the_transaction_held_and_nothing_else() {
        let mut c = WriteClaims::new();
        for k in [1, 2, 3] {
            c.claim(1, k).unwrap();
        }
        c.claim(2, 4).unwrap();
        assert_eq!(c.release_all(1), 3);
        assert_eq!((c.owner_of(1), c.owner_of(4)), (None, Some(2)));
        assert_eq!(c.claim(2, 1), Ok(()), "a released key can be claimed by another");
        assert_eq!(c.release_all(9), 0);
    }

    #[test]
    fn s4b_c1_a_conflict_changes_neither_the_owner_nor_the_losers_claims() {
        let mut c = WriteClaims::new();
        c.claim(1, 10).unwrap();
        assert!(c.claim(2, 10).is_err());
        assert_eq!(c.owner_of(10), Some(1));
        assert_eq!(c.release_all(2), 0, "the loser holds nothing");
        assert_eq!(c.release_all(1), 1);
        assert_eq!(c.owner_of(10), None);
    }

    proptest! {
        #![proptest_config(ProptestConfig { cases: 256, failure_persistence: None, ..ProptestConfig::default() })]

        /// Property: against a map from key to owner.
        #[test]
        fn s4b_c1_property_claims_match_an_owner_map(ops in proptest::collection::vec((any::<bool>(), 1u64..4, 0i64..5), 0..40)) {
            let mut c = WriteClaims::new();
            let mut m: HashMap<i64, u64> = HashMap::new();
            for (claim, txn, key) in ops {
                if claim {
                    let want = match m.get(&key) { Some(&o) if o != txn => Err(Conflict { owner: o }), _ => { m.insert(key, txn); Ok(()) } };
                    prop_assert_eq!(c.claim(txn, key), want);
                } else {
                    let n = m.values().filter(|&&o| o == txn).count();
                    m.retain(|_, o| *o != txn);
                    prop_assert_eq!(c.release_all(txn), n);
                }
                for k in 0..5 { prop_assert_eq!(c.owner_of(k), m.get(&k).copied()); }
            }
        }
    }
}
// @@ challenge 4b-c1 end

// @@ challenge 4b-c2 begin
mod ch_4b_c2 {
    use proptest::prelude::*;

    use bustub::concurrency::si_checker::{check_si, Txn, Violation::*};

    fn t(id: u32, start: u64, commit: u64, reads: &[(i64, i64)], writes: &[(i64, i64)]) -> Txn {
        Txn { id, start, commit, reads: reads.to_vec(), writes: writes.to_vec() }
    }

    #[test]
    fn s4b_c2_a_reader_that_started_before_a_commit_does_not_see_it() {
        let h = [t(1, 1, 3, &[], &[(0, 1)]), t(2, 2, 4, &[(0, 0)], &[])];
        assert_eq!(check_si(&h), Ok(()));
    }

    #[test]
    fn s4b_c2_a_reader_that_started_after_a_commit_must_see_it() {
        let h = [t(1, 1, 2, &[], &[(0, 1)]), t(2, 3, 4, &[(0, 1)], &[])];
        assert_eq!(check_si(&h), Ok(()));
        let stale = [t(1, 1, 2, &[], &[(0, 1)]), t(2, 3, 4, &[(0, 0)], &[])];
        assert_eq!(check_si(&stale), Err(StaleRead { txn: 2, key: 0 }));
    }

    #[test]
    fn s4b_c2_reading_a_value_that_was_not_yet_committed_is_a_violation() {
        let h = [t(1, 1, 4, &[], &[(0, 1)]), t(2, 2, 5, &[(0, 1)], &[])];
        assert_eq!(check_si(&h), Err(StaleRead { txn: 2, key: 0 }));
    }

    #[test]
    fn s4b_c2_overlapping_writers_of_one_key_are_a_lost_update() {
        let h = [t(1, 1, 4, &[(0, 0)], &[(0, 5)]), t(2, 2, 5, &[(0, 0)], &[(0, 7)])];
        assert_eq!(check_si(&h), Err(LostUpdate { a: 1, b: 2, key: 0 }));
    }

    #[test]
    fn s4b_c2_writers_of_one_key_that_do_not_overlap_are_fine() {
        let h = [t(1, 1, 2, &[(0, 0)], &[(0, 5)]), t(2, 3, 4, &[(0, 5)], &[(0, 7)])];
        assert_eq!(check_si(&h), Ok(()));
    }

    #[test]
    fn s4b_c2_write_skew_is_allowed_by_snapshot_isolation() {
        // both read x and y = 0, then write different keys: legal under SI, an anomaly under serializability
        let h = [t(1, 1, 3, &[(0, 0), (1, 0)], &[(0, 1)]), t(2, 2, 4, &[(0, 0), (1, 0)], &[(1, 1)])];
        assert_eq!(check_si(&h), Ok(()));
    }

    proptest! {
        #![proptest_config(ProptestConfig { cases: 128, failure_persistence: None, ..ProptestConfig::default() })]

        /// Property: histories produced by a model SI engine are valid; changing one read makes them invalid.
        #[test]
        fn s4b_c2_property_engine_histories_are_valid_and_corrupted_ones_are_not(plan in proptest::collection::vec((0u64..3, 0i64..3, any::<bool>(), 1i64..9), 1..8)) {
            // run transactions one after another but let each start a little before the previous commit (overlap by `gap`)
            let mut store: std::collections::HashMap<i64, Vec<(u64, i64)>> = Default::default();
            let mut hist: Vec<Txn> = Vec::new();
            let mut clock = 10u64;
            let mut claimed: Vec<(u64, u64, i64)> = Vec::new(); // (start, commit, key) of committed writers
            for (id, (gap, key, writes, val)) in plan.into_iter().enumerate() {
                let start = clock.saturating_sub(gap).max(1);
                let commit = clock + 5;
                // first committer wins: skip a writer that overlaps an earlier committed writer of the same key
                let do_write = writes && !claimed.iter().any(|&(_, c, k)| k == key && c > start);
                let snapshot = store.get(&key).and_then(|v| v.iter().rev().find(|&&(c, _)| c <= start).map(|&(_, x)| x)).unwrap_or(0);
                let t = Txn { id: id as u32, start, commit, reads: vec![(key, snapshot)], writes: if do_write { vec![(key, val)] } else { vec![] } };
                if do_write { store.entry(key).or_default().push((commit, val)); claimed.push((start, commit, key)); }
                hist.push(t);
                clock += 10;
            }
            prop_assert_eq!(check_si(&hist), Ok(()));
            let mut bad = hist.clone();
            let last = bad.len() - 1;
            let original = bad[last].reads[0].1;
            bad[last].reads[0].1 = original + 100;
            prop_assert!(check_si(&bad).is_err());
        }
    }
}
// @@ challenge 4b-c2 end

// @@ challenge 4b-c3 begin
mod ch_4b_c3 {
    use proptest::prelude::*;

    use bustub::concurrency::version_gc::{gc_chain, read_at};

    #[test]
    fn s4b_c3_the_newest_version_at_or_below_the_watermark_stays() {
        let mut c = vec![(9, 30), (5, 20), (2, 10)];
        assert_eq!(gc_chain(&mut c, 6), 1);
        assert_eq!(c, vec![(9, 30), (5, 20)]);
        assert_eq!(read_at(&c, 6), Some(20), "a reader at the watermark still reads version 5");
    }

    #[test]
    fn s4b_c3_a_watermark_exactly_on_a_version_keeps_that_version() {
        let mut c = vec![(9, 30), (5, 20), (2, 10)];
        gc_chain(&mut c, 5);
        assert_eq!(c, vec![(9, 30), (5, 20)]);
    }

    #[test]
    fn s4b_c3_a_watermark_below_everything_removes_nothing() {
        let mut c = vec![(9, 30), (5, 20)];
        assert_eq!(gc_chain(&mut c, 1), 0);
        assert_eq!(c.len(), 2);
    }

    #[test]
    fn s4b_c3_a_watermark_above_everything_keeps_only_the_newest() {
        let mut c = vec![(9, 30), (5, 20), (2, 10)];
        assert_eq!(gc_chain(&mut c, 100), 2);
        assert_eq!(c, vec![(9, 30)]);
    }

    proptest! {
        #![proptest_config(ProptestConfig { cases: 256, failure_persistence: None, ..ProptestConfig::default() })]

        /// Property: every read at or above the watermark is unchanged, and collecting again removes nothing.
        #[test]
        fn s4b_c3_property_readers_at_or_above_the_watermark_are_unaffected(stamps in proptest::collection::btree_set(1u64..30, 0..8), watermark in 0u64..32) {
            let mut chain: Vec<(u64, i64)> = stamps.iter().rev().map(|&t| (t, t as i64 * 10)).collect();
            let before = chain.clone();
            gc_chain(&mut chain, watermark);
            for t in watermark..40 {
                prop_assert_eq!(read_at(&chain, t), read_at(&before, t), "read at {}", t);
            }
            prop_assert!(chain.windows(2).all(|w| w[0].0 > w[1].0));
            prop_assert_eq!(gc_chain(&mut chain, watermark), 0);
            if !before.is_empty() { prop_assert!(!chain.is_empty()); }
        }
    }
}
// @@ challenge 4b-c3 end

// @@ challenge 4b-c4 begin
mod ch_4b_c4 {
    use proptest::prelude::*;

    use bustub::concurrency::predicate_validation::{validate, Pred, Write};

    fn w(old: Option<&[i64]>, new: Option<&[i64]>) -> Write {
        Write { old: old.map(|r| r.to_vec()), new: new.map(|r| r.to_vec()) }
    }

    #[test]
    fn s4b_c4_an_insert_into_the_range_is_a_phantom() {
        assert!(!validate(&[Pred::Range(0, 10, 20)], &[w(None, Some(&[15]))]));
        assert!(validate(&[Pred::Range(0, 10, 20)], &[w(None, Some(&[25]))]));
    }

    #[test]
    fn s4b_c4_a_delete_from_the_range_matters_too() {
        assert!(!validate(&[Pred::Range(0, 10, 20)], &[w(Some(&[15]), None)]));
    }

    #[test]
    fn s4b_c4_an_update_matters_when_either_image_is_in_the_range() {
        let p = [Pred::Range(0, 10, 20)];
        assert!(validate(&p, &[w(Some(&[5]), Some(&[8]))]), "outside to outside");
        assert!(!validate(&p, &[w(Some(&[15]), Some(&[30]))]), "moved out of the range");
        assert!(!validate(&p, &[w(Some(&[5]), Some(&[12]))]), "moved into the range");
    }

    #[test]
    fn s4b_c4_equality_predicates_and_other_columns() {
        assert!(!validate(&[Pred::Eq(1, 7)], &[w(None, Some(&[0, 7]))]));
        assert!(validate(&[Pred::Eq(1, 7)], &[w(None, Some(&[7, 0]))]), "the value is in another column");
    }

    #[test]
    fn s4b_c4_nothing_to_check_is_valid() {
        assert!(validate(&[], &[w(None, Some(&[1]))]));
        assert!(validate(&[Pred::Eq(0, 1)], &[]));
        assert!(validate(&[Pred::Eq(0, 1)], &[w(None, None)]));
    }

    proptest! {
        #![proptest_config(ProptestConfig { cases: 256, failure_persistence: None, ..ProptestConfig::default() })]

        /// Property: monotone in the predicates and the writes, and equal to a brute-force check.
        #[test]
        fn s4b_c4_property_conflicts_are_monotone(preds in proptest::collection::vec((0usize..2, 0i64..6, 0i64..6), 0..4), writes in proptest::collection::vec((proptest::option::of((0i64..8, 0i64..8)), proptest::option::of((0i64..8, 0i64..8))), 0..5)) {
            let ps: Vec<Pred> = preds.iter().map(|&(c, a, b)| if a == b { Pred::Eq(c, a) } else { Pred::Range(c, a.min(b), a.max(b)) }).collect();
            let ws: Vec<Write> = writes.iter().map(|(o, n)| Write { old: o.map(|(a, b)| vec![a, b]), new: n.map(|(a, b)| vec![a, b]) }).collect();
            let brute = ws.iter().all(|w| [&w.old, &w.new].into_iter().flatten().all(|r| ps.iter().all(|p| match p { Pred::Eq(c, v) => r[*c] != *v, Pred::Range(c, lo, hi) => !(*lo <= r[*c] && r[*c] <= *hi) })));
            prop_assert_eq!(validate(&ps, &ws), brute);
            if !validate(&ps[..ps.len().saturating_sub(1)], &ws) { prop_assert!(!validate(&ps, &ws)); }
        }
    }
}
// @@ challenge 4b-c4 end

// @@ challenge 4b-c5 begin
mod ch_4b_c5 {
    use proptest::prelude::*;

    use bustub::concurrency::savepoints::{NoSuchSavepoint, SavepointTxn};
    use std::collections::BTreeMap;

    #[test]
    fn s4b_c5_rolling_back_to_a_savepoint_undoes_what_came_after_it() {
        let mut t = SavepointTxn::begin(BTreeMap::new());
        t.set(1, 1);
        let a = t.savepoint();
        t.set(1, 2);
        t.set(2, 9);
        t.delete(1);
        assert_eq!(t.rollback_to(a), Ok(()));
        assert_eq!((t.get(1), t.get(2)), (Some(1), None));
        assert_eq!(t.commit(), BTreeMap::from([(1, 1)]));
    }

    #[test]
    fn s4b_c5_a_later_savepoint_disappears_when_an_earlier_one_is_rolled_back_to() {
        let mut t = SavepointTxn::begin(BTreeMap::new());
        let a = t.savepoint();
        t.set(1, 1);
        let b = t.savepoint();
        t.set(2, 2);
        t.rollback_to(a).unwrap();
        assert_eq!(t.rollback_to(b), Err(NoSuchSavepoint));
        assert_eq!(t.rollback_to(a), Ok(()), "the savepoint itself can be used again");
        assert_eq!(t.commit(), BTreeMap::new());
    }

    #[test]
    fn s4b_c5_nested_savepoints_roll_back_innermost_first() {
        let mut t = SavepointTxn::begin(BTreeMap::from([(1, 10)]));
        let a = t.savepoint();
        t.set(1, 20);
        let b = t.savepoint();
        t.set(1, 30);
        t.rollback_to(b).unwrap();
        assert_eq!(t.get(1), Some(20));
        t.rollback_to(a).unwrap();
        assert_eq!(t.get(1), Some(10));
    }

    #[test]
    fn s4b_c5_release_keeps_the_changes_but_forgets_the_marks() {
        let mut t = SavepointTxn::begin(BTreeMap::new());
        let a = t.savepoint();
        t.set(1, 1);
        let b = t.savepoint();
        t.set(2, 2);
        assert_eq!(t.release(a), Ok(()));
        assert_eq!(t.rollback_to(b), Err(NoSuchSavepoint), "releasing a savepoint releases the later ones too");
        assert_eq!(t.release(99), Err(NoSuchSavepoint));
        assert_eq!(t.commit(), BTreeMap::from([(1, 1), (2, 2)]));
    }

    #[test]
    fn s4b_c5_rolling_back_everything_restores_the_initial_state() {
        let init = BTreeMap::from([(1, 1), (2, 2)]);
        let mut t = SavepointTxn::begin(init.clone());
        t.set(1, 5);
        t.delete(2);
        t.set(3, 3);
        t.rollback_all();
        assert_eq!(t.commit(), init);
    }

    proptest! {
        #![proptest_config(ProptestConfig { cases: 256, failure_persistence: None, ..ProptestConfig::default() })]

        /// Property: rolling back to a savepoint gives exactly the map as it was when the savepoint was taken.
        #[test]
        fn s4b_c5_property_rollback_equals_a_snapshot(ops in proptest::collection::vec((0u8..4, 0i64..4, 0i64..9), 0..40)) {
            let mut t = SavepointTxn::begin(BTreeMap::new());
            let mut model: BTreeMap<i64, i64> = BTreeMap::new();
            let mut snaps: Vec<(u64, BTreeMap<i64, i64>)> = Vec::new();
            for (op, k, v) in ops {
                match op {
                    0 => { t.set(k, v); model.insert(k, v); }
                    1 => { t.delete(k); model.remove(&k); }
                    2 => { let id = t.savepoint(); snaps.push((id, model.clone())); }
                    _ => {
                        if let Some(pos) = snaps.len().checked_sub(1).map(|n| (v as usize) % (n + 1)).filter(|&p| p < snaps.len()) {
                            let (id, snap) = snaps[pos].clone();
                            prop_assert_eq!(t.rollback_to(id), Ok(()));
                            model = snap;
                            snaps.truncate(pos + 1);
                        }
                    }
                }
                for key in 0..4 { prop_assert_eq!(t.get(key), model.get(&key).copied()); }
            }
            prop_assert_eq!(t.commit(), model);
        }
    }
}
// @@ challenge 4b-c5 end
