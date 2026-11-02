The last stage is the lock manager used the way a database uses it: many transactions at once, taking table and row locks in random order, deadlocking, being aborted and retrying. There is nothing new to write. The tests run workloads and check what a correct lock manager guarantees.

## The task

Make the tests of this stage pass. They are workloads, not unit tests:

- **A bank.** Six threads move money between five accounts, locking the two accounts in random order, under strict two-phase locking with the deadlock detector running. Every transfer commits in the end (a victim retries); **no money is lost**; and **the history of the committed transactions is conflict-serializable**: the test records every row access in global order and checks that the precedence graph has no cycle, which is exactly "equal to some one-at-a-time order".
- **Deadlocks really happen.** Three accounts, six threads, random order: at least one deadlock occurs, and every one is resolved. A test that never deadlocked would prove nothing.
- **Mutual exclusion.** Eight transactions take X on the same row over and over; never two inside at once.
- **Readers and a table scan.** Shared row locks coexist; a whole-table S lock keeps writers (IX) out until it is released.
- **A row deadlock** between two transactions: the victim's `lock_row` fails with `Deadlock`, its transaction is aborted, the survivor goes on.

## Your freedom

Everything. These tests exercise the interface from the earlier stages; a bug found here is a bug in one of them.

## The Rust toolbox

**Reading a failure.** A history that is not serializable names no culprit; reproduce with fewer workers and add logging of lock requests and grants, in order.

## If this is new

- Y5 Testing & verification: checking a system by the history it leaves.

## Tests

- The bank: nothing lost, serializable.
- Deadlocks happen and are resolved.
- Mutual exclusion, readers against a scan, a row deadlock.

## Hints

### A lost update in the bank means a lock was granted twice

Check the mutual-exclusion test first: if two transactions can be inside the same row, no workload can pass.

### A hang in a workload is a missing wake-up or a deadlock the detector missed

Run the deadlock tests of the last stage on their own; then look at who is waiting (`waiting`) when it hangs.

## Performance

The workloads use a handful of threads and a few hundred transfers; they should finish in a couple of seconds. If they take far longer, look at how long a victim waits for the detector (its interval) and at how many times a transfer is retried.

**Measure it.** Count aborts per committed transfer for 3, 5 and 50 accounts: more accounts, fewer collisions.

## Experiment

Optional. Predict first, then run.

1. **Release each row lock right after using it** (not strict 2PL). Does the bank still lose no money? Does the history stay serializable? What does the check say?
2. **Take both account locks in id order.** How many deadlocks are left, and what does that say about prevention versus detection?

## Other designs

- **MVCC** (modules 4a and 4b) runs the same bank with readers that never wait and writers that abort on conflict instead of waiting.
- **Hybrid:** MVCC snapshots for reads and row locks for writes, as PostgreSQL and InnoDB do.

## In BusTub

The 2022 course's lock manager grading tests are workloads of this kind (`lock_manager_test`, `deadlock_detection_test`): threads that take locks in orders that deadlock, and checks that the detector breaks every cycle.

## The C/C++ way

| C / C++ | Rust |
|---|---|
| a test with `std::thread`s and `sleep_for` | a test with `thread::spawn` and a watchdog that fails instead of hanging |

**Port rule:** a concurrent test records what happened and checks a property of the record, not the timing.

## Learn more

- [Serializability (Wikipedia)](https://en.wikipedia.org/wiki/Serializability) · [Jepsen: Elle](https://github.com/jepsen-io/elle)
