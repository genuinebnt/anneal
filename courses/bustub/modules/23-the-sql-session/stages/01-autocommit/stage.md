In modules 4a and 4b you built a transaction manager: `begin` gives you a transaction, statements run inside it through `execute_sql_txn`, and `commit` or `abort` ends it. Nobody talks to a database that way. A client types `update t set v = 1;` and expects it to happen **completely or not at all**, and to be visible to everybody the moment the prompt comes back. Something has to make a transaction for that statement, run it, and commit it or throw it away, and that something is a **session**: the object that stands for one client's connection. This stage is its simplest mode, the one every client is in until it says `begin`: **autocommit**.

> [!CHECK] `update t set v = v / (id - 2)` on rows with `id` 1, 2 and 3 fails with a division by zero when it reaches the row with `id = 2`: the row `id = 1` was already changed by then. After the error, what should `select * from t` show, and what has to happen between the error and your session's next statement for that to be true? What would a bug here look like to another session?
> ||All three rows as they were: the statement was a transaction, and a transaction that fails is aborted, which removes the versions it wrote. The session has to **abort the transaction it created** when the statement fails (and commit it when it succeeds); nothing else cleans up. A bug shows as the half-applied update being visible to other sessions, or, subtler, as a transaction left running that holds the garbage collector's watermark back for ever.||
>
> - Which transaction does a statement run in if the session has none open?
> - What does the session do with the transaction when the statement returns an error? When it returns "failed" without an error (a write conflict)?
> - What does it do with a text of several statements when the second one fails?

## The task

Implement `Session` in `src/common/session.rs` for the case where no transaction is open:

- `Session::new(&db)` and `execute(sql) -> DbResult<Vec<String>>`: parse the text (`parser::parse`) and run each statement in order. The error is your `DbError` from module 3k: the engine's `Exception` converts with `?`, and keeps its code (a division by zero is still `22012` when it reaches the client).
- Each data statement runs in **its own transaction**, begun with the session's default isolation level (snapshot isolation), executed with `BusTubInstance::execute_statements(..., Some(&txn))`, and committed when it succeeds. If it fails (an `Err`, or `Ok(false)`, which is the engine saying the statement conflicted) the transaction is aborted and the error comes back.
- The answer is the rows the statements printed, one string per row, cells separated by one space (use `SimpleStreamWriter::new(&mut out, true, " ")`).
- The first failing statement ends the text; the statements before it stay committed.
- A commit that is refused (`commit` returns `false`) is an error, not a success.

## Your freedom

Whether a statement is run through `execute_statements` once per statement or the whole text at once, how you represent "no open transaction" (an `Option`, an enum), and how the error for a refused commit is shaped. The tests check visibility, atomicity and that nothing is left running; they do not look at your fields.

## The Rust toolbox

**A struct that borrows.** `Session<'a>` holds `&'a BusTubInstance`; the lifetime says the session cannot outlive the database. Several sessions on one instance borrow it shared, which is why `BusTubInstance` uses interior mutability.

**`matches!` on a nested result.** `matches!(reply, Ok(Some(_)))` is `true` for one shape of a `Result<Option<_>>` and `false` for the other two, without a three-armed `match`.

**Abort on every exit.** There are several ways out of "run the statement": success, `Err`, `Ok(false)`. Put the cleanup in one place after the call instead of at each `return`.

**`?` across error types.** The engine returns `Result<_, Exception>` and the session `DbResult<_>`; `?` calls your `From<Exception> for DbError` on the way out. Where the error is the session's own doing (the transaction manager refusing to begin or commit), `.context("committing a transaction")` from 3k says what the session was doing, for the log.

## If this is new

- [L3 Lifetimes](/t/l3-lifetimes): a struct that holds a reference.
- [S1 Option and Result](/t/s1-option-result): `Ok(false)` as a third outcome next to `Ok(true)` and `Err`.

## Tests

- A statement is visible to another session as soon as it returns; several statements in one text run in order.
- A statement that fails halfway changes nothing; the session keeps working.
- An error stops the text but keeps what ran before it.
- No transaction is left running after any of this.

## Hints

### Decide the end of the transaction before you start it

For each statement there are exactly two endings, commit and abort. Write the code so that the transaction is created, the statement is run, and then one `if ok { commit } else { abort }` follows: no path should be able to skip it.

### The watermark is your test instrument

`txn_manager.get_watermark()` equals `last_commit_ts()` when no transaction is running. If it does not after your session returned, you left one registered.

### `Ok(false)` is not success

The engine turns a transaction conflict into "the statement did not succeed" without an error. Treat it as a failure: abort, and give the caller an error with a message that says why.

## Performance

Autocommit makes a transaction per statement, so every statement pays a begin, a commit and the registration with the watermark. That cost is small next to executing, and large for a loop of one-row inserts: a thousand inserts as a thousand transactions is much slower than one `begin; ...; commit`. That is the reason for batching, and the reason a durable database's autocommit mode is dominated by the log flush at each commit (module 4c).

**Measure it.** Insert 10 000 rows one statement at a time, and again inside one transaction. How much of the difference is commit overhead?

## Experiment

Optional. Predict first, then run.

1. **Forget to abort when the statement returns `Ok(false)`.** Which test fails, and what does `get_watermark` show?
2. **Commit even when the statement failed.** What does `update t set v = v / (id - 2)` leave in the table?

## Other designs

- **One transaction per text** (all statements of one `execute` call in one transaction): what some drivers do for a multi-statement string; a different answer to "do the statements before the error stay?".
- **Statement-level atomicity inside an explicit transaction** (a savepoint around each statement): the failed statement is undone and the transaction continues. MySQL does this; PostgreSQL does not (see 4e-04).

## In BusTub

`BusTubInstance::ExecuteSql` runs a statement without a transaction; the shell's `begin` / `commit` / `abort` are handled by `ExecuteSqlTxn` with a transaction the *shell* manages ("only supported in managed txn mode, please use bustub-shell"). This stage is what that shell loop is, as a reusable object.

## The C/C++ way

| C / C++ | Rust |
|---|---|
| `Transaction *txn = txn_mgr->Begin(); ... txn_mgr->Commit(txn);` and a `catch` that aborts | `begin()?`, a call, then one `if ok { commit } else { abort }` |
| a raw pointer that may be null for "no transaction" | `Option<Arc<Transaction>>` |
| a bool return that means success | `Result<bool>`: `Ok(false)` is a failure the engine reports without an error |
| a `catch (Exception &)` per layer that rethrows | `?` and one `From` per layer, into the one error type of module 3k |

**Port rule:** every path out of a function that opened a transaction closes it; the compiler cannot check this, so structure the code so there is only one path.

## Learn more

- [PostgreSQL: transaction control](https://www.postgresql.org/docs/current/tutorial-transactions.html) · [Rust: lifetimes in structs](https://doc.rust-lang.org/book/ch10-03-lifetime-syntax.html#lifetime-annotations-in-struct-definitions)
