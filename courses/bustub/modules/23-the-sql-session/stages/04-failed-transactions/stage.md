Autocommit has an easy answer to an error: the statement was the whole transaction, so abort it. Inside `begin ... commit` it is harder. Two statements ran, the third failed. Were the first two meant to stay? Should the fourth run? PostgreSQL's answer is blunt and good: **the transaction is dead**. Every later statement is refused until the client says `rollback` (or `commit`, which also rolls back), because the client might otherwise commit half of what it meant to do. This stage implements that, and with it a decision that is easy to get subtly wrong: *when* the work of the dead transaction is undone.

> [!CHECK] A transaction ran an update, then a statement failed. The client is slow and does not send `rollback` for ten seconds. During those ten seconds, other clients keep committing. Should the database undo the dead transaction's update now or when the client says rollback? What does each choice cost, and what does the client see in either case?
> ||Now. The client sees the same either way (every statement is refused; `rollback` and `commit` both answer `ROLLBACK`), so nothing is gained by waiting, and waiting costs: the transaction stays registered with the watermark, the version chains it touched cannot be pruned, and any row it wrote stays locked against other writers (a write conflict for them) for ten seconds for nothing. The *session state* ("failed") has to persist until the client ends it, because only the client knows when it has seen the error; the *work* does not need to.||
>
> - Which errors fail a transaction? A syntax error? A write conflict?
> - What does `commit` answer in a failed transaction, and is it an error?
> - Does an error in autocommit mode leave the session failed?

## The task

Extend `Session` (in `src/common/session.rs`):

- A statement that fails inside an open transaction (an error, or the engine's `Ok(false)` for a write conflict) **fails the transaction**: it is aborted at once and the session is *failed*: `is_failed()` is true and `in_transaction()` stays true.
- While failed, every statement except `commit` and `rollback` (and `begin`, which warns as before) is refused with an `Execution` error whose message contains `current transaction is aborted`.
- `commit` of a failed transaction answers `ROLLBACK` and returns the session to idle; so does `rollback`. Nothing the failed transaction did is visible, including the statements before the error.
- A **syntax error** in the text fails an open transaction too.
- In autocommit an error leaves nothing failed.
- A write-write conflict between two sessions fails the one that lost, never the one that was first.

## Your freedom

The shape of the state (a flag next to the optional transaction, or an enum), the text of the error message beyond the phrase the tests look for, and whether the abort happens in the session or in a helper.

## The Rust toolbox

**A state that is more than `Option`.** "Open and failed" and "open and healthy" and "closed" are three states; a `bool` next to an `Option` works until a path forgets to set it. An enum with `Failed` makes the `match` exhaustive.

**Early `return Err` with cleanup first.** In the failure path: abort, record the state, then `return Err(...)`. Writing the cleanup *after* the return is the bug the tests catch.

**`let _ = result;`** for an abort whose own failure you cannot do anything about, with a comment that says so.

## If this is new

- L8 Error design: an error that carries enough to act on, and errors that are states.
- [S1 Option and Result](/t/s1-option-result): an outcome with three cases.

## Tests

- An error fails the transaction; later statements are refused with the reason.
- COMMIT and ROLLBACK end a failed transaction, both answering ROLLBACK; the earlier statements are gone.
- A syntax error fails it too; a write conflict fails only the loser.
- A failed transaction does not hold the garbage collector back.

## Hints

### Fail at the point of failure

The place where a statement is known to have failed is the place to abort and to set the state: not at the next statement and not at `commit`. Then every later path sees a session whose state is already right.

### `commit` has three situations

Idle (a warning), healthy (commit, which may itself be refused), failed (answer `ROLLBACK`, clear the state). A `match` on the three keeps them apart.

### The parser is part of the transaction

`parse(sql)?` returning early on a syntax error skips your failure handling. Handle the parse error in the same place as the others when a transaction is open.

## Performance

Aborting at once costs the same as aborting later and releases resources earlier, so there is no trade-off to find; the instructive number is how long a failed transaction *used* to hold things. Measure by what a writer on the same row sees: a failed transaction that is not aborted keeps blocking it.

**Measure it.** Session A updates a row and then fails; session B updates the same row. With abort-at-failure B succeeds at once; with abort-at-rollback B is refused until A's client acts. How long is that in a script with a `sleep`?

## Experiment

Optional. Predict first, then run.

1. **Let a failed statement be skipped and the transaction continue** (MySQL's statement-level rollback). Which test fails, and what half-applied state can a client now commit?
2. **Abort only at `rollback`.** Which test fails?

## Other designs

- **Savepoints** (`savepoint a; ...; rollback to a`): the failed statement's effects are undone back to a named point and the transaction continues; the client chooses where to resume. The engine's undo logs make this natural.
- **Statement-level atomicity without a failed state** (MySQL, SQLite's default conflict handling): easier for clients, and a transaction can commit having skipped a statement.
- **Retry on the server** (for serialization failures): the database re-runs the whole transaction itself; needs the transaction to be a stored procedure, not a stream of statements.

## In BusTub

BusTub's shell prints the error and keeps the transaction (a tainted one keeps running statements that all fail); there is no failed *state*. Marking a conflict taints the transaction (project 4); this stage makes that visible to the client in the PostgreSQL way.

## The C/C++ way

| C / C++ | Rust |
|---|---|
| `bool failed_` checked at the top of `Execute` | a state that the `match` forces you to handle |
| `try { ... } catch (const ExecutionException &) { Abort(); throw; }` | cleanup in the failure arm, then `return Err(e)` |
| an error string compared in tests | a message phrase and an error kind |

**Port rule:** the state a client observes after an error is part of the interface: write it down and test it.

## Learn more

- [PostgreSQL: transactions in the tutorial](https://www.postgresql.org/docs/current/tutorial-transactions.html) · [PostgreSQL: SAVEPOINT](https://www.postgresql.org/docs/current/sql-savepoint.html)
