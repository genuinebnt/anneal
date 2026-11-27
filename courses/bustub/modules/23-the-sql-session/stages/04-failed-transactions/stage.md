Autocommit has an easy answer to an error: the statement was the whole transaction, so abort it. Inside `begin ... commit` it is harder. Two statements ran, the third failed. Were the first two meant to stay? Should the fourth run? PostgreSQL's answer is blunt and good: **the transaction is dead**. Every later statement is refused until the client says `rollback` (or `commit`, which also rolls back), because the client might otherwise commit half of what it meant to do.

The client has to be told which situation it is in, and a program cannot read English. "Your transaction lost a race, run it again", "your transaction is dead, say rollback" and "you made a mistake" each call for a different action, so each gets its own SQLSTATE code, through the `DbError` you built in module 3k. This stage gives the session those codes, and then puts the session where a person meets it: the shell. Until now `begin;` in the shell answered "only supported in managed txn mode"; after this stage the shell is one session, and a transaction spans everything you type.

> [!CHECK] Two clients see an error from the same `update`. Client A got "could not serialize access due to concurrent update", client B "division by zero". Both wrap the update in a loop that retries on error. What happens to each, and what should the loop have looked at instead of "is it an error"? Then: the shell prints the error for a person. What should it print, and where does the rest of the error go?
> ||A's transaction succeeds on a later attempt: it lost a race, and the race is over. B's loop retries for ever (or up to its limit), because a division by zero fails every time. The loop must look at the **code**: `40001` (serialization failure) is the one a client retries, and it is a different code from anything a mistake in the statement produces. Matching the words "conflict" or "could not" breaks the day someone rewords a message, and matches messages that only sound alike. The shell prints `ERROR <code>: <message>` (your `client_message`), which hides the inside of system failures; the whole chain of causes and the statement go to the server's log (standard error), where an operator can read what the client may not.||
>
> - Which of the three situations does running the transaction again fix?
> - Which part of an error is meant for programs, which for people?
> - What may a stranger on the other end of a connection learn from an I/O error's message?

## The task

Extend `Session` (in `src/common/session.rs`), your `DbError` (in `src/common/db_error.rs`) and the shell (`src/bin/bustub_shell.rs`):

- **The failed state.** A statement that fails inside an open transaction (an error, a syntax error included, or the engine's `Ok(false)` for a write conflict) **fails the transaction**: it is aborted at once and the session is *failed*: `is_failed()` is true and `in_transaction()` stays true. In autocommit an error leaves nothing failed. A write-write conflict between two sessions fails the one that lost, never the one that was first.
- **Refused until it ends.** While failed, every statement except `commit` and `rollback` (and `begin`, which warns as before) is refused with code **`25P02`** and the message `current transaction is aborted, commands ignored until end of transaction block`. `commit` of a failed transaction answers `ROLLBACK` and returns the session to idle; so does `rollback`. Nothing the failed transaction did is visible, including the statements before the error.
- **Conflicts are serialization failures.** A statement that loses a write conflict, and a commit that is refused, fail with code **`40001`**: the code a client answers by running the whole transaction again. No other error has it.
- **Codes come from the cause.** The session's own outcomes are a cause like any other: `DbError::code()` reads them from the root of the chain, so context on top never changes them. The two warnings of 4e-02 (`25P01`, `25001`) keep their codes too.
- **The shell is a session.** It runs everything typed into one session, so `begin;`, several statements over several lines, and `commit;` form one transaction. It prints each reply line; for an error it prints `client_message()` (`ERROR <code>: <message>`, the inside of system failures hidden), and writes to standard error, the server's log, the error's whole chain (`report()`) with its code and the statement that failed. With a script on standard input it prints no prompts (given).

> [!ASIDE] The steps, if you would rather not work them out
> 1. Give the transaction outcomes a small error type of your own (a conflict, a refused commit, a statement in a failed transaction), with `Display` and `std::error::Error`, and a code for each.
> 2. In `db_error.rs`: one `From` for it, like the other layers, and one line in `code()` that recognises it at the root.
> 3. In the session: at each place a statement fails, abort, record the failed state, then return the error; refuse statements in the failed state with the new error; make `commit` and `rollback` end the failed state with `ROLLBACK`.
> 4. In the shell: one `Session` for the whole run; print `execute`'s lines, or `client_message()` on stdout and the code, `report()` and the statement on stderr.

## Your freedom

The shape of the error type (one enum, a struct with a kind, or two types), its messages beyond the phrase the tests look for, whether the warnings use it, and the shape of the failed state (a flag next to the optional transaction, or an enum). What the log line looks like, as long as it holds the code, the chain and the statement. The tests use `execute`, `is_failed`, `in_transaction`, `code()`, `client_message()`, and the shell's two output streams.

## The Rust toolbox

**A cause of your own.** A new failure is one more type at the root of the chain: implement `Display` and `Error`, convert it with one `From`, and teach `code()` one `downcast_ref`. Module 3k's `XX000` default is what it got before you did, which is why the tests fail until then rather than panic.

**A state that is more than `Option`.** "Open and failed" and "open and healthy" and "closed" are three states; a `bool` next to an `Option` works until a path forgets to set it. An enum with `Failed` makes the `match` exhaustive.

**Early `return Err` with cleanup first.** In the failure path: abort, record the state, then `return Err(...)`. Writing the cleanup *after* the return is the bug the tests catch. `let _ = result;` is for an abort whose own failure you cannot do anything about.

**`println!` and `eprintln!`.** Two streams: a client reads one, the operator's log collects the other. `std::io::IsTerminal` tells the shell whether a person is typing.

## If this is new

- L8 Error design: an error that carries enough to act on, and errors that are states.
- [S1 Option and Result](/t/s1-option-result): an outcome with three cases.
- S6 Trait objects: `downcast_ref` on the root of the chain.

## Tests

- An error fails the transaction; later statements are refused with `25P02` and the reason.
- COMMIT and ROLLBACK end a failed transaction, both answering ROLLBACK; the earlier statements are gone.
- A syntax error fails it too; a write conflict fails only the loser, with `40001`; so does a refused commit.
- The shell: `begin; ...; commit;` over several inputs; an error shows its code, the failed state holds until ROLLBACK, and the log has the rest.

## Hints

### Fail at the point of failure

The place where a statement is known to have failed is the place to abort and to set the state: not at the next statement and not at `commit`. Then every later path sees a session whose state is already right. `parse(sql)?` returning early on a syntax error skips that place: handle the parse error there too when a transaction is open.

### A code is chosen where the meaning is known

The engine's `Ok(false)` and the transaction manager's refused commit carry no error at all; only the session knows they mean "lost a race". So the session is where the `40001` is made, as a typed cause, not as an `Exception` with the right words. A message that says "conflict" proves nothing: an internal error can say "could not commit" too, and retrying it does not help.

### Decide what the client sees once, at the edge

`client_message()` is the only text the shell shows for an error; the shell must not print `{e}` or `{e:?}` beside it, or the inside of a system failure leaks after all. Everything the client is not shown goes to the log in one place, and the log is where context pays off: the statement, and what the session was doing when the transaction manager failed.

## Performance

Aborting at once costs the same as aborting later and releases resources earlier, so there is no trade-off to find; the instructive number is how long a failed transaction *used* to hold things. Measure by what a writer on the same row sees: a failed transaction that is not aborted keeps blocking it. A code costs a walk to the root of a short chain and a few type checks, on the failure path only.

**Measure it.** Session A updates a row and then fails; session B updates the same row. With abort-at-failure B succeeds at once; with abort-at-rollback B is refused until A's client acts. How long is that in a script for the shell with a pause in it?

## Experiment

Optional. Predict first, then run.

1. **Let a failed statement be skipped and the transaction continue** (MySQL's statement-level rollback). Which test fails, and what half-applied state can a client now commit?
2. **Abort only at `rollback`.** Which test fails?
3. **Give a conflict the code of its old `Exception` (`XX000`).** What does the client see in the shell, and what would a retrying client do?
4. **Print `{e}` instead of `client_message()` in the shell.** Which test fails, and what would an I/O error now show a client?

## Other designs

- **Savepoints** (`savepoint a; ...; rollback to a`): the failed statement's effects are undone back to a named point and the transaction continues; the client chooses where to resume. The engine's undo logs make this natural.
- **Statement-level atomicity without a failed state** (MySQL, SQLite's default conflict handling): easier for clients, and a transaction can commit having skipped a statement.
- **Retry on the server** (for serialization failures): the database re-runs the whole transaction itself; needs the transaction to be a stored procedure, not a stream of statements.
- **Notices beside the result** (PostgreSQL's protocol): a warning travels as a separate message with its own severity and code, so a driver can log it without parsing reply lines.

## In BusTub

BusTub's shell prints the error and keeps the transaction (a tainted one keeps running statements that all fail); there is no failed *state*, and an error is a C++ exception's type and message, with no code. Marking a conflict taints the transaction (project 4); this stage makes that visible to the client in the PostgreSQL way, by state and by code.

## The C/C++ way

| C / C++ | Rust |
|---|---|
| `bool failed_` checked at the top of `Execute` | a state that the `match` forces you to handle |
| `try { ... } catch (const ExecutionException &) { Abort(); throw; }` | cleanup in the failure arm, then `return Err(e)` |
| `if (strstr(e.what(), "conflict")) retry();` | `if e.code() == "40001"`: a code, read from the root cause |
| `std::cerr << e.what()` for everyone | `client_message()` for the client, `report()` for the log |

**Port rule:** the state a client observes after an error, and the code it reads, are part of the interface: write them down and test them.

## Learn more

- [PostgreSQL: transactions in the tutorial](https://www.postgresql.org/docs/current/tutorial-transactions.html) · [PostgreSQL: SAVEPOINT](https://www.postgresql.org/docs/current/sql-savepoint.html) · [PostgreSQL: serialization failure handling](https://www.postgresql.org/docs/current/mvcc-serialization-failure-handling.html) · [PostgreSQL: error codes](https://www.postgresql.org/docs/current/errcodes-appendix.html)
