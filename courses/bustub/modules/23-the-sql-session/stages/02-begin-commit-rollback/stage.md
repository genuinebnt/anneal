Autocommit is a transaction per statement. The point of a transaction is the opposite: several statements that become visible **together**, or not at all. `begin` says "from now on, keep everything I do in one transaction"; the session has to remember it between calls, run later statements inside it, and end it at `commit` or `rollback`. The state this adds to your session is one optional transaction, and the hard part is every way a client can leave it in a state nobody planned for: a second `begin`, a `commit` with nothing open, a connection that simply vanishes.

> [!CHECK] A client runs `begin; update t set v = 0;` and then its connection drops. Nobody calls `rollback`. Describe what the database must do, and what would go wrong for *every other client* if it did nothing.
> ||It must abort the transaction when the session goes away: the changes disappear and the transaction is removed from the set of running ones. If it did nothing, the transaction would stay registered with the watermark for ever, so the oldest version it could still read could never be garbage collected: every row updated since then keeps its whole version chain, memory grows, and scans slow down. In Rust this is exactly what `Drop` is for: the session's destructor aborts the transaction it owns.||
>
> - What should `begin` do inside a transaction? What should `commit` do with none open?
> - Where does the transaction live between two calls of `execute`?
> - Is `commit` allowed to fail?

## The task

Extend `Session` (in `src/common/session.rs`):

- `begin` (or `begin transaction`, `start transaction`) opens a transaction with the session's default level and answers `BEGIN`. Statements after it run in that transaction and are **not** committed one by one.
- `commit` ends it, makes its changes visible and answers `COMMIT`. `rollback` (or `abort`) throws them away and answers `ROLLBACK`. After either, the session is idle again (autocommit).
- `begin` inside a transaction changes nothing and answers `WARNING: there is already a transaction in progress`. `commit` or `rollback` with none open answers `WARNING: there is no transaction in progress`. Neither is an error.
- `in_transaction()` says whether one is open.
- Dropping a session with a transaction open aborts it.

## Your freedom

How the open transaction is represented, and whether `begin` and friends are handled before the statement reaches the engine (they must be: the engine only says "managed txn mode"). The tests use `execute`, `in_transaction` and the watermark.

## The Rust toolbox

**`Option::take`.** `self.txn.take()` moves the transaction out of the session and leaves `None` behind: ending a transaction is a take, and a second `commit` finds nothing.

**`impl Drop`.** The destructor runs when the session goes out of scope, on every path including panics; abort there, and ignore the result (a destructor cannot fail).

**Matching on the AST.** `Statement::Begin { .. }`, `Statement::Commit`, `Statement::Rollback` come out of the parser already; the `match` in the session is where they stop being SQL.

## If this is new

- [L1 Ownership and moves](/t/l1-ownership-moves): `take` and a destructor that consumes a field.
- S7 Smart pointers: the transaction is shared between the session and the manager.

## Tests

- Changes are invisible to others until commit; own writes are visible.
- Rollback undoes inserts, updates and deletes.
- A transaction spans several calls; stray BEGIN, COMMIT and ROLLBACK only warn.
- A dropped session rolls back; transactions of two sessions do not interfere.

## Hints

### Handle the control statements first

Check for `begin`, `commit` and `rollback` before anything else: they are about the session, not the data. Everything else goes to the code from 4e-01, which now has two ways to get its transaction: the open one, or a new one it commits itself.

### Autocommit must not commit the open transaction

The statement runner from the last stage commits "its" transaction. Make sure it only does so for a transaction it created: committing the session's explicit one after every statement would make `begin` a no-op.

### Test the cleanup with the watermark

Another session commits after your `begin`; then the watermark is behind the last commit for as long as your transaction is open, and equal again when it ends.

## Performance

A transaction that stays open holds back garbage collection for the whole database: the cost of one slow client is paid by everyone, in memory and in scan time. Long transactions are the usual cause of "the table is bloated" in production. Cheap protection: a timeout that aborts a session's transaction after it was idle for too long.

**Measure it.** Open a transaction in one session and run 100 000 single-row updates in another; compare the version chain length (and a `select` over the table) with and without the open transaction.

## Experiment

Optional. Predict first, then run.

1. **Remove `Drop`.** Which test fails? Run `quiet` by hand to see what is left.
2. **Make `begin` inside a transaction commit the first one and start a new one** (as MySQL does). Which test fails, and why is that the surprising behaviour?

## Other designs

- **A real state enum** (`Idle`, `Active(txn)`, `Failed`): the exhaustive `match` makes forgotten transitions a compile error; the optional field and a flag you are using are the cheap version of the same thing.
- **Implicit transactions** (as SQL Server's `SET IMPLICIT_TRANSACTIONS ON`): the first statement starts a transaction that must be ended explicitly.

## In BusTub

BusTub's shell keeps a `txn` variable and passes it to `ExecuteSqlTxn`; `begin`, `commit` and `abort` are handled in the shell's loop, and a `\txn` command shows the current transaction. The session of this stage is that loop made into an object that tests can drive.

## The C/C++ way

| C / C++ | Rust |
|---|---|
| a destructor `~Session() { if (txn_) txn_mgr_->Abort(txn_); }` | `impl Drop for Session` |
| `txn_ = nullptr;` after committing | `self.txn.take()` |
| `std::cout << "WARNING: ..."` | a returned line: the session has no stdout |

**Port rule:** a resource that a client can abandon needs an owner whose destructor cleans it up.

## Learn more

- [PostgreSQL: BEGIN](https://www.postgresql.org/docs/current/sql-begin.html) · [Rust: the `Drop` trait](https://doc.rust-lang.org/std/ops/trait.Drop.html)
