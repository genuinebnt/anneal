Until now the manager took any request. A real database has rules, and they are what the word **isolation** means. This stage adds the transaction's view: a `LockTxn` has an isolation level and a phase (growing, shrinking, committed, aborted), and `lock_table`, `lock_row`, `unlock_table`, `unlock_row` and `release_all` enforce what that level allows. A request that breaks a rule is refused with a reason, and the transaction is **aborted**.

## The task

In `src/concurrency/lock_manager.rs`:

- **Isolation levels.** Read uncommitted never takes shared locks (S, IS, SIX are refused). Locks may only be requested while growing, except that read committed may still take IS and S while shrinking. Repeatable read takes nothing once shrinking.
- **Phases.** Releasing an exclusive lock starts the shrinking phase at every level; releasing a shared lock does so only under repeatable read.
- **Rows.** A row can only be locked S or X, and only when the transaction holds a suitable table lock: any table lock for S, and IX, SIX or X for X.
- **Releasing.** A table cannot be released while rows of it are locked; releasing what is not held is an error; `release_all` frees everything (rows first) and leaves the phase alone, which is what commit and abort do.
- A violation aborts the transaction and says why (`LockErrorKind`). An aborted or committed transaction takes no locks.

The tests: one scenario per rule and per isolation level, upgrades through these methods, `release_all`, and a property: **for any sequence of lock and unlock requests at any isolation level, every answer and every change of phase equals what the rules (written out again in the test) say**.

## Your freedom

How you remember what each transaction holds (needed for the row rules); where the checks live; whether the raw `lock` and `unlock` of the earlier stages stay available.

## The Rust toolbox

**Errors that carry a reason.** `LockErrorKind` is an enum; every refusal is a value with a name, and the caller (and the test) can match on it.

**A check function that returns `Result<(), LockError>`.** All the rules about "may I ask for this now?" in one place; every `lock_*` method starts with `check()?;`.

**Interior mutability for the phase.** `LockTxn` keeps its state behind a `Mutex`, so the manager can change it through `&LockTxn`.

## If this is new

- L8 Error design: an error enum with reasons.
- L7 Enums & pattern matching: a `match` on isolation level and phase.

## Tests

- Read uncommitted and shared locks; shrinking after an exclusive release; shared releases per level.
- Read committed while shrinking; intention locks on rows; table locks that are enough; rows under a table.
- Unlock errors, upgrades and `release_all`.
- Property: the rules against a model, for any sequence at any level.

## Hints

### Write the rules as a table first

Rows: the three levels. Columns: growing, shrinking. Cells: which modes may be requested. Code that mirrors the table is easy to check.

### Which lock moves you to the shrinking phase?

Not every unlock does. List, per level, which modes do, then write one function.

### The row rules look at the table lock you hold

You need to know what a transaction holds, in the manager, before you can say "table lock not present".

## Performance

Each request does a few map lookups under the manager's mutex. The cost grows with the number of locks a transaction holds only for `release_all`.

**Measure it.** A transaction that takes IX on a table and X on ten thousand rows, then commits: `release_all` should be linear in the rows.

## Experiment

Optional. Predict first, then run.

1. **Let repeatable read take locks while shrinking.** Which test fails, and what anomaly would that allow in a real workload?
2. **Do not abort on a violation.** What state is the transaction left in, and which test shows it?

## Other designs

- **Strict 2PL** (keep every exclusive lock until commit) is what `release_all` at commit gives you; this stage lets you also release early.
- **Conservative 2PL** (take all locks at the start): no deadlocks, needs to know the lock set in advance.
- **Multi-version concurrency control** (modules 4a and 4b) gets isolation without readers taking locks.

## In BusTub

The 2022 lock manager project has exactly these checks in `LockTable` and `LockRow` (`LOCK_ON_SHRINKING`, `LOCK_SHARED_ON_READ_UNCOMMITTED`, `ATTEMPTED_INTENTION_LOCK_ON_ROW`, `TABLE_LOCK_NOT_PRESENT`, `INCOMPATIBLE_UPGRADE`, `ATTEMPTED_UNLOCK_BUT_NO_LOCK_HELD`, `TABLE_UNLOCKED_BEFORE_UNLOCKING_ROWS`).

## The C/C++ way

| C / C++ | Rust |
|---|---|
| `throw TransactionAbortException(id, AbortReason::LOCK_ON_SHRINKING)` after `txn->SetState(ABORTED)` | `Err(abort(txn, LockErrorKind::LockOnShrinking))` where `abort` sets the state |
| `txn->GetSharedTableLockSet()` | a map inside the manager, or a field you add |

**Port rule:** an abort means "set the state, then return the error", in one helper, so no path forgets half.

## Learn more

- [PostgreSQL: transaction isolation](https://www.postgresql.org/docs/current/transaction-iso.html) · [`std::sync::Mutex`](https://doc.rust-lang.org/std/sync/struct.Mutex.html)
