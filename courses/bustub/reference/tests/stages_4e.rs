//! Tests for module 4e: SQL transactions: sessions, BEGIN, COMMIT, ROLLBACK, isolation levels and failed transactions.

use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use std::thread;
use std::time::Duration;

use proptest::prelude::*;

use bustub::common::bustub_instance::BusTubInstance;
use bustub::common::db_error::DbError;
use bustub::common::session::Session;
use bustub::concurrency::transaction::IsolationLevel;

fn exec(s: &mut Session<'_>, sql: &str) -> Vec<String> {
    s.execute(sql).unwrap_or_else(|e| panic!("{sql}: {e:?}"))
}

fn fail(s: &mut Session<'_>, sql: &str) -> DbError {
    match s.execute(sql) {
        Ok(r) => panic!("{sql}: expected an error, got {r:?}"),
        Err(e) => e,
    }
}

/// An instance with `t(id, v)` holding (1, 10), (2, 20), (3, 30).
fn db() -> BusTubInstance {
    let db = BusTubInstance::new(64);
    {
        let mut s = Session::new(&db);
        exec(&mut s, "create table t(id int, v int)");
        exec(&mut s, "insert into t values (1, 10), (2, 20), (3, 30)");
    }
    db
}

/// Nothing holds the garbage collector back: no transaction is registered as running.
fn quiet(db: &BusTubInstance) -> bool {
    db.txn_manager.get_watermark() == db.txn_manager.last_commit_ts()
}

// ---- 4e-01 · autocommit -----------------------------------------------------------------------------------------------------

#[test]
fn s4e_01_a_statement_outside_a_transaction_is_visible_to_other_sessions_at_once() {
    let db = db();
    let (mut a, mut b) = (Session::new(&db), Session::new(&db));
    assert_eq!(exec(&mut a, "update t set v = 11 where id = 1"), ["1"], "the update reports its row count");
    assert_eq!(exec(&mut b, "select v from t where id = 1"), ["11"], "another session sees the committed change at once");
    assert!(quiet(&db), "autocommit leaves no transaction running");
}

#[test]
fn s4e_01_several_statements_in_one_text_run_in_order_and_each_returns_its_rows() {
    let db = db();
    let mut a = Session::new(&db);
    assert_eq!(
        exec(&mut a, "insert into t values (4, 40); select id from t where id > 2 order by id;"),
        ["1", "3", "4"],
        "the insert's count, then the select's rows"
    );
}

#[test]
fn s4e_01_a_statement_that_fails_halfway_changes_nothing() {
    let db = db();
    let mut a = Session::new(&db);
    // id = 1 is updated first; id = 2 divides by zero
    assert_eq!(fail(&mut a, "update t set v = v / (id - 2)").code(), "22012", "the error comes out as it is: a division by zero");
    assert_eq!(
        exec(&mut a, "select id, v from t order by id"),
        ["1 10", "2 20", "3 30"],
        "no row was changed, not even the ones before the error"
    );
    assert!(quiet(&db), "and the transaction that failed was not left running");
}

#[test]
fn s4e_01_the_session_keeps_working_after_an_error() {
    let db = db();
    let mut a = Session::new(&db);
    assert!(a.execute("select * from nope").is_err(), "an unknown table is an error");
    assert!(quiet(&db), "the failed statement left nothing running");
    assert_eq!(exec(&mut a, "select count(*) from t"), ["3"], "the next statement works");
}

#[test]
fn s4e_01_an_error_stops_the_text_but_keeps_what_ran_before_it() {
    let db = db();
    let mut a = Session::new(&db);
    assert!(
        a.execute("update t set v = 0 where id = 1; select 1 / 0; update t set v = 0 where id = 3").is_err(),
        "the second statement fails"
    );
    assert_eq!(
        exec(&mut a, "select id, v from t order by id"),
        ["1 0", "2 20", "3 30"],
        "the first statement committed on its own, the third never ran"
    );
}

#[test]
fn s4e_01_every_autocommitted_statement_leaves_nothing_running() {
    let db = db();
    let mut a = Session::new(&db);
    for sql in ["select * from t", "update t set v = v + 1", "delete from t where id = 3", "insert into t values (9, 90)"] {
        exec(&mut a, sql);
        assert!(quiet(&db), "after {sql}: no transaction is registered as running (the watermark equals the last commit)");
    }
}

// ---- 4e-02 · BEGIN, COMMIT, ROLLBACK ----------------------------------------------------------------------------------------

#[test]
fn s4e_02_changes_are_invisible_to_others_until_commit() {
    let db = db();
    let (mut a, mut b) = (Session::new(&db), Session::new(&db));
    assert_eq!(exec(&mut a, "begin"), ["BEGIN"], "BEGIN is answered with its tag");
    assert!(a.in_transaction(), "a transaction is open");
    exec(&mut a, "update t set v = 99 where id = 1");
    assert_eq!(exec(&mut a, "select v from t where id = 1"), ["99"], "a transaction sees its own writes");
    assert_eq!(exec(&mut b, "select v from t where id = 1"), ["10"], "another session does not");
    assert_eq!(exec(&mut a, "commit"), ["COMMIT"], "COMMIT is answered with its tag");
    assert!(!a.in_transaction(), "and the transaction is over");
    assert_eq!(exec(&mut b, "select v from t where id = 1"), ["99"], "now it is visible");
}

#[test]
fn s4e_02_rollback_undoes_inserts_updates_and_deletes() {
    let db = db();
    let mut a = Session::new(&db);
    exec(&mut a, "begin");
    exec(&mut a, "insert into t values (4, 40)");
    exec(&mut a, "update t set v = 0 where id = 1");
    exec(&mut a, "delete from t where id = 2");
    assert_eq!(exec(&mut a, "select count(*) from t"), ["3"], "inside: 3 original rows - 1 deleted + 1 inserted");
    assert_eq!(exec(&mut a, "rollback"), ["ROLLBACK"], "ROLLBACK is answered with its tag");
    assert_eq!(exec(&mut a, "select id, v from t order by id"), ["1 10", "2 20", "3 30"], "everything is as it was");
}

#[test]
fn s4e_02_a_transaction_spans_several_calls_and_many_statements() {
    let db = db();
    let (mut a, mut b) = (Session::new(&db), Session::new(&db));
    exec(&mut a, "begin");
    for i in 4..=8 {
        exec(&mut a, &format!("insert into t values ({i}, {i}0)"));
    }
    assert_eq!(exec(&mut b, "select count(*) from t"), ["3"], "five inserts and none visible yet");
    exec(&mut a, "commit");
    assert_eq!(exec(&mut b, "select count(*) from t"), ["8"], "all five at once");
}

#[test]
fn s4e_02_begin_inside_a_transaction_and_commit_outside_one_only_warn() {
    let db = db();
    let mut a = Session::new(&db);
    assert_eq!(exec(&mut a, "commit"), ["WARNING 25P01: there is no transaction in progress"], "COMMIT without BEGIN");
    assert_eq!(exec(&mut a, "rollback"), ["WARNING 25P01: there is no transaction in progress"], "ROLLBACK without BEGIN");
    exec(&mut a, "begin");
    exec(&mut a, "update t set v = 1 where id = 1");
    assert_eq!(exec(&mut a, "begin"), ["WARNING 25001: there is already a transaction in progress"], "BEGIN inside a transaction");
    assert_eq!(exec(&mut a, "select v from t where id = 1"), ["1"], "the transaction and its change are still there");
    exec(&mut a, "rollback");
}

#[test]
fn s4e_02_a_session_that_goes_away_rolls_its_transaction_back() {
    let db = db();
    {
        let mut a = Session::new(&db);
        exec(&mut a, "begin");
        exec(&mut a, "update t set v = 0");
        exec(&mut Session::new(&db), "insert into t values (9, 90)");
        assert!(!quiet(&db), "while it is open the transaction holds the watermark back, behind the commit that came after it began");
    }
    assert!(quiet(&db), "dropping the session aborted it");
    let mut b = Session::new(&db);
    assert_eq!(exec(&mut b, "select sum(v) from t"), ["150"], "its changes are gone (60 and the 90 of the other session)");
}

#[test]
fn s4e_02_transactions_of_different_sessions_do_not_interfere() {
    let db = db();
    let (mut a, mut b) = (Session::new(&db), Session::new(&db));
    exec(&mut a, "begin");
    exec(&mut b, "begin");
    exec(&mut a, "update t set v = 111 where id = 1");
    exec(&mut b, "update t set v = 222 where id = 2");
    exec(&mut a, "commit");
    exec(&mut b, "rollback");
    assert_eq!(
        exec(&mut a, "select id, v from t order by id"),
        ["1 111", "2 20", "3 30"],
        "a's commit stands, b's rollback removed only b's work"
    );
}

// ---- 4e-03 · isolation levels -----------------------------------------------------------------------------------------------

#[test]
fn s4e_03_begin_takes_an_isolation_level() {
    let db = db();
    let mut a = Session::new(&db);
    for (sql, want) in [
        ("begin isolation level serializable", IsolationLevel::Serializable),
        ("begin isolation level snapshot", IsolationLevel::SnapshotIsolation),
        ("begin transaction isolation level repeatable read", IsolationLevel::SnapshotIsolation),
        ("begin isolation level read uncommitted", IsolationLevel::ReadUncommitted),
        ("BEGIN ISOLATION LEVEL   Serializable", IsolationLevel::Serializable),
    ] {
        assert_eq!(exec(&mut a, sql), ["BEGIN"], "{sql}");
        assert_eq!(a.isolation_level(), Some(want), "{sql}");
        exec(&mut a, "rollback");
    }
    assert_eq!(a.isolation_level(), None, "no transaction, no level");
}

#[test]
fn s4e_03_the_default_level_is_per_session_and_applies_to_later_transactions() {
    let db = db();
    let (mut a, mut b) = (Session::new(&db), Session::new(&db));
    exec(&mut b, "begin");
    assert_eq!(b.isolation_level(), Some(IsolationLevel::SnapshotIsolation), "the default is snapshot isolation");
    exec(&mut b, "rollback");
    exec(&mut a, "set default_transaction_isolation = 'serializable'");
    exec(&mut a, "begin");
    assert_eq!(a.isolation_level(), Some(IsolationLevel::Serializable), "BEGIN uses the session's default");
    exec(&mut a, "rollback");
    exec(&mut a, "begin isolation level snapshot");
    assert_eq!(a.isolation_level(), Some(IsolationLevel::SnapshotIsolation), "a level named in BEGIN wins");
    exec(&mut a, "rollback");
    exec(&mut b, "begin");
    assert_eq!(b.isolation_level(), Some(IsolationLevel::SnapshotIsolation), "the other session is not affected");
    exec(&mut b, "rollback");
}

#[test]
fn s4e_03_levels_the_engine_does_not_have_are_refused_and_open_nothing() {
    let db = db();
    let mut a = Session::new(&db);
    assert_eq!(
        fail(&mut a, "begin isolation level read committed").code(),
        "0A000",
        "read committed is not a level of this engine: a feature that is not supported"
    );
    assert!(!a.in_transaction(), "a refused BEGIN opens no transaction");
    assert!(a.execute("begin isolation level banana").is_err(), "an unknown level");
    assert!(a.execute("begin isolation").is_err(), "a level without a name is a syntax error");
    assert!(quiet(&db), "none of it left a transaction running");
}

#[test]
fn s4e_03_a_snapshot_transaction_reads_the_same_thing_every_time() {
    let db = db();
    let (mut a, mut b) = (Session::new(&db), Session::new(&db));
    exec(&mut a, "begin isolation level snapshot");
    assert_eq!(exec(&mut a, "select v from t where id = 1"), ["10"], "first read");
    exec(&mut b, "update t set v = 77 where id = 1");
    exec(&mut b, "insert into t values (4, 40)");
    assert_eq!(exec(&mut a, "select v from t where id = 1"), ["10"], "the same value again: repeatable");
    assert_eq!(exec(&mut a, "select count(*) from t"), ["3"], "and no phantom row");
    exec(&mut a, "commit");
    assert_eq!(exec(&mut a, "select v from t where id = 1"), ["77"], "a new transaction sees what was committed");
}

#[test]
fn s4e_03_the_same_interleaving_commits_at_snapshot_and_is_refused_at_serializable() {
    for (level, commits) in [("snapshot", true), ("serializable", false)] {
        let db = db();
        let (mut a, mut b) = (Session::new(&db), Session::new(&db));
        // a reads the whole table, b changes a row of it and commits, a inserts elsewhere and commits. The schedule is serializable
        // (a, then b), but the commit-time check is conservative: a later commit that wrote a row a's scan matches refuses a, a false
        // positive that serializable snapshot isolation in PostgreSQL would commit. Write skew is the anomaly the check is there for.
        exec(&mut a, &format!("begin isolation level {level}"));
        assert_eq!(exec(&mut a, "select sum(v) from t"), ["60"], "{level}: a reads");
        exec(&mut b, "update t set v = 0 where id = 1");
        exec(&mut a, "insert into t values (10, 600)");
        let result = a.execute("commit");
        assert_eq!(result.is_ok(), commits, "{level}: commit result {result:?}");
        assert!(!a.in_transaction(), "{level}: either way the transaction is over");
        assert!(quiet(&db), "{level}: nothing is left running");
    }
}

// ---- 4e-04 · failed transactions --------------------------------------------------------------------------------------------

#[test]
fn s4e_04_an_error_fails_the_transaction_until_it_ends() {
    let db = db();
    let mut a = Session::new(&db);
    exec(&mut a, "begin");
    exec(&mut a, "update t set v = 1 where id = 1");
    assert!(a.execute("select 1 / 0").is_err(), "the error");
    assert!(a.is_failed() && a.in_transaction(), "the transaction is failed but still open");
    for sql in ["select 1", "update t set v = 5", "insert into t values (7, 7)"] {
        let e = fail(&mut a, sql);
        assert_eq!(e.code(), "25P02", "{sql}: refused as a statement in a failed transaction, got {e:?}");
        assert!(e.to_string().contains("current transaction is aborted"), "{sql}: and the message says why, got {e:?}");
    }
}

#[test]
fn s4e_04_commit_of_a_failed_transaction_is_a_rollback_and_everything_in_it_is_gone() {
    let db = db();
    let (mut a, mut b) = (Session::new(&db), Session::new(&db));
    exec(&mut a, "begin");
    exec(&mut a, "update t set v = 1 where id = 1");
    exec(&mut a, "insert into t values (4, 40)");
    assert!(a.execute("select nope from t").is_err(), "an error");
    assert_eq!(exec(&mut a, "commit"), ["ROLLBACK"], "COMMIT answers with the tag of what really happened");
    assert!(!a.is_failed() && !a.in_transaction(), "the session is idle again");
    assert_eq!(
        exec(&mut b, "select id, v from t order by id"),
        ["1 10", "2 20", "3 30"],
        "the statements that had succeeded were undone too"
    );
}

#[test]
fn s4e_04_rollback_ends_a_failed_transaction_and_the_session_works_again() {
    let db = db();
    let mut a = Session::new(&db);
    exec(&mut a, "begin");
    assert!(a.execute("select * from nope").is_err(), "an error");
    assert_eq!(exec(&mut a, "rollback"), ["ROLLBACK"], "ROLLBACK");
    assert_eq!(exec(&mut a, "select count(*) from t"), ["3"], "autocommit works again");
    exec(&mut a, "begin");
    assert_eq!(exec(&mut a, "select 1"), ["1"], "and so does a new transaction");
    exec(&mut a, "commit");
}

#[test]
fn s4e_04_a_syntax_error_fails_the_open_transaction_too() {
    let db = db();
    let mut a = Session::new(&db);
    exec(&mut a, "begin");
    exec(&mut a, "update t set v = 1 where id = 1");
    assert!(a.execute("selec 1").is_err(), "a syntax error");
    assert!(a.is_failed(), "it fails the transaction as any other error does");
    assert_eq!(exec(&mut a, "commit"), ["ROLLBACK"], "and the earlier update is not committed");
    assert_eq!(exec(&mut a, "select v from t where id = 1"), ["10"], "it is gone");
}

#[test]
fn s4e_04_a_write_conflict_fails_the_second_writer_and_not_the_first() {
    let db = db();
    let (mut a, mut b) = (Session::new(&db), Session::new(&db));
    exec(&mut a, "begin");
    exec(&mut b, "begin");
    exec(&mut a, "update t set v = 100 where id = 2");
    let e = fail(&mut b, "update t set v = 200 where id = 2");
    assert_eq!(e.code(), "40001", "a conflict is a serialization failure, the code a client retries on: {e:?}");
    assert!(b.is_failed(), "the transaction that lost is failed");
    assert!(!a.is_failed(), "the one that won is not");
    assert_eq!(exec(&mut a, "commit"), ["COMMIT"], "the winner commits");
    assert_eq!(exec(&mut b, "commit"), ["ROLLBACK"], "the loser's COMMIT is a rollback");
    assert_eq!(exec(&mut a, "select v from t where id = 2"), ["100"], "the winner's value stands");
}

#[test]
fn s4e_04_an_autocommit_statement_that_conflicts_fails_alone() {
    let db = db();
    let (mut a, mut b) = (Session::new(&db), Session::new(&db));
    exec(&mut a, "begin");
    exec(&mut a, "update t set v = 100 where id = 2");
    let e = fail(&mut b, "update t set v = 200 where id = 2");
    assert_eq!(e.code(), "40001", "the writer that arrives second is refused with a serialization failure: {e:?}");
    assert!(!b.in_transaction() && !b.is_failed(), "autocommit has nothing to fail: the session is as before");
    assert_eq!(exec(&mut b, "select v from t where id = 1"), ["10"], "and works at once");
    exec(&mut a, "commit");
    assert_eq!(exec(&mut b, "select v from t where id = 2"), ["100"], "the first writer's value stands");
    assert!(quiet(&db), "no transaction is left running");
}

#[test]
fn s4e_04_a_failed_transaction_does_not_hold_the_garbage_collector_back() {
    let db = db();
    let mut a = Session::new(&db);
    exec(&mut a, "begin");
    assert!(a.execute("select 1 / 0").is_err(), "an error");
    assert!(a.is_failed(), "failed, and the client has not said ROLLBACK yet");
    assert!(quiet(&db), "but its work was already undone, and the watermark is free to move");
}

#[test]
fn s4e_04_a_refused_commit_is_a_serialization_failure_the_client_sees_by_code() {
    let db = db();
    let (mut a, mut b) = (Session::new(&db), Session::new(&db));
    exec(&mut a, "begin isolation level serializable");
    exec(&mut a, "select sum(v) from t");
    exec(&mut b, "update t set v = 0 where id = 1");
    exec(&mut a, "insert into t values (10, 600)");
    let e = fail(&mut a, "commit");
    assert_eq!(e.code(), "40001", "a commit that no longer validates is a serialization failure: {e:?}");
    assert!(e.client_message().starts_with("ERROR 40001: "), "and the client is shown its code: {}", e.client_message());
    assert!(!a.in_transaction() && !a.is_failed(), "the refused commit ended the transaction: {e:?}");
    assert!(quiet(&db), "nothing is left running");
}

/// Runs the shell with `script` as its standard input and returns what it wrote to standard output and to standard error.
fn shell(script: &str) -> (Vec<String>, String) {
    use std::io::Write;
    use std::process::{Command, Stdio};
    let mut child = Command::new(env!("CARGO_BIN_EXE_bustub_shell"))
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("the shell starts");
    child.stdin.take().expect("a piped stdin").write_all(script.as_bytes()).expect("the shell reads its input");
    let out = child.wait_with_output().expect("the shell exits at the end of its input");
    let stdout = String::from_utf8_lossy(&out.stdout).lines().map(|l| l.trim_end().to_string()).collect();
    (stdout, String::from_utf8_lossy(&out.stderr).into_owned())
}

#[test]
fn s4e_04_the_shell_runs_a_transaction_across_inputs() {
    let (out, _) = shell(
        "create table acct(id int, bal int);\n\
         begin;\n\
         insert into acct values (1, 100);\n\
         update acct\n   set bal = 90\n where id = 1;\n\
         commit;\n\
         select bal from acct;\n",
    );
    assert!(!out.iter().any(|l| l.contains("managed txn mode")), "BEGIN and COMMIT are the session's, not refused: {out:?}");
    let begin = out.iter().position(|l| l == "BEGIN").unwrap_or_else(|| panic!("BEGIN is answered with its tag: {out:?}"));
    assert_eq!(out[begin + 1..], ["1", "1", "COMMIT", "90"], "the insert and the update count a row each, COMMIT, then the value: {out:?}");
}

#[test]
fn s4e_04_the_shell_shows_an_error_by_its_code_and_logs_the_rest() {
    let (out, log) = shell("begin;\nselect 1 / 0;\nselect 1;\nrollback;\ncommit;\nselect 2;\n");
    let error = out.iter().position(|l| l.starts_with("ERROR ")).unwrap_or_else(|| panic!("the division fails: {out:?}"));
    assert!(out[error].starts_with("ERROR 22012: "), "the client sees the code and the message: {out:?}");
    assert_eq!(
        out[error + 1..],
        [
            "ERROR 25P02: current transaction is aborted, commands ignored until end of transaction block",
            "ROLLBACK",
            "WARNING 25P01: there is no transaction in progress",
            "2"
        ],
        "the failed transaction refuses work until ROLLBACK, and the session is idle after it: {out:?}"
    );
    assert!(log.contains("select 1 / 0"), "the log names the statement that failed: {log}");
    assert!(log.contains("22012") && log.contains("25P02"), "and the code of each error: {log}");
}

// ---- 4e-05 · boss: sessions in conflict -------------------------------------------------------------------------------------

fn finish_within<T: Send + 'static>(what: &str, f: impl FnOnce() -> T + Send + 'static) -> T {
    let (tx, rx) = std::sync::mpsc::channel();
    let what = what.to_string();
    thread::spawn(move || {
        let _ = tx.send(std::panic::catch_unwind(std::panic::AssertUnwindSafe(f)));
    });
    match rx.recv_timeout(Duration::from_secs(60)) {
        Ok(Ok(v)) => v,
        Ok(Err(p)) => std::panic::resume_unwind(p),
        Err(_) => panic!("{what}: still running after a minute"),
    }
}

fn account_db(accounts: i32, balance: i32) -> Arc<BusTubInstance> {
    let db = Arc::new(BusTubInstance::new(256));
    {
        let mut s = Session::new(&db);
        exec(&mut s, "create table acct(id int, bal int)");
        let rows: Vec<String> = (0..accounts).map(|i| format!("({i}, {balance})")).collect();
        exec(&mut s, &format!("insert into acct values {}", rows.join(", ")));
    }
    db
}

/// Moves `amount` from `from` to `to` inside one transaction; Err if any statement or the commit was refused (the session is idle again).
fn transfer(s: &mut Session<'_>, from: i32, to: i32, amount: i32) -> Result<(), DbError> {
    s.execute("begin")?;
    let result = (|| {
        s.execute(&format!("update acct set bal = bal - {amount} where id = {from}"))?;
        s.execute(&format!("update acct set bal = bal + {amount} where id = {to}"))?;
        s.execute("commit")
    })();
    if result.is_err() && s.in_transaction() {
        let _ = s.execute("rollback");
    }
    result.map(|_| ())
}

#[test]
fn s4e_05_a_bank_of_sessions_loses_no_money_and_conflicts_really_happen() {
    let db = account_db(4, 100);
    let conflicts = Arc::new(AtomicUsize::new(0));
    let (db2, conflicts2) = (db.clone(), conflicts.clone());
    finish_within("bank", move || {
        let hs: Vec<_> = (0..4u64)
            .map(|w| {
                let (db, conflicts) = (db2.clone(), conflicts2.clone());
                thread::spawn(move || {
                    let mut s = Session::new(&db);
                    let mut x = w * 7919 + 17;
                    for _ in 0..40 {
                        x = x.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
                        let (from, to) = (((x >> 33) % 4) as i32, ((x >> 17) % 4) as i32);
                        if from == to {
                            continue;
                        }
                        // retry until it commits: a conflict means someone else got there first
                        while let Err(e) = transfer(&mut s, from, to, 1 + ((x >> 8) % 5) as i32) {
                            assert_eq!(
                                e.code(),
                                "40001",
                                "a transfer is only ever refused for a conflict, the error a client retries: {e:?}"
                            );
                            conflicts.fetch_add(1, Ordering::SeqCst);
                            thread::yield_now();
                        }
                    }
                })
            })
            .collect();
        for h in hs {
            h.join().unwrap();
        }
    });
    let mut s = Session::new(&db);
    assert_eq!(exec(&mut s, "select sum(bal) from acct"), ["400"], "the total never changes: a transfer is all or nothing");
    assert!(
        conflicts.load(Ordering::SeqCst) > 0,
        "four sessions on four accounts must have conflicted at least once, or the test proves nothing"
    );
    assert!(quiet(&db), "no transaction is left running");
}

#[test]
fn s4e_05_a_reader_always_sees_a_consistent_total_while_transfers_run() {
    let db = account_db(4, 100);
    let stop = Arc::new(std::sync::atomic::AtomicBool::new(false));
    let writer = {
        let (db, stop) = (db.clone(), stop.clone());
        thread::spawn(move || {
            let mut s = Session::new(&db);
            let mut i = 0;
            while !stop.load(Ordering::SeqCst) {
                let _ = transfer(&mut s, i % 4, (i + 1) % 4, 3);
                i += 1;
            }
        })
    };
    let db2 = db.clone();
    finish_within("reader", move || {
        let mut r = Session::new(&db2);
        for _ in 0..200 {
            exec(&mut r, "begin isolation level snapshot");
            let first = exec(&mut r, "select sum(bal) from acct");
            let second = exec(&mut r, "select sum(bal) from acct");
            exec(&mut r, "commit");
            assert_eq!(first, ["400"], "a snapshot never sees half a transfer");
            assert_eq!(first, second, "and the same total inside one transaction");
        }
    });
    stop.store(true, Ordering::SeqCst);
    writer.join().unwrap();
}

#[test]
fn s4e_05_two_read_then_write_increments_cannot_both_win() {
    let db = Arc::new(BusTubInstance::new(64));
    let mut s = Session::new(&db);
    exec(&mut s, "create table counter(id int, n int)");
    exec(&mut s, "insert into counter values (1, 0)");
    let (mut a, mut b) = (Session::new(&db), Session::new(&db));
    exec(&mut a, "begin");
    exec(&mut b, "begin");
    assert_eq!(exec(&mut a, "select n from counter"), ["0"], "a reads 0");
    assert_eq!(exec(&mut b, "select n from counter"), ["0"], "b reads 0");
    exec(&mut a, "update counter set n = 1");
    assert!(b.execute("update counter set n = 1").is_err(), "b's write conflicts with a's uncommitted one");
    assert_eq!(exec(&mut a, "commit"), ["COMMIT"], "a commits");
    assert_eq!(exec(&mut b, "commit"), ["ROLLBACK"], "b is rolled back");
    assert_eq!(exec(&mut s, "select n from counter"), ["1"], "one increment, not two lost into one");
}

#[test]
fn s4e_05_write_skew_gets_through_snapshot_isolation_and_not_serializable() {
    for (level, both_commit) in [("snapshot", true), ("serializable", false)] {
        let db = Arc::new(BusTubInstance::new(64));
        let mut s = Session::new(&db);
        exec(&mut s, "create table doctor(id int, on_call int)");
        exec(&mut s, "insert into doctor values (1, 1), (2, 1)");
        let (mut a, mut b) = (Session::new(&db), Session::new(&db));
        exec(&mut a, &format!("begin isolation level {level}"));
        exec(&mut b, &format!("begin isolation level {level}"));
        assert_eq!(exec(&mut a, "select sum(on_call) from doctor"), ["2"], "{level}: a sees two on call");
        assert_eq!(exec(&mut b, "select sum(on_call) from doctor"), ["2"], "{level}: b sees two on call");
        exec(&mut a, "update doctor set on_call = 0 where id = 1");
        exec(&mut b, "update doctor set on_call = 0 where id = 2");
        let (ra, rb) = (a.execute("commit"), b.execute("commit"));
        let on_call = exec(&mut s, "select sum(on_call) from doctor");
        if both_commit {
            assert!(ra.is_ok() && rb.is_ok(), "{level}: both commit: {ra:?} {rb:?}");
            assert_eq!(on_call, ["0"], "{level}: and nobody is on call: write skew");
        } else {
            assert!(ra.is_ok() != rb.is_ok(), "{level}: exactly one of them is refused: {ra:?} {rb:?}");
            assert_eq!(on_call, ["1"], "{level}: so one doctor stays on call");
        }
    }
}

#[test]
fn s4e_05_sessions_leave_the_garbage_collector_free_whatever_happened() {
    let db = account_db(3, 50);
    {
        let mut a = Session::new(&db);
        let mut b = Session::new(&db);
        let _ = transfer(&mut a, 0, 1, 5);
        exec(&mut b, "begin");
        let _ = b.execute("select 1 / 0");
        let _ = transfer(&mut a, 1, 2, 5);
        exec(&mut a, "begin");
        exec(&mut a, "update acct set bal = 0");
    }
    assert!(quiet(&db), "after all the sessions are gone nothing is registered as running");
    let mut s = Session::new(&db);
    db.txn_manager.garbage_collection();
    assert_eq!(exec(&mut s, "select sum(bal) from acct"), ["150"], "and after a garbage collection the data is intact");
}

proptest! {
    #![proptest_config(ProptestConfig { cases: 24, failure_persistence: None, ..ProptestConfig::default() })]

    /// Property: a random script of BEGIN, COMMIT, ROLLBACK and updates run by one session leaves the table equal to a model that applies
    /// an update only when its transaction commits (autocommit statements apply at once).
    #[test]
    fn s4e_05_property_a_script_agrees_with_a_model(ops in proptest::collection::vec(0u8..7, 1..30)) {
        let db = db();
        let mut s = Session::new(&db);
        let mut model = [10i32, 20, 30];
        let mut pending: Option<[i32; 3]> = None;
        let mut next = 100;
        for op in ops {
            match op {
                0 if pending.is_none() => { exec(&mut s, "begin"); pending = Some(model); }
                1 if pending.is_some() => { exec(&mut s, "commit"); model = pending.take().unwrap(); }
                2 if pending.is_some() => { exec(&mut s, "rollback"); pending = None; }
                3..=5 => {
                    let id = (op - 3) as usize;
                    next += 1;
                    exec(&mut s, &format!("update t set v = {next} where id = {}", id + 1));
                    match pending.as_mut() { Some(p) => p[id] = next, None => model[id] = next }
                }
                _ => {}
            }
        }
        if pending.is_some() { exec(&mut s, "rollback"); }
        let want: Vec<String> = model.iter().enumerate().map(|(i, v)| format!("{} {v}", i + 1)).collect();
        prop_assert_eq!(exec(&mut s, "select id, v from t order by id"), want);
        prop_assert!(quiet(&db));
    }
}

// @@ challenge 4e-c1 begin
mod ch_4e_c1 {

    use super::*;
    use bustub::common::exception::{Exception, ExceptionType};
    use bustub::common::retry::{backoff_ms, is_retryable, run_with_retry};

    /// A real write conflict: the second of two transactions to update one row.
    fn conflict() -> DbError {
        let db = db();
        let (mut a, mut b) = (Session::new(&db), Session::new(&db));
        exec(&mut a, "begin");
        exec(&mut a, "update t set v = 1 where id = 1");
        fail(&mut b, "update t set v = 2 where id = 1")
    }

    /// A real refused commit: a serializable transaction whose read a later commit changed.
    fn refused_commit() -> DbError {
        let db = db();
        let (mut a, mut b) = (Session::new(&db), Session::new(&db));
        exec(&mut a, "begin isolation level serializable");
        exec(&mut a, "select sum(v) from t");
        exec(&mut b, "update t set v = 0 where id = 1");
        exec(&mut a, "insert into t values (10, 600)");
        fail(&mut a, "commit")
    }

    fn syntax_error() -> DbError {
        fail(&mut Session::new(&db()), "selec 1")
    }

    #[test]
    fn s4e_c1_only_serialization_failures_are_retryable() {
        assert!(is_retryable(&conflict()), "a write conflict");
        assert!(is_retryable(&refused_commit()), "a refused commit");
        assert!(is_retryable(&conflict().context("transferring 5 from 1 to 2")), "a conflict under context: the code is the root's");
        assert!(!is_retryable(&syntax_error()), "a syntax error never gets better");
        let db = db();
        let mut a = Session::new(&db);
        assert!(!is_retryable(&fail(&mut a, "select 1 / 0")), "nor does a division by zero");
        exec(&mut a, "begin");
        let _ = a.execute("select 1 / 0");
        assert!(!is_retryable(&fail(&mut a, "select 1")), "a statement refused in a failed transaction: only ROLLBACK helps");
    }

    #[test]
    fn s4e_c1_the_decision_is_the_code_not_the_words() {
        let worded = DbError::from(Exception::new(ExceptionType::Execution, "could not commit: it conflicts with a concurrent one"));
        assert_ne!(worded.code(), "40001", "an internal error that only sounds like a conflict");
        assert!(!is_retryable(&worded), "is not retried: the message is for people, the code is for programs");
        let duplicate = DbError::from(bustub::storage::index::index::unique_violation("t_pk"));
        assert!(!is_retryable(&duplicate), "a duplicate key (23505) is a conflict of data, not of transactions: retrying repeats it");
    }

    #[test]
    fn s4e_c1_success_reports_the_attempts_used() {
        assert_eq!(run_with_retry(5, |_| Ok::<_, DbError>("v")).unwrap(), ("v", 1), "first try");
        let mut calls = vec![];
        let r = run_with_retry(5, |a| {
            calls.push(a);
            if a < 3 {
                Err(conflict())
            } else {
                Ok(a * 10)
            }
        });
        assert_eq!(r.unwrap(), (30, 3), "two conflicts, then success");
        assert_eq!(calls, [1, 2, 3], "attempts are numbered from 1");
    }

    #[test]
    fn s4e_c1_it_gives_up_after_the_limit_with_the_last_error() {
        let mut n = 0;
        let r: Result<((), usize), DbError> = run_with_retry(3, |_| {
            n += 1;
            Err(conflict())
        });
        assert_eq!(r.map_err(|e| e.code()).unwrap_err(), "40001", "still conflicting: the last conflict comes back");
        assert_eq!(n, 3, "exactly three attempts");
        assert_eq!(run_with_retry(0, |_| Ok::<_, DbError>(1)).unwrap(), (1, 1), "a limit of zero still tries once");
    }

    #[test]
    fn s4e_c1_a_non_retryable_error_stops_at_once() {
        let mut n = 0;
        let r: Result<((), usize), DbError> = run_with_retry(10, |_| {
            n += 1;
            Err(syntax_error())
        });
        assert_eq!(r.unwrap_err().code(), syntax_error().code(), "the error is passed on as it is");
        assert_eq!(n, 1, "no retry");
    }

    #[test]
    fn s4e_c1_backoff_grows_to_a_cap_and_is_deterministic() {
        for seed in [0u64, 1, 99, u64::MAX] {
            for attempt in 1..=12usize {
                let b = backoff_ms(attempt, seed);
                let base = (1u64 << (attempt - 1).min(6)).min(64);
                assert!(b >= base / 2 && b <= base, "attempt {attempt}, seed {seed}: {b} outside [{}, {base}]", base / 2);
                assert_eq!(b, backoff_ms(attempt, seed), "the same inputs, the same wait");
            }
        }
        assert!((1..=12).any(|a| backoff_ms(a, 1) != backoff_ms(a, 2)), "different clients (seeds) are spread apart");
    }
}
// @@ challenge 4e-c1 end

// @@ challenge 4e-c2 begin
mod ch_4e_c2 {

    use super::*;
    use bustub::sql::split::split_statements;

    #[test]
    fn s4e_c2_plain_statements_split_at_semicolons_and_empties_are_dropped() {
        assert_eq!(split_statements("select 1; select 2;"), ["select 1", "select 2"], "two statements");
        assert_eq!(split_statements("  ;; select 1 ;  ;"), ["select 1"], "empty statements and spaces vanish");
        assert_eq!(split_statements(""), Vec::<String>::new(), "nothing");
    }

    #[test]
    fn s4e_c2_a_semicolon_in_a_string_or_identifier_is_not_a_boundary() {
        assert_eq!(
            split_statements("insert into t values ('a;b'); select 1"),
            ["insert into t values ('a;b')", "select 1"],
            "inside a string"
        );
        assert_eq!(
            split_statements("select \"we;ird\" from t; select 2"),
            ["select \"we;ird\" from t", "select 2"],
            "inside a quoted identifier"
        );
        assert_eq!(
            split_statements("select 'it''s; fine'; select 2"),
            ["select 'it''s; fine'", "select 2"],
            "a doubled quote does not end the string"
        );
    }

    #[test]
    fn s4e_c2_comments_hide_semicolons_and_quotes() {
        assert_eq!(
            split_statements("select 1 -- a; b\n; select 2"),
            ["select 1 -- a; b", "select 2"],
            "a line comment runs to the end of the line"
        );
        assert_eq!(
            split_statements("select /* ; ' */ 1; select 2"),
            ["select /* ; ' */ 1", "select 2"],
            "a block comment, and the quote inside it starts nothing"
        );
        assert_eq!(
            split_statements("select 1; -- the end"),
            ["select 1", "-- the end"],
            "a trailing comment is a statement of its own as written"
        );
    }

    #[test]
    fn s4e_c2_unterminated_input_is_the_last_statement_as_it_stands() {
        assert_eq!(split_statements("select 'open; still open"), ["select 'open; still open"], "an unterminated string");
        assert_eq!(split_statements("select 1; select /* open ; "), ["select 1", "select /* open ;"], "an unterminated comment");
    }

    #[test]
    fn s4e_c2_a_minus_or_a_slash_alone_is_code() {
        assert_eq!(split_statements("select 4 - 1; select 6 / 2"), ["select 4 - 1", "select 6 / 2"], "only -- and /* start comments");
    }

    proptest! {
        #![proptest_config(ProptestConfig { cases: 300, failure_persistence: None, ..ProptestConfig::default() })]

        /// Property: statements built from safe words, quoted strings and comments, joined by semicolons, split back into themselves.
        #[test]
        fn s4e_c2_property_joining_then_splitting_returns_the_statements(stmts in proptest::collection::vec(
            prop_oneof![
                "[a-z ]{1,8}".prop_map(|s| format!("select {}", s.trim())),
                "[a-z;]{0,6}".prop_map(|s| format!("insert into t values ('{s}')")),
                "[a-z ;]{0,6}".prop_map(|s| format!("select 1 /* {s} */")),
                "[a-z ;]{0,6}".prop_map(|s| format!("select 2 -- {s}\nfrom t")),
            ], 0..6)) {
            let stmts: Vec<String> = stmts.into_iter().map(|s| s.trim().to_string()).filter(|s| !s.is_empty()).collect();
            let script = stmts.join(";\n");
            prop_assert_eq!(split_statements(&script), stmts);
        }
    }
}
// @@ challenge 4e-c2 end

// @@ challenge 4e-c3 begin
mod ch_4e_c3 {

    use super::*;
    use bustub::common::session_state::{step, Input, Input::*, Reply, State, State::*};

    fn run(inputs: &[Input]) -> (State, Vec<Reply>) {
        let mut s = Idle;
        let mut replies = vec![];
        for i in inputs {
            let (next, r) = step(s, *i);
            s = next;
            replies.push(r);
        }
        (s, replies)
    }

    #[test]
    fn s4e_c3_a_healthy_transaction_commits() {
        let (end, r) = run(&[Begin, Statement { ok: true }, Commit]);
        assert_eq!(r, [Reply::Tag("BEGIN"), Reply::Done, Reply::Tag("COMMIT")], "a transaction with no failure commits");
        assert_eq!(end, Idle, "and is over");
    }

    #[test]
    fn s4e_c3_commit_of_a_failed_transaction_says_rollback() {
        let (end, r) = run(&[Begin, Statement { ok: true }, Statement { ok: false }, Commit]);
        assert_eq!(r[3], Reply::Tag("ROLLBACK"), "it was rolled back, and the client must be told so");
        assert_eq!(end, Idle, "and the session is idle");
    }

    #[test]
    fn s4e_c3_everything_but_commit_and_rollback_is_refused_in_a_failed_transaction() {
        let (end, r) = run(&[Begin, Statement { ok: false }, Statement { ok: true }, Begin, Statement { ok: false }]);
        assert_eq!(&r[2..], [Reply::Refused, Reply::Refused, Reply::Refused], "no work gets done after a failure");
        assert_eq!(end, Failed, "and it stays failed until the client ends it");
    }

    #[test]
    fn s4e_c3_rollback_ends_every_state() {
        for start in [Idle, InTxn, Failed] {
            let (next, _) = step(start, Rollback);
            assert_eq!(next, Idle, "{start:?}: ROLLBACK ends in idle");
        }
        assert_eq!(step(Failed, Rollback), (Idle, Reply::Tag("ROLLBACK")), "a failed transaction is rolled back");
    }

    #[test]
    fn s4e_c3_stray_commands_outside_a_transaction_only_warn() {
        assert_eq!(step(Idle, Commit), (Idle, Reply::Warning), "COMMIT with nothing open");
        assert_eq!(step(Idle, Rollback), (Idle, Reply::Warning), "ROLLBACK with nothing open");
        assert_eq!(step(InTxn, Begin), (InTxn, Reply::Warning), "BEGIN inside a transaction");
        assert_eq!(step(Idle, Statement { ok: false }), (Idle, Reply::Error), "an error in autocommit changes no state");
    }

    proptest! {
        #![proptest_config(ProptestConfig { cases: 300, failure_persistence: None, ..ProptestConfig::default() })]

        /// Property: a COMMIT reply is only given for a transaction in which no statement failed.
        #[test]
        fn s4e_c3_property_commit_is_never_reported_after_a_failure(inputs in proptest::collection::vec(prop_oneof![Just(Begin), Just(Statement { ok: true }), Just(Statement { ok: false }), Just(Commit), Just(Rollback)], 0..25)) {
            let mut s = Idle;
            let mut failed_in_txn = false;
            for i in inputs {
                let (next, reply) = step(s, i);
                if s == Idle && next == InTxn { failed_in_txn = false; }
                if s == InTxn && matches!(i, Statement { ok: false }) { failed_in_txn = true; }
                if reply == Reply::Tag("COMMIT") {
                    prop_assert!(!failed_in_txn, "COMMIT reported for a transaction with a failed statement, after {:?}", i);
                }
                s = next;
            }
        }
    }
}
// @@ challenge 4e-c3 end

// @@ challenge 4e-c4 begin
mod ch_4e_c4 {

    use super::*;
    use bustub::common::idle_reaper::{expired, next_check_in, SessionInfo};

    fn s(id: u64, in_txn: bool, last: u64) -> SessionInfo {
        SessionInfo { id, in_txn, last_active_ms: last }
    }

    #[test]
    fn s4e_c4_only_sessions_idle_for_more_than_the_timeout_in_a_transaction_expire() {
        let sessions = [s(1, true, 850), s(2, true, 900), s(3, false, 0), s(4, true, 950)];
        assert_eq!(expired(&sessions, 1000, 100), [1], "idle 150; 100 is not more than 100; a session without a transaction never expires");
    }

    #[test]
    fn s4e_c4_the_longest_idle_goes_first_and_ties_are_by_id() {
        let sessions = [s(7, true, 100), s(3, true, 100), s(5, true, 0), s(9, true, 500)];
        assert_eq!(expired(&sessions, 1000, 100), [5, 3, 7, 9], "idle 1000, then the two idle 900 by id, then 500");
    }

    #[test]
    fn s4e_c4_a_clock_that_went_backwards_expires_nothing() {
        assert_eq!(expired(&[s(1, true, 5000)], 1000, 10), Vec::<u64>::new(), "last activity after now: idle for zero");
        assert_eq!(
            next_check_in(&[s(1, true, 5000)], 1000, 10),
            Some(11),
            "and it will expire 11 ms after it was last active (as far as we can tell)"
        );
    }

    #[test]
    fn s4e_c4_the_next_check_is_when_the_earliest_session_will_pass_the_timeout() {
        let sessions = [s(1, true, 950), s(2, true, 990), s(3, false, 0)];
        assert_eq!(next_check_in(&sessions, 1000, 100), Some(51), "session 1 is idle 50, so it expires 51 ms from now");
        assert_eq!(next_check_in(&[s(3, false, 0)], 1000, 100), None, "no transaction, nothing to wait for");
        assert_eq!(next_check_in(&[s(1, true, 0)], 1000, 100), None, "already expired: reaped now, not waited for");
    }

    #[test]
    fn s4e_c4_no_sessions_no_work() {
        assert!(expired(&[], 10, 5).is_empty(), "this check: expected `expired(&[], 10, 5).is_empty()`");
        assert_eq!(next_check_in(&[], 10, 5), None, "this check");
    }

    proptest! {
        #![proptest_config(ProptestConfig { cases: 300, failure_persistence: None, ..ProptestConfig::default() })]

        /// Property: advancing the clock by `next_check_in` expires at least one more session; a larger timeout never adds one.
        #[test]
        fn s4e_c4_property_the_deadline_is_the_moment_something_expires(raw in proptest::collection::vec((any::<bool>(), 0u64..1000), 0..8), now in 1000u64..1200, timeout in 0u64..300) {
            let sessions: Vec<SessionInfo> = raw.iter().enumerate().map(|(i, (t, l))| s(i as u64, *t, *l)).collect();
            let now_ids = expired(&sessions, now, timeout);
            prop_assert!(now_ids.iter().all(|id| sessions[*id as usize].in_txn), "only transactions expire");
            let bigger = expired(&sessions, now, timeout + 50);
            prop_assert!(bigger.iter().all(|id| now_ids.contains(id)), "a larger timeout is a subset");
            if let Some(wait) = next_check_in(&sessions, now, timeout) {
                prop_assert!(expired(&sessions, now + wait, timeout).len() > now_ids.len(), "something more is expired {} ms later", wait);
                prop_assert!(wait == 0 || expired(&sessions, now + wait - 1, timeout).len() == now_ids.len(), "and not a millisecond earlier");
            }
        }
    }
}
// @@ challenge 4e-c4 end

// @@ challenge 4e-c5 begin
mod ch_4e_c5 {

    use super::*;
    use bustub::common::session_pool::SessionPool;
    use std::sync::atomic::AtomicUsize;

    #[test]
    fn s4e_c5_sequential_borrowers_share_one_session() {
        let db = db();
        let pool = SessionPool::new(&db, 4);
        for _ in 0..5 {
            assert_eq!(pool.with(|s| exec(s, "select count(*) from t")), ["3"], "a borrowed session works");
        }
        assert_eq!((pool.created(), pool.idle()), (1, 1), "one session was enough, and it is back");
    }

    #[test]
    fn s4e_c5_a_transaction_left_open_is_rolled_back_before_the_next_borrower() {
        let db = db();
        let pool = SessionPool::new(&db, 1);
        pool.with(|s| {
            exec(s, "begin");
            exec(s, "update t set v = 0");
        });
        assert!(quiet(&db), "the leftover transaction was rolled back, so nothing is running");
        pool.with(|s| {
            assert!(!s.in_transaction(), "the next borrower starts clean");
            assert_eq!(exec(s, "select sum(v) from t"), ["60"], "and sees no trace of it");
        });
    }

    #[test]
    fn s4e_c5_a_failed_transaction_is_cleaned_up_too() {
        let db = db();
        let pool = SessionPool::new(&db, 1);
        pool.with(|s| {
            exec(s, "begin");
            assert!(s.execute("select 1 / 0").is_err(), "this check: expected `s.execute(\"select 1 / 0\").is_err()`");
            assert!(s.is_failed(), "left failed");
        });
        pool.with(|s| {
            assert!(!s.is_failed() && !s.in_transaction(), "the failed state does not leak");
            assert_eq!(exec(s, "select 1"), ["1"], "and statements run");
        });
    }

    #[test]
    fn s4e_c5_a_changed_isolation_default_is_reset() {
        let db = db();
        let pool = SessionPool::new(&db, 1);
        pool.with(|s| {
            exec(s, "set default_transaction_isolation = 'serializable'");
        });
        pool.with(|s| {
            exec(s, "begin");
            assert_eq!(s.isolation_level(), Some(IsolationLevel::SnapshotIsolation), "the setting of the previous client is gone");
            exec(s, "rollback");
        });
    }

    #[test]
    fn s4e_c5_no_more_than_max_borrowers_run_at_once() {
        let db = db();
        let pool = SessionPool::new(&db, 2);
        let (inside, worst) = (AtomicUsize::new(0), AtomicUsize::new(0));
        thread::scope(|scope| {
            for _ in 0..6 {
                scope.spawn(|| {
                    for _ in 0..5 {
                        pool.with(|s| {
                            let now = inside.fetch_add(1, Ordering::SeqCst) + 1;
                            worst.fetch_max(now, Ordering::SeqCst);
                            exec(s, "select count(*) from t");
                            thread::sleep(Duration::from_millis(1));
                            inside.fetch_sub(1, Ordering::SeqCst);
                        });
                    }
                });
            }
        });
        assert!(worst.load(Ordering::SeqCst) <= 2, "never more than two at once, saw {}", worst.load(Ordering::SeqCst));
        assert!(pool.created() <= 2, "and never more than two sessions created");
        assert_eq!(pool.idle(), pool.created(), "all of them are back");
    }
}
// @@ challenge 4e-c5 end

// @@ challenge 4e-c6 begin
mod ch_4e_c6 {

    use super::*;
    use bustub::common::read_only::ReadOnlySession;

    #[test]
    fn s4e_c6_queries_and_transaction_control_pass_through() {
        let db = db();
        let mut r = ReadOnlySession::new(Session::new(&db));
        assert_eq!(r.execute("select id from t order by id").unwrap(), ["1", "2", "3"], "a query");
        assert_eq!(r.execute("begin").unwrap(), ["BEGIN"], "BEGIN is allowed");
        assert_eq!(r.execute("select count(*) from t").unwrap(), ["3"], "a query inside a transaction");
        assert!(r.inner().in_transaction(), "the wrapped session is the one in the transaction");
        assert_eq!(r.execute("rollback").unwrap(), ["ROLLBACK"], "ROLLBACK is allowed");
        assert!(r.execute("explain select * from t").is_ok(), "EXPLAIN of a query");
        assert!(r.execute("set default_transaction_isolation = 'serializable'").is_ok(), "SET");
    }

    #[test]
    fn s4e_c6_every_kind_of_change_is_refused_by_name() {
        let db = db();
        let mut r = ReadOnlySession::new(Session::new(&db));
        for (sql, kind) in [
            ("insert into t values (9, 9)", "INSERT"),
            ("update t set v = 0", "UPDATE"),
            ("delete from t", "DELETE"),
            ("create table u(a int)", "CREATE TABLE"),
            ("create index i on t(id)", "CREATE INDEX"),
        ] {
            let e = r.execute(sql).unwrap_err();
            assert_eq!(e.code(), "25006", "{sql}: refused as a write in a read-only transaction");
            assert!(e.to_string().contains(kind), "{sql}: the message names the statement: {e:?}");
        }
    }

    #[test]
    fn s4e_c6_nothing_of_a_refused_text_runs() {
        let db = db();
        let mut r = ReadOnlySession::new(Session::new(&db));
        assert!(r.execute("select 1; delete from t where id = 1").is_err(), "the second statement is a write");
        assert_eq!(Session::new(&db).execute("select count(*) from t").unwrap(), ["3"], "and the table is untouched");
        assert!(r.execute("update t set v = 1; select 1").is_err(), "a write first");
        assert_eq!(Session::new(&db).execute("select sum(v) from t").unwrap(), ["60"], "nothing changed");
    }

    #[test]
    fn s4e_c6_a_change_hidden_in_explain_analyze_is_refused() {
        let db = db();
        let mut r = ReadOnlySession::new(Session::new(&db));
        assert!(r.execute("explain analyze delete from t").is_err(), "EXPLAIN ANALYZE would execute it");
        assert_eq!(Session::new(&db).execute("select count(*) from t").unwrap(), ["3"], "nothing was deleted");
    }

    #[test]
    fn s4e_c6_a_refusal_changes_no_state() {
        let db = db();
        let mut r = ReadOnlySession::new(Session::new(&db));
        r.execute("begin").unwrap();
        assert!(r.execute("insert into t values (4, 4)").is_err(), "refused");
        assert!(r.inner().in_transaction() && !r.inner().is_failed(), "the transaction is still open and healthy");
        assert_eq!(r.execute("select count(*) from t").unwrap(), ["3"], "and works");
        r.execute("commit").unwrap();
    }
}
// @@ challenge 4e-c6 end
