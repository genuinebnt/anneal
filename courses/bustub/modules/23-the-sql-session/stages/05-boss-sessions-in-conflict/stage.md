Nothing new to write. The last stage of the module runs your sessions against each other, as clients: a bank of threads moving money with retries, a reader that must never see half a transfer, two clients incrementing one counter, two doctors going off call. The guarantees these check are the ones the lab of module 4d (the anomaly lab) showed on a toy: snapshot isolation prevents lost updates and dirty reads and allows **write skew**; serializable prevents that too. Here they are properties of *your* transaction manager, reached through the SQL front end, and a failure points at one of five layers: the session, the parser, the engine's conflict detection, the version chains, or the commit validation.

## The task

Make the tests of this stage pass. They run threads or interleave sessions deterministically:

- **A bank:** four threads move money between four accounts with retry on conflict; the total never changes, conflicts really happen (a test that never conflicted proves nothing), and nothing is left running.
- **A consistent reader:** while transfers run, a snapshot transaction sees a total of exactly 400, twice in a row.
- **No lost update:** two transactions read a counter and write it back; one is rolled back.
- **Write skew:** two doctors each check that the other is on call and go off call. At snapshot isolation both commit and nobody is on call; at serializable exactly one commit is refused.
- **Cleanup:** after sessions fail, abandon transactions and drop, a garbage collection leaves the data intact and no transaction registered.
- **A property:** a random script of `begin`, `commit`, `rollback` and updates agrees with a model that applies an update only when its transaction commits.

## Your freedom

Everything. A failure here is a bug in one of the four stages before, or in 4a and 4b.

## The Rust toolbox

**Retry loops.** `while transfer(...).is_err() { ... }` is the client's half of the contract: a conflict is an expected outcome, not a crash. Bound the loop in real code.

**Reading a hang.** A test that does not finish names a transaction that is never ended: a session that did not abort on a failure path, or a `commit` that returned early. The watermark and `in_transaction()` tell you which.

## If this is new

- Y5 Testing & verification: a model that applies effects only at commit.
- [C1 Threads and shared state](/t/c1-threads-shared-state): sharing one database between sessions.

## Tests

- The bank: nothing lost, conflicts happen, nothing left running.
- A reader sees consistent totals during transfers.
- Lost updates are prevented; write skew passes snapshot and fails serializable.
- Cleanup after failures; a random script against a model.

## Hints

### A conflict that never happens means a missing check

If the bank test reports no conflicts, writes are not being checked against concurrent ones: look at the write-write check in 4b's update path before looking at the session.

### Totals that are off by an amount you recognise

A total off by exactly one transfer's amount is a half-applied transaction: either the abort path left a version, or the commit path published one half. Run a single-threaded bank first to see which.

## Performance

The bank workload does a few hundred small transactions and finishes in well under a second. Retries are cheap at four threads and four accounts; they dominate when many threads fight over few rows. The number to watch is conflicts per committed transfer.

**Measure it.** Count conflicts for 2, 4 and 50 accounts: more accounts, fewer collisions. Then run the same bank with serializable instead of snapshot isolation and compare the abort rates.

## Experiment

Optional. Predict first, then run.

1. **Run the bank at `read uncommitted`.** Does the total still hold? Does the reader's consistent total?
2. **Add a deliberate sleep between the two updates of a transfer.** How does the conflict count change and why?

## Other designs

- **Pessimistic sessions:** run the same workloads on the lock manager of 4d with strict two-phase locking: no write-skew anomaly at repeatable read, deadlocks instead of aborts, and readers that block.
- **Hybrid** (4d's snapshot reads with row locks for writes): writers wait rather than abort; the challenge 4d-c6.

## In BusTub

BusTub's grading for project 4 runs transactions through `bustub-shell`-like drivers (`txn_*_test`) in the same style: interleave a few sessions, check what each sees and what commits. This stage's tests are of that kind, but speak SQL.

## The C/C++ way

| C / C++ | Rust |
|---|---|
| `std::thread` and a mutex-protected counter for conflicts | threads and an `AtomicUsize` |
| a test that sleeps and hopes | deterministic interleaving by calling sessions in an order, and threads only where concurrency is the point |

**Port rule:** a concurrency test records what happened and checks a property of the record, not timing.

## Learn more

- [PostgreSQL: concurrency control](https://www.postgresql.org/docs/current/mvcc.html) · [Jepsen: consistency models](https://jepsen.io/consistency)
