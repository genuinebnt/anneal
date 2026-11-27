A transaction that read a row and now wants to change it holds S and wants X. It must not give up its read lock first (somebody could change the row in between) and it must not wait behind everyone who queued in the meantime (it already holds a lock on the thing). So an **upgrade** replaces the lock it holds, goes ahead of the waiting requests, and waits only for the *other holders* to leave. And only one transaction per lock may be upgrading at a time: two transactions that each hold S and each wait to upgrade would wait for each other for ever.

> [!CHECK] T1 and T2 both hold S. T1 asks to upgrade to X. Whom does it wait for? Then T2 asks to upgrade to X: what happens, and why refuse instead of queueing it?
> ||T1 waits only for T2's S. T2 gets `UpgradeConflict` at once, and T1's upgrade still goes through when T2 lets go. Queueing T2 would create a two-cycle (each waits for the other's S) that only the deadlock detector could break, by aborting someone.||
>
> - Which holders conflict with T1's X?
> - Draw the waits-for edges if T2 were queued.
> - What does each need from the other?

## The task

Extend `LockManager::lock` in `src/concurrency/lock_manager.rs`:

- Asking for the mode you already hold changes nothing. Asking for a stronger mode is an upgrade: it replaces the held mode once every other holder is compatible with it.
- An upgrade goes ahead of every request that is not itself an upgrade, however long they have been waiting.
- A second transaction that asks to upgrade the same lock while one is pending gets an `UpgradeConflict` error at once; the first upgrade carries on.
- A weaker (or unrelated) mode than the one held is an `IncompatibleUpgrade` error and changes nothing.

The tests: a lone holder upgrades at once; asking again changes nothing; an upgrade waits for the other holders; **an upgrade is granted before a writer that was already waiting**; a second upgrade is refused and the first still works; a weaker mode is refused; a queue still works after an upgrade in the middle of it.

## Your freedom

How the pending upgrade is remembered, and whether the old lock stays among the holders until the new one is granted or is swapped out. Whichever you choose, the transaction must never be left without a lock it had.

## The Rust toolbox

**Ordering a queue.** An upgrade is inserted at the front of the waiting list instead of the back (`Vec::insert(0, ..)` or a separate slot).

**An `Option<TxnId>` for "who is upgrading".** A second upgrade checks it and fails fast.

## If this is new

- [C1 Threads & shared state](/t/c1-threads-shared-state): reasoning about wake-ups.

## Tests

- A lone upgrade, a repeated request, an upgrade that waits.
- An upgrade ahead of a waiting writer; a conflicting second upgrade; a weaker mode.

## Hints

### Remove yourself from the question

When an upgrade asks "is anyone in the way?", the answer must ignore the asker's own old lock.

### Keep the old lock until the new one is granted

If the old lock is released first, another writer can slip in between and the whole point of the upgrade is lost.

## Performance

Upgrades are rare relative to plain requests; the cost is one more check per request. The cost that matters is the abort rate: upgrading S to X on a hot row aborts one of two racing transactions, which is why systems offer `SELECT … FOR UPDATE` (take X at the start).

**Measure it.** Two threads that read-then-write the same row, with and without taking X first: count the aborts.

## Experiment

Optional. Predict first, then run.

1. **Queue the upgrade at the back.** Which test fails?
2. **Allow two pending upgrades.** What does the test that expected `UpgradeConflict` show instead, and why can it hang?

## Other designs

- **Release then re-lock** (what a naive implementation does): simple and unsafe; another writer can change the row between.
- **Update locks** (a mode that says "I will upgrade", compatible with S but not with another update lock): avoids upgrade deadlocks without aborting; used by SQL Server.

## In BusTub

BusTub's `LockTable` and `LockRow` implement upgrades the same way: only one `upgrading_` transaction per queue, the upgrade request goes before the first waiting request, and a second one aborts with `UPGRADE_CONFLICT`.

## The C/C++ way

| C / C++ | Rust |
|---|---|
| `queue->upgrading_ = txn->GetTransactionId();` | `q.upgrading = Some(txn)` |
| `throw TransactionAbortException(txn_id, AbortReason::UPGRADE_CONFLICT)` | `return Err(LockError::new(txn, LockErrorKind::UpgradeConflict))` |

**Port rule:** an abort is a returned error the caller must deal with, not an exception that skips cleanup.

## Learn more

- [`Vec::insert`](https://doc.rust-lang.org/std/vec/struct.Vec.html#method.insert) · [PostgreSQL: row-level locks](https://www.postgresql.org/docs/current/explicit-locking.html#LOCKING-ROWS)
