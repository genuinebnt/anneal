A transaction in this engine has three timestamps. Its **read timestamp** (the last commit when it began) decides which versions it sees. Its **temporary timestamp** (`2^62` + its id) marks the tuples it has written but not committed: bigger than any commit, so nobody else reads them, and unique to the writer, so it recognises its own work. Its **commit timestamp** is assigned at commit, one more than the last, and **replaces the temporary timestamp** in every tuple the transaction wrote, which publishes all its changes at one instant. `begin` and `commit` are small, and both are about taking the right timestamp at the right moment.

> [!CHECK] `begin` reads the last commit timestamp and then registers it with the watermark. If another transaction commits between the two steps, what does the watermark do, and what could garbage collection then free that this transaction still needs? How do you prevent it? In `commit`, the order is: stamp the tuples, then tell the watermark, then store the new last commit, then remove your own reader. Why not store the last commit first?
> ||Between the two steps the commit moves the watermark's commit timestamp past the timestamp this transaction is about to read at; `add_txn` then fails ("read ts < commit ts") or, worse, the watermark was already computed without this reader and garbage collection frees versions it needs. Hold the watermark's lock across both steps (read the timestamp *inside* the critical section). In commit, if `last_commit_ts` were published before the tuples are stamped, a transaction beginning now would read at the new timestamp and look for versions that are still marked with the temporary timestamp: it would miss the changes (they would appear uncommitted). The tuples are stamped first so that "the last commit" never names a commit whose data is not yet in place.||
>
> - What state is a transaction in after an abort, and after a commit?
> - What does a tainted transaction's commit return?
> - What is the read timestamp of the first transaction in a new database?

## The task

In `src/concurrency/transaction_manager.rs`:

`begin(isolation_level)`: the first lines (given) take the next id (starting at `2^62`, `TXN_START_ID`), create the `Transaction` and put it in `txn_map`. Write the part marked `4a-02`: take the last commit timestamp, set it as the transaction's read timestamp (`txn.set_read_ts(..)`), register it with `running_txns` (the watermark), propagating its error; **the two steps happen while holding the watermark's lock**.

`commit(txn)` (the checks are given: one commit at a time via `commit_mutex`, a tainted transaction returns `Ok(false)` and stays tainted, any other state but `Running` is an error). Write the region marked `4a-02` in it: a committed transaction gets `commit_ts = last_commit_ts + 1`. Every tuple in its write sets (under that page's write latch) is stamped with `ts = commit_ts` and the **same** `is_deleted` it had. The transaction then records its commit timestamp and becomes `Committed`. Last, under the watermark's lock and in this order: the watermark learns the commit timestamp, `last_commit_ts` is stored, and the transaction's read timestamp is removed from the watermark.

> [!ASIDE] The steps, if you would rather not work them out
> 1. `commit_ts = last_commit_ts + 1`.
> 2. For every `(table oid, rids)` in `txn.write_sets()`: look the table up in the catalog and, with `table.table.with_page_mut(rid, ..)` (the page's write latch), set the tuple's metadata to `ts = commit_ts` and the **same** `is_deleted`.
> 3. `txn.set_commit_ts(commit_ts)` and `txn.set_state(Committed)`.
> 4. Under the watermark's lock, in this order: `update_commit_ts(commit_ts)`, store `last_commit_ts`, `remove_txn(txn.read_ts())`.

`abort(txn)`: a `Running` or `Tainted` transaction becomes `Aborted` and its read timestamp is removed from the watermark. (Given: any other state is an error. Restoring the tuples an aborted transaction wrote is module 4b's job.)

The tests: exact scenarios (ids start at `2^62` and count up; a new transaction is running and has not committed; in a new database everything reads at 0; the manager remembers the transaction; a running transaction is registered with the watermark; commit timestamps count up from 1; a transaction that begins after a commit reads it; commit stamps the write set and leaves other tuples alone; a tainted transaction cannot commit; committing twice is an error; commit moves the watermark; abort releases the read timestamp; a tainted one can be aborted but a finished one cannot), and a property: **any interleaving of begin, commit and abort** keeps ids unique, read timestamps equal to the last commit at begin, commit timestamps 1, 2, 3 in commit order (aborts take none), and the manager's watermark equal to the smallest read timestamp of the running transactions.

## Your freedom

How you take the locks (`running_txns.lock()` for the whole critical section is the intended design) and how you walk the write sets.

## The Rust toolbox

**A `Mutex` guard as a critical section.** `let mut running = self.running_txns.lock().unwrap();` holds the lock until `running` is dropped at the end of the scope, so everything in between is one atomic step.

**Atomics for the counters.** `self.last_commit_ts.load(Ordering::SeqCst)` / `store(..)`; `next_txn_id.fetch_add(1, SeqCst)` hands out ids without a lock.

**Interior mutability on `Arc<Transaction>`.** `txn.set_read_ts(..)`, `set_state(..)`: the transaction has `&self` methods that change `Mutex`/atomic fields, because many threads share it through `Arc`.

**A page closure.** `table.table.with_page_mut(rid, |page| ...)` runs your code under the page's write latch and returns its value.

**Iterating a `HashMap<TableOid, HashSet<Rid>>`.** `for (oid, rids) in txn.write_sets() { let table = catalog.table_info(oid)?; for rid in rids { ... } }`.

## If this is new

- [C1 Threads & shared state](/t/c1-threads-shared-state): `Mutex`, lock scope, atomics.
- S7 Smart pointers & interior mutability: `Arc<Transaction>` with `&self` setters.
- L8 Error design: `Result` through `begin`.
- The optional *snapshot isolation* concept.
- [C3 Atomics & lock-free](/t/c3-atomics-lock-free): Understand it: atomic counters for timestamps and ids.
- Y5 Testing & verification: Build it: a model of versions; random histories and sessions.

## Tests

- Ids and state of new transactions; the manager remembers them; the watermark knows the running ones.
- Commit timestamps; a later transaction reads a commit; the write set is stamped and nothing else; tainted and double commits; commit and abort move the watermark.
- Property: random begin/commit/abort sequences against a model of ids, read timestamps, commit timestamps and the watermark.

## Hints

### Take the timestamp inside the lock

`let mut running = lock(); let read_ts = last_commit_ts.load(); running.add_txn(read_ts)?;` is correct. Reading `last_commit_ts` before locking is the bug the stage's question is about.

### Stamp before publishing

Tuples first, `update_commit_ts` and `last_commit_ts` after, your own reader last. The reader removal is last because the watermark must not jump past your own read timestamp before your commit is visible.

### The deleted flag

A commit changes only the `ts` of a tuple; a tuple deleted by the transaction must stay deleted with the commit timestamp.

## Performance

Commits are serialised by one mutex (`commit_mutex`): the throughput ceiling of this design is how fast one thread can stamp a write set. Real engines allocate commit timestamps with an atomic counter and allow commits to overlap.

**Measure it.** Commit a million empty transactions; then transactions with 100 writes each, and note where the time goes.

## Experiment

Optional. Predict first, then run.

1. **Race on purpose.** Read `last_commit_ts` before taking the watermark lock in `begin` and run a stress test with many threads beginning and committing. Does anything fail?
2. **Publish first.** Store `last_commit_ts` before stamping the tuples. Which scan test notices (stage 5)?

## Other designs

- **Locks around begin and commit (ours).**
- **Atomic timestamp counters** and a lock-free list of running transactions.
- **Hybrid logical clocks** across machines.
- **Commit timestamps assigned lazily** at the first read of an uncommitted tuple (some engines).

## In BusTub

Project 4 of the 2025 course: `watermark.cpp`, `transaction_manager.cpp` (`Begin`, `Commit`, `Abort`), `execution_common.cpp` (`ReconstructTuple`, `CollectUndoLogs`, `GenerateNewUndoLog`, `GenerateUpdatedUndoLog`) and the transactional path of `seq_scan_executor.cpp`. BusTub's MVCC keeps the newest version in the table and the older ones as **undo logs**, each a delta that turns a version into the previous one, chained from the tuple.

## The C/C++ way

| C / C++ | Rust |
|---|---|
| `std::lock_guard<std::mutex> lock(running_txns_.mutex_)` | `let mut running = self.running_txns.lock().unwrap();` |
| `std::atomic<timestamp_t> last_commit_ts_` | `AtomicI64` with `load`/`store` |
| `txn->SetState(TransactionState::COMMITTED)` | `txn.set_state(TransactionState::Committed)` |
| `std::shared_ptr<Transaction>` | `Arc<Transaction>` |

**Port rule:** a lock guard object is a `MutexGuard` whose scope is the critical section; `shared_ptr` is `Arc`.

## Learn more

- [`Mutex`](https://doc.rust-lang.org/std/sync/struct.Mutex.html) · [`Ordering`](https://doc.rust-lang.org/std/sync/atomic/enum.Ordering.html) · BusTub's [Project 4 page](https://15445.courses.cs.cmu.edu/fall2025/project4/)
