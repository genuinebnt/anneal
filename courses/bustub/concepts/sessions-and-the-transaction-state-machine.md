---
title: Sessions and the transaction state machine
summary: A SQL session is a small state machine: idle (autocommit), in a transaction, or in a failed transaction. What every statement does, and what COMMIT and ROLLBACK mean, follows from the state and nothing else.
minutes: 8
---
A transaction manager knows how to begin, commit and abort transactions. It does not know what a *client* means by `begin; update ...; select ...; commit;`: that three statements sent one at a time belong together, that a statement sent with no `begin` in front of it is a complete transaction of its own, and that after an error the client has to say something before the database will listen again. That knowledge lives one layer up, in the **session**, and it is a state machine with three states and a handful of rules.

## Three states

- **Idle.** No transaction is open. A data statement runs in a transaction made for it alone, which commits when the statement finishes (**autocommit**) and aborts if it fails. This is the mode of a client that never says `begin`: every statement is atomic by itself and visible to everyone the moment it returns.
- **In a transaction.** `begin` opened one. Statements run inside it, see its own writes, and become visible to others only at `commit`.
- **Failed.** A statement in the open transaction raised an error (a division by zero, an unknown table, a syntax error, a write conflict). The transaction cannot continue: some of its statements ran and one did not, and the database refuses to guess what the client meant. Every statement except `commit` and `rollback` is rejected with *current transaction is aborted, commands ignored until end of transaction block*; either of those two ends the state, and **`commit` of a failed transaction rolls back**, and says `ROLLBACK`.

## The transitions

| state | `begin` | statement | `commit` | `rollback` |
|---|---|---|---|---|
| idle | in a transaction | runs and commits by itself | warning: nothing to commit | warning |
| in a transaction | warning: already in one | runs; an error fails the transaction | commits (a refusal is an error, and the transaction is over) | aborts |
| failed | refused with the reason | refused with the reason | aborts; answers `ROLLBACK` | aborts |

Everything in this table is a decision a real system also made, and mostly the same one: PostgreSQL's warnings for a stray `commit` or a second `begin`, the "aborted" state, and the idea that an error inside a transaction poisons it are all PostgreSQL's behaviour.

## Why the failed state exists

The alternative is to let the client carry on after an error: the failed statement is skipped and the transaction continues. That is how a series of statements can leave a half-applied business operation (the debit ran, the credit failed) that a later `commit` makes permanent. Making the whole transaction fail forces the client to look at the error, and gives the database one simple guarantee: **a transaction either commits all of its statements or none**. Systems that want to continue after an error offer *savepoints*, a named point inside a transaction to roll back to.

The failed transaction's **work** is undone at once in this design: its versions are removed and the watermark is free to move. The *state* persists until the client ends it, because only the client knows when it has noticed.

## What a commit can refuse

`commit` is not always a formality. Under snapshot isolation a transaction that wrote a row someone else changed and committed first cannot commit (first committer wins); under serializable isolation a transaction whose reads were invalidated by a commit that happened after it started fails validation. Both come back as an **error from `commit`**, and the transaction is then over: a session that gets one must not treat it as still open. The client's answer is to run the whole transaction again, which is why robust code wraps transactions in a retry loop.

## C++ comparison

| C / C++ | Rust |
|---|---|
| a `Session` class with `Transaction *txn_` that may be null and a `bool failed_` | an `enum State { Idle, Active(Txn), Failed }`, or `Option<Arc<Txn>>` plus a flag |
| a destructor that rolls back | `impl Drop for Session` |
| `try { ... } catch (...) { abort; throw; }` | `match result { Err(e) => { abort; Err(e) } ... }` |
| a flag nobody remembers to reset | a state enum whose `match` is exhaustive |

## In real code

### Using it: the three states as a type

```rust test
#[derive(Debug, Clone, Copy, PartialEq)]
enum State {
    Idle,
    InTxn,
    Failed,
}

#[derive(Debug, Clone, Copy, PartialEq)]
enum Input {
    Begin,
    Statement { ok: bool },
    Commit,
    Rollback,
}

#[derive(Debug, PartialEq)]
enum Reply {
    Tag(&'static str),
    Warning(&'static str),
    Refused,
    Done,
    Error,
}

/// What a session does with `input` in `state`.
fn step(state: State, input: Input) -> (State, Reply) {
    use Input::*;
    use State::*;
    match (state, input) {
        (Idle, Begin) => (InTxn, Reply::Tag("BEGIN")),
        (InTxn, Begin) => (InTxn, Reply::Warning("already in a transaction")),
        (Failed, Begin) => (Failed, Reply::Refused),
        // autocommit: the statement is its own transaction, whatever happens to it
        (Idle, Statement { ok }) => (Idle, if ok { Reply::Done } else { Reply::Error }),
        (InTxn, Statement { ok: true }) => (InTxn, Reply::Done),
        (InTxn, Statement { ok: false }) => (Failed, Reply::Error),
        (Failed, Statement { .. }) => (Failed, Reply::Refused),
        (Idle, Commit) | (Idle, Rollback) => (Idle, Reply::Warning("no transaction in progress")),
        (InTxn, Commit) => (Idle, Reply::Tag("COMMIT")),
        (Failed, Commit) | (Failed, Rollback) | (InTxn, Rollback) => (Idle, Reply::Tag("ROLLBACK")),
    }
}

fn run(inputs: &[Input]) -> (State, Vec<Reply>) {
    let mut s = State::Idle;
    let mut replies = vec![];
    for i in inputs {
        let (next, r) = step(s, *i);
        s = next;
        replies.push(r);
    }
    (s, replies)
}

#[test]
fn an_error_inside_a_transaction_poisons_it_until_the_client_ends_it() {
    use Input::*;
    let (end, replies) = run(&[Begin, Statement { ok: true }, Statement { ok: false }, Statement { ok: true }, Commit]);
    assert_eq!(replies, [Reply::Tag("BEGIN"), Reply::Done, Reply::Error, Reply::Refused, Reply::Tag("ROLLBACK")]);
    assert_eq!(end, State::Idle, "COMMIT of a failed transaction rolled it back and ended it");
}

#[test]
fn outside_a_transaction_an_error_changes_nothing_about_the_state() {
    use Input::*;
    let (end, replies) = run(&[Statement { ok: false }, Statement { ok: true }, Commit]);
    assert_eq!(replies, [Reply::Error, Reply::Done, Reply::Warning("no transaction in progress")]);
    assert_eq!(end, State::Idle);
}

#[test]
fn every_state_has_a_way_out() {
    use Input::*;
    for s in [State::Idle, State::InTxn, State::Failed] {
        let (next, _) = step(s, Rollback);
        assert_eq!(next, State::Idle, "{s:?}: ROLLBACK always ends up idle");
    }
}
```

### In the exercises

- **4e-01:** the `Idle` row: a statement is its own transaction. **4e-02:** `Begin`, `Commit`, `Rollback` and the warnings. **4e-03:** the level taken by `Begin`. **4e-04:** the `Failed` state and what `Commit` does in it, the codes a client reads in each row (`40001` to retry, `25P02` while failed, `25P01` and `25001` for the warnings), and the shell, which becomes one session.

### Where it is used

- **PostgreSQL**: `TBLOCK_DEFAULT`, `TBLOCK_INPROGRESS`, `TBLOCK_ABORT` in `xact.c` are these states; the message "current transaction is aborted, commands ignored until end of transaction block" is the third column of the table.
- **MySQL / InnoDB**: autocommit mode (`SET autocommit = 0` to start every statement inside a transaction), and a statement error that rolls back only the statement, which is the other possible design.
- **SQLite**: `BEGIN` (deferred, immediate, exclusive), autocommit when none is open, and the `Drop` of a connection rolls back.
