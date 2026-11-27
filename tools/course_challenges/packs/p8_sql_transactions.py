from _c import C
M4E = "23-the-sql-session"
CH = []

CH.append(C("4e-c1", M4E, "90-challenge-retry-with-backoff", "build", "Challenge: retry with backoff", "easy", "stages_4e::s4e_c1",
  ["telling a conflict you should retry from an error you should not","a backoff schedule that spreads competing clients apart"],
  ["snapshot-isolation","sessions-and-the-transaction-state-machine","property-testing-and-fuzzing"],
  "`is_retryable`, `backoff_ms` and `run_with_retry` in `src/common/retry.rs`: the client's half of optimistic concurrency. A transaction that lost a conflict is not a bug, it is the contract: run it again. `run_with_retry` calls a closure up to a limit, retrying only errors that are conflicts, and `backoff_ms` says how long to wait before attempt `n` so that two clients that collided do not collide again in lockstep.",
  "Every system with optimistic concurrency control (snapshot isolation, serializable validation, compare-and-swap) pushes retrying onto the caller. Done badly it either gives up on the first conflict, retries errors that can never succeed (a syntax error, ten times), or retries in lockstep so that the same two transactions collide for ever. The three decisions are small and each is a classic bug. The first is made on the SQLSTATE code: a message is written for people and reworded at will, and a retry loop that matches words retries an internal error that happens to say "could not commit".",
  ["`is_retryable(e)`: true exactly when `e.code()` is `40001`, a serialization failure (a write conflict or a refused commit, from your session of 4e-04, with any context on top); false for everything else, whatever its message says.","`run_with_retry(max_attempts, f)` calls `f(attempt)` with attempts numbered from 1. Success returns `Ok((value, attempts_used))`. A retryable error is retried while attempts remain; any other error, or the last failure, is returned at once. `max_attempts` of 0 behaves as 1.","`backoff_ms(attempt, seed)`: the wait before attempt `attempt + 1`: an exponential base `min(2^(attempt - 1), 64)` milliseconds with jitter, a value in `[base / 2, base]` that depends only on `(attempt, seed)`."],
  ["The closure is called at most `max_attempts` times and never after a non-retryable error.","`backoff_ms` is deterministic and never above 64."],
  ["Attempts used is 1 when the first call succeeds.","The upper envelope of the backoff doubles until the cap.","Different seeds give different jitter for some attempt."],
  ["f fails with a conflict twice, then succeeds: Ok((v, 3))","f fails with a syntax error: Err after one call","an internal error worded \"could not commit: it conflicts\" (XX000): not retryable"],
  ["Real conflicts and refused commits are retryable, under context too; nothing else is, whatever its words.","Success, retried success, giving up; non-retryable errors stop at once.","Backoff bounds and determinism."],
  src=("src/common/retry.rs", '''
//! Retrying a transaction that lost a conflict.

use crate::common::db_error::{DbError, DbResult};

/// Is this error a serialization failure (code `40001`), which the client answers by running the transaction again?
pub fn is_retryable(e: &DbError) -> bool {
    // @begin 4e-c1
    e.code() == "40001"
    //~ todo!("4e-c1: serialization failures only, whatever their message says")
    // @end
}

/// Milliseconds to wait before attempt `attempt + 1`.
pub fn backoff_ms(attempt: usize, seed: u64) -> u64 {
    // @begin 4e-c1
    let base: u64 = 1u64 << attempt.saturating_sub(1).min(6);
    let base = base.min(64);
    // a cheap deterministic mix of (attempt, seed)
    let mut x = seed ^ (attempt as u64).wrapping_mul(0x9E37_79B9_7F4A_7C15);
    x ^= x >> 33;
    x = x.wrapping_mul(0xff51_afd7_ed55_8ccd);
    x ^= x >> 29;
    base / 2 + x % (base / 2 + 1)
    //~ todo!("4e-c1: min(2^(attempt-1), 64), then a jitter in [base/2, base] that depends only on (attempt, seed)")
    // @end
}

/// Runs `f(1)`, `f(2)`, ... until it succeeds, fails with something that is not retryable, or `max_attempts` calls were made.
pub fn run_with_retry<T>(max_attempts: usize, mut f: impl FnMut(usize) -> DbResult<T>) -> DbResult<(T, usize)> {
    // @begin 4e-c1
    let max = max_attempts.max(1);
    let mut attempt = 1;
    loop {
        match f(attempt) {
            Ok(v) => return Ok((v, attempt)),
            Err(e) if attempt < max && is_retryable(&e) => attempt += 1,
            Err(e) => return Err(e),
        }
    }
    //~ let _ = (max_attempts, &mut f);
    //~ todo!("4e-c1: call f with the attempt number; retry only retryable errors while attempts remain")
    // @end
}
'''),
  test=("tests/stages_4e.rs", '''
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
''')))

CH.append(C("4e-c2", M4E, "91-challenge-splitting-a-script", "build", "Challenge: splitting a script", "medium", "stages_4e::s4e_c2",
  ["finding statement boundaries in SQL text","why a semicolon inside a string or a comment is not a boundary"],
  ["lexers-and-precedence-climbing","property-testing-and-fuzzing","parsing-numbers-from-text"],
  "`split_statements` in `src/sql/split.rs`: split a script into its statements at the semicolons that really end one, without a full parser: skip over single-quoted strings (a doubled quote is a quote), double-quoted identifiers, `--` comments to the end of the line and `/* ... */` comments. Each statement comes back trimmed, without its semicolon; empty statements are dropped.",
  "Every shell, migration tool and driver has to do this before a single statement reaches a session, and the naive `text.split(';')` breaks on the first `insert into t values ('a;b')`. It is also a small state machine that is easy to get almost right: the interesting bugs are an unterminated string at the end, a quote inside a comment, and a doubled quote.",
  ["Normal text ends a statement at `;`.","A `'` starts a string that ends at the next `'` not followed by another `'`; `;` inside is ordinary.","A `\"` starts a quoted identifier that ends at the next `\"` (`\"\"` is a quote).","`--` starts a comment that ends at the end of the line; `/*` one that ends at `*/` (no nesting). A `;` inside a comment is ordinary, and quotes inside a comment do not start strings.","An unterminated string or comment runs to the end of the text: it is the last statement as it stands.","Each returned statement is trimmed of whitespace; statements that are empty after trimming are dropped."],
  ["Joining the result with `;` and splitting again gives the same list.","No returned statement is empty."],
  ["A script without special characters equals `text.split(';')` trimmed, empties dropped.","Appending a statement appends exactly one element."],
  ["`select 1; select 2;` -> [`select 1`, `select 2`]","`insert into t values ('a;b'); select 1` -> two statements","`select 1 -- a; b` -> one"],
  ["Plain splitting and empties.","Strings and quoted identifiers.","Comments.","Unterminated input.","A property against a model."],
  src=("src/sql/split.rs", '''
//! Splitting a SQL script into statements.

pub fn split_statements(text: &str) -> Vec<String> {
    // @begin 4e-c2
    #[derive(PartialEq)]
    enum S {
        Code,
        Str,
        Ident,
        Line,
        Block,
    }
    let chars: Vec<char> = text.chars().collect();
    let mut out = Vec::new();
    let mut cur = String::new();
    let mut state = S::Code;
    let mut i = 0;
    let mut push = |cur: &mut String, out: &mut Vec<String>| {
        let t = cur.trim();
        if !t.is_empty() {
            out.push(t.to_string());
        }
        cur.clear();
    };
    while i < chars.len() {
        let c = chars[i];
        let next = chars.get(i + 1).copied();
        match state {
            S::Code => match (c, next) {
                (';', _) => {
                    push(&mut cur, &mut out);
                }
                ('\\'', _) => {
                    state = S::Str;
                    cur.push(c);
                }
                ('"', _) => {
                    state = S::Ident;
                    cur.push(c);
                }
                ('-', Some('-')) => {
                    state = S::Line;
                    cur.push_str("--");
                    i += 1;
                }
                ('/', Some('*')) => {
                    state = S::Block;
                    cur.push_str("/*");
                    i += 1;
                }
                _ => cur.push(c),
            },
            S::Str | S::Ident => {
                let quote = if state == S::Str { '\\'' } else { '"' };
                cur.push(c);
                if c == quote {
                    if next == Some(quote) {
                        cur.push(quote);
                        i += 1;
                    } else {
                        state = S::Code;
                    }
                }
            }
            S::Line => {
                cur.push(c);
                if c == '\\n' {
                    state = S::Code;
                }
            }
            S::Block => {
                cur.push(c);
                if c == '*' && next == Some('/') {
                    cur.push('/');
                    i += 1;
                    state = S::Code;
                }
            }
        }
        i += 1;
    }
    push(&mut cur, &mut out);
    out
    //~ todo!("4e-c2: a small state machine over the characters: code, string, quoted identifier, line comment, block comment")
    // @end
}
'''),
  test=("tests/stages_4e.rs", '''
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
    assert_eq!(split_statements("insert into t values ('a;b'); select 1"), ["insert into t values ('a;b')", "select 1"], "inside a string");
    assert_eq!(split_statements("select \\"we;ird\\" from t; select 2"), ["select \\"we;ird\\" from t", "select 2"], "inside a quoted identifier");
    assert_eq!(split_statements("select 'it''s; fine'; select 2"), ["select 'it''s; fine'", "select 2"], "a doubled quote does not end the string");
}

#[test]
fn s4e_c2_comments_hide_semicolons_and_quotes() {
    assert_eq!(split_statements("select 1 -- a; b\\n; select 2"), ["select 1 -- a; b", "select 2"], "a line comment runs to the end of the line");
    assert_eq!(split_statements("select /* ; ' */ 1; select 2"), ["select /* ; ' */ 1", "select 2"], "a block comment, and the quote inside it starts nothing");
    assert_eq!(split_statements("select 1; -- the end"), ["select 1", "-- the end"], "a trailing comment is a statement of its own as written");
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
            "[a-z ;]{0,6}".prop_map(|s| format!("select 2 -- {s}\\nfrom t")),
        ], 0..6)) {
        let stmts: Vec<String> = stmts.into_iter().map(|s| s.trim().to_string()).filter(|s| !s.is_empty()).collect();
        let script = stmts.join(";\\n");
        prop_assert_eq!(split_statements(&script), stmts);
    }
}
''')))

CH.append(C("4e-c3", M4E, "92-challenge-the-commit-that-kept-a-failed-transaction", "debug", "Challenge: the commit that kept a failed transaction", "easy", "stages_4e::s4e_c3",
  ["reading a state machine for the transition that is wrong","what COMMIT must say about a transaction that failed"],
  ["sessions-and-the-transaction-state-machine","property-testing-and-fuzzing","errors-4b"],
  "`step` in `src/common/session_state.rs` is the transition function of a session: from a state (idle, in a transaction, failed) and an input (BEGIN, a statement that succeeded or failed, COMMIT, ROLLBACK) to the next state and a reply. It looks right and has one wrong transition: it lets a client believe work was saved that was not. Find the bug and fix it.",
  "Every other mistake in a transaction state machine is loud: a refused statement, a warning. This one is silent, and it is the worst thing a database can do: report `COMMIT` for a transaction that was rolled back, so the application carries on believing its changes are durable. A table of transitions you can read at a glance, and a property over sequences, is how this class of bug is found before a customer does.",
  ["Idle: BEGIN opens a transaction (`BEGIN`); a statement runs by itself (`Done` or `Error`, state unchanged); COMMIT and ROLLBACK only warn.","In a transaction: BEGIN warns; a statement that succeeds is `Done`; one that fails is `Error` and fails the transaction; COMMIT answers `COMMIT`, ROLLBACK answers `ROLLBACK`, both go idle.","Failed: BEGIN and statements are refused; COMMIT and ROLLBACK both answer `ROLLBACK` and go idle."],
  ["A reply of `COMMIT` is only ever given for a transaction in which no statement failed.","From any state, ROLLBACK ends in idle."],
  ["Replaying the same inputs gives the same replies.","Inserting a COMMIT after a failed statement never yields `COMMIT`."],
  ["BEGIN, failed statement, COMMIT -> BEGIN, Error, ROLLBACK","BEGIN, ok statement, COMMIT -> BEGIN, Done, COMMIT"],
  ["Each row of the transition table.","A property over input sequences."],
  src=("src/common/session_state.rs", '''
//! The states of a SQL session and what each input does in each state.

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum State {
    Idle,
    InTxn,
    Failed,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Input {
    Begin,
    Statement { ok: bool },
    Commit,
    Rollback,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Reply {
    Tag(&'static str),
    Warning,
    Refused,
    Done,
    Error,
}

pub fn step(state: State, input: Input) -> (State, Reply) {
    use Input::*;
    use State::*;
    match (state, input) {
        (Idle, Begin) => (InTxn, Reply::Tag("BEGIN")),
        (InTxn, Begin) => (InTxn, Reply::Warning),
        (Failed, Begin) => (Failed, Reply::Refused),
        (Idle, Statement { ok }) => (Idle, if ok { Reply::Done } else { Reply::Error }),
        (InTxn, Statement { ok: true }) => (InTxn, Reply::Done),
        (InTxn, Statement { ok: false }) => (Failed, Reply::Error),
        (Failed, Statement { .. }) => (Failed, Reply::Refused),
        (Idle, Commit) | (Idle, Rollback) => (Idle, Reply::Warning),
        (InTxn, Commit) => (Idle, Reply::Tag("COMMIT")),
        (InTxn, Rollback) => (Idle, Reply::Tag("ROLLBACK")),
        // @begin 4e-c3
        (Failed, Commit) | (Failed, Rollback) => (Idle, Reply::Tag("ROLLBACK")),
        //~ (Failed, Commit) => (Idle, Reply::Tag("COMMIT")),
        //~ (Failed, Rollback) => (Idle, Reply::Tag("ROLLBACK")),
        // @end
    }
}
'''),
  test=("tests/stages_4e.rs", '''
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
''')))

CH.append(C("4e-c4", M4E, "93-challenge-the-idle-reaper", "build", "Challenge: the idle reaper", "easy", "stages_4e::s4e_c4",
  ["deciding which sessions to abort for sitting idle in a transaction","computing when to look again instead of polling"],
  ["watermarks-and-garbage-collection","sessions-and-the-transaction-state-machine","performance-tests-and-measuring"],
  "`expired` and `next_check_in` in `src/common/idle_reaper.rs`: given what the server knows about its sessions (an id, whether a transaction is open, when it last did something) and the current time, say which sessions to abort, oldest first, and how many milliseconds until the earliest one that is not yet expired will be. The server's timer thread uses `next_check_in` to sleep exactly as long as it needs to.",
  "A client that opens a transaction and goes to lunch holds back the garbage collector for the whole database: every version written since is retained, and every scan walks it. Production systems have a setting for it (`idle_in_transaction_session_timeout` in PostgreSQL) and the decision is a pure function of the session table and the clock, which is the right shape for a function that must be right and is hard to test with real time.",
  ["A session is expired when it has a transaction open and `now - last_active` is **strictly greater** than the timeout (use `saturating_sub`: a clock that went backwards expires nothing).","`expired` returns the ids of expired sessions, the longest idle first, ties by smaller id first.","Sessions with no open transaction never expire, however long they have been idle.","`next_check_in` is the smallest `timeout - idle + 1` over sessions that have a transaction open and are not yet expired (the moment the earliest one becomes expired); `None` if there is none."],
  ["`expired` only names sessions that have a transaction open.","Whatever `expired` names would also be named a millisecond later."],
  ["Raising the timeout never adds an id to the result.","Advancing `now` by `next_check_in` makes at least one more session expired."],
  ["timeout 100: a (txn, idle 150), b (txn, idle 100), c (no txn, idle 900) -> [a]; next check in 1 ms (b becomes expired at 101)"],
  ["Which sessions expire and in which order.","No transaction, no expiry.","The next deadline.","A property."],
  src=("src/common/idle_reaper.rs", '''
//! Which sessions have been idle in a transaction for too long?

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SessionInfo {
    pub id: u64,
    pub in_txn: bool,
    pub last_active_ms: u64,
}

/// Ids to abort: a transaction open and idle for more than `timeout_ms`, longest idle first, ties by id.
pub fn expired(sessions: &[SessionInfo], now_ms: u64, timeout_ms: u64) -> Vec<u64> {
    // @begin 4e-c4
    let mut hits: Vec<(u64, u64)> = sessions
        .iter()
        .filter(|s| s.in_txn && now_ms.saturating_sub(s.last_active_ms) > timeout_ms)
        .map(|s| (now_ms.saturating_sub(s.last_active_ms), s.id))
        .collect();
    hits.sort_by(|a, b| b.0.cmp(&a.0).then(a.1.cmp(&b.1)));
    hits.into_iter().map(|(_, id)| id).collect()
    //~ todo!("4e-c4: sessions with a transaction idle for more than the timeout, longest first, ties by id")
    // @end
}

/// Milliseconds until the next session (with a transaction, not yet expired) becomes expired.
pub fn next_check_in(sessions: &[SessionInfo], now_ms: u64, timeout_ms: u64) -> Option<u64> {
    // @begin 4e-c4
    sessions
        .iter()
        .filter(|s| s.in_txn)
        .map(|s| now_ms.saturating_sub(s.last_active_ms))
        .filter(|idle| *idle <= timeout_ms)
        .map(|idle| timeout_ms - idle + 1)
        .min()
    //~ todo!("4e-c4: the smallest time until a transaction-holding session passes the timeout")
    // @end
}
'''),
  test=("tests/stages_4e.rs", '''
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
    assert_eq!(next_check_in(&[s(1, true, 5000)], 1000, 10), Some(11), "and it will expire 11 ms after it was last active (as far as we can tell)");
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
    assert!(expired(&[], 10, 5).is_empty());
    assert_eq!(next_check_in(&[], 10, 5), None);
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
''')))

CH.append(C("4e-c5", M4E, "94-challenge-a-session-pool", "extend", "Challenge: a session pool", "medium", "stages_4e::s4e_c5",
  ["handing sessions out and taking them back with their state cleaned","bounding the number of concurrent sessions"],
  ["sessions-and-the-transaction-state-machine","condvars-and-blocking-queues","raii-guards-and-lifetimes"],
  "`SessionPool` in `src/common/session_pool.rs`, built on **your** `Session`: `with(|session| ...)` lends a session to a closure, waiting if the maximum number are already out, and when the closure returns takes the session back after **resetting it**: an open transaction is rolled back (even a failed one), and its default isolation level is back to the default. A pool that hands the next client a session in the middle of someone else's transaction is a data leak.",
  "Connection pools are in front of nearly every database in production, and their worst bugs are not about speed but about what a session remembers. The classic one: client A leaves a transaction open or sets a session variable, the pool gives the same connection to client B, and B's statements run inside A's transaction or at A's isolation level. PostgreSQL poolers have a `server_reset_query` for exactly this.",
  ["`new(db, max)`: at most `max` sessions are created, lazily, over the pool's life.","`with(f)`: waits until a session is free (an idle one, or room to create one), runs `f(&mut session)`, resets the session and returns it to the pool, and returns what `f` returned.","The reset: if `in_transaction()`, run `rollback`; then `set default_transaction_isolation = 'snapshot'`.","`created()` is how many sessions exist, `idle()` how many are waiting in the pool."],
  ["At most `max` closures run at once.","A session handed out has no open transaction and the default level, whatever the previous borrower did."],
  ["Sequential `with` calls reuse one session (`created()` stays 1).","A transaction left open by one `with` is gone in the next (the watermark is free)."],
  ["max 2, three threads calling `with`: never more than two inside at once; created() <= 2"],
  ["Reuse and the bound.","A leftover transaction (healthy and failed) is rolled back.","A changed default level is reset.","Many threads."],
  src=("src/common/session_pool.rs", '''
//! A bounded pool of sessions.

use std::sync::{Condvar, Mutex};

use crate::common::bustub_instance::BusTubInstance;
use crate::common::session::Session;

pub struct SessionPool<'a> {
    db: &'a BusTubInstance,
    // @begin 4e-c5
    max: usize,
    state: Mutex<PoolState<'a>>,
    freed: Condvar,
    //~ _pool: (),
    // @end
}

// @begin 4e-c5
struct PoolState<'a> {
    idle: Vec<Session<'a>>,
    created: usize,
}
// @end

impl<'a> SessionPool<'a> {
    pub fn new(db: &'a BusTubInstance, max: usize) -> SessionPool<'a> {
        // @begin 4e-c5
        SessionPool { db, max: max.max(1), state: Mutex::new(PoolState { idle: Vec::new(), created: 0 }), freed: Condvar::new() }
        //~ let _ = (db, max);
        //~ todo!("4e-c5: an empty pool that may create up to `max` sessions")
        // @end
    }

    /// Sessions that exist.
    pub fn created(&self) -> usize {
        // @begin 4e-c5
        self.state.lock().unwrap().created
        //~ todo!("4e-c5: how many sessions were created")
        // @end
    }

    /// Sessions waiting to be lent.
    pub fn idle(&self) -> usize {
        // @begin 4e-c5
        self.state.lock().unwrap().idle.len()
        //~ todo!("4e-c5: how many sessions are waiting")
        // @end
    }

    /// Lends a session to `f`, then resets it and takes it back.
    pub fn with<R>(&self, f: impl FnOnce(&mut Session<'a>) -> R) -> R {
        // @begin 4e-c5
        let mut session = {
            let mut st = self.state.lock().unwrap();
            loop {
                if let Some(s) = st.idle.pop() {
                    break s;
                }
                if st.created < self.max {
                    st.created += 1;
                    break Session::new(self.db);
                }
                st = self.freed.wait(st).unwrap();
            }
        };
        let result = f(&mut session);
        // reset: nobody may inherit a transaction or a setting
        if session.in_transaction() {
            let _ = session.execute("rollback");
        }
        let _ = session.execute("set default_transaction_isolation = 'snapshot'");
        self.state.lock().unwrap().idle.push(session);
        self.freed.notify_one();
        result
        //~ let _ = f;
        //~ todo!("4e-c5: take an idle session (or create one, or wait); run f; reset the session; give it back and wake a waiter")
        // @end
    }
}
'''),
  test=("tests/stages_4e.rs", '''
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
        assert!(s.execute("select 1 / 0").is_err());
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
''')))

CH.append(C("4e-c6", M4E, "95-challenge-a-read-only-session", "extend", "Challenge: a read-only session", "easy", "stages_4e::s4e_c6",
  ["refusing writes before any of the text runs","walking a parsed statement, including the ones that hide a statement inside"],
  ["sessions-and-the-transaction-state-machine","the-sql-pipeline","property-testing-and-fuzzing"],
  "`ReadOnlySession` in `src/common/read_only.rs`: a wrapper around **your** `Session` that accepts queries and refuses everything that would change the database (`INSERT`, `UPDATE`, `DELETE`, `CREATE TABLE`, `CREATE INDEX`, and an `EXPLAIN ANALYZE` of one of those). The check is made on the whole text *before* anything runs, so a script that ends in a delete never executes its first statements either.",
  "Reporting users, replicas and dashboards get read-only credentials, and the safe version of \"read-only\" is enforced where statements are understood, not by hoping the application is careful. The detail that makes it a good exercise is *before anything runs*: refusing at the third statement of three leaves two of them committed, which is exactly what a read-only guarantee exists to prevent.",
  ["`new(session)` wraps a session; `execute(sql)` parses the text with `parser::parse`; if any statement is a data or schema change (including one inside `EXPLAIN`), it returns an error with code `25006` (PostgreSQL's read-only SQL transaction; an `Exception` can name its code with `with_sqlstate`) whose message names the kind of statement, and runs nothing. Errors of the wrapped session come back as they are.","Otherwise it forwards the text to the wrapped session and returns its answer. `BEGIN`, `COMMIT`, `ROLLBACK`, `SET`, `SHOW`, `SELECT` and `EXPLAIN` of a query are allowed.","`inner()` gives access to the wrapped session (for `in_transaction()` and so on)."],
  ["No statement of a refused text is executed.","A text of only queries behaves exactly as it does without the wrapper."],
  ["Adding a write to the end of any allowed text makes it refused.","Refusal changes no state: an open transaction stays open."],
  ["`select 1; delete from t` -> Err, and `t` is untouched","`explain select * from t` -> allowed"],
  ["Queries pass through.","Each kind of write is refused.","Nothing of a refused text runs.","Transaction control and SET are allowed."],
  src=("src/common/read_only.rs", '''
//! A session that cannot change anything.

use crate::common::db_error::DbResult;
use crate::common::exception::{Exception, ExceptionType, Result};
use crate::common::session::Session;
use crate::sql::ast::Statement;
use crate::sql::parser;

pub struct ReadOnlySession<'a> {
    // @begin 4e-c6
    inner: Session<'a>,
    //~ _ro: std::marker::PhantomData<&'a ()>,
    // @end
}

impl<'a> ReadOnlySession<'a> {
    /// Wraps `session`.
    pub fn new(session: Session<'a>) -> ReadOnlySession<'a> {
        // @begin 4e-c6
        ReadOnlySession { inner: session }
        //~ todo!("4e-c6: wrap the session")
        // @end
    }

    /// The wrapped session. Statements run through it are not checked.
    pub fn inner(&mut self) -> &mut Session<'a> {
        // @begin 4e-c6
        &mut self.inner
        //~ todo!("4e-c6: the wrapped session")
        // @end
    }

    /// Runs `sql` if none of its statements changes data or schema.
    ///
    /// # Errors
    ///
    /// Refuses the whole text, running nothing, with an error of code `25006` (a read-only transaction) naming the first kind of
    /// statement that would change something. Otherwise, the errors of `Session::execute`.
    pub fn execute(&mut self, sql: &str) -> DbResult<Vec<String>> {
        // @begin 4e-c6
        for statement in parser::parse(sql)? {
            if let Some(kind) = Self::change_kind(&statement) {
                return Err(Exception::new(ExceptionType::Invalid, format!("cannot execute {kind} in a read-only session"))
                    .with_sqlstate("25006")
                    .into());
            }
        }
        self.inner.execute(sql)
        //~ let _ = (sql, parser::parse as fn(&str) -> Result<Vec<Statement>>);
        //~ todo!("4e-c6: parse; refuse the whole text if any statement changes data or schema; else run it")
        // @end
    }

    // @begin 4e-c6
    /// What kind of change `statement` is, if it is one (also when hidden inside EXPLAIN).
    fn change_kind(statement: &Statement) -> Option<&'static str> {
        match statement {
            Statement::Insert { .. } => Some("INSERT"),
            Statement::Update { .. } => Some("UPDATE"),
            Statement::Delete { .. } => Some("DELETE"),
            Statement::CreateTable { .. } => Some("CREATE TABLE"),
            Statement::CreateIndex { .. } => Some("CREATE INDEX"),
            Statement::Explain { statement, .. } => Self::change_kind(statement),
            _ => None,
        }
    }
    //~ // TODO(4e-c6): a helper of yours
    // @end
}
'''),
  test=("tests/stages_4e.rs", '''
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
''')))
