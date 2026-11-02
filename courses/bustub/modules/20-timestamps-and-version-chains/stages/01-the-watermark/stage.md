Multi-version concurrency control keeps several versions of a row so that readers and writers do not block each other: a reader sees the database as it was at its **read timestamp**, whatever writers have done since. Old versions pile up and must eventually be thrown away, but not while a running transaction could still read them. The **watermark** is the answer to "how old can a version be before nobody can read it?": the smallest read timestamp among the running transactions, or the last commit timestamp when nobody is running. A version that was replaced before the watermark is garbage. The structure behind it is small: a count of running transactions per read timestamp.

> [!CHECK] Transactions read at 3, 5 and 5. What is the watermark? The one at 3 finishes: what now? One of the two at 5 finishes: what now? Nobody is running and the last commit timestamp is 9: what is it? Why must `add_txn` refuse a read timestamp *older* than the last commit timestamp, and what structure makes `get_watermark` cheap even with a million running transactions?
> ||3; then 5; still 5 (one reader remains, so the count at 5 goes from two to one and the timestamp stays); then 9 (the watermark of an idle system is the last commit, because anyone starting now reads at it). A new transaction reads at the last commit timestamp, so a request for something older means a bug in the caller: it would read versions that garbage collection may already have freed. An ordered map from timestamp to count (a `BTreeMap`) gives the smallest key in `O(log n)`, and a count per timestamp makes duplicates cheap.||
>
> - What happens to a timestamp whose count reaches zero?
> - What does the watermark do when the *last commit* moves but readers remain?
> - Which timestamp does a transaction that begins right now get?

## The task

In `src/concurrency/watermark.rs`, `Watermark` has `commit_ts` (the timestamp of the last commit, kept up to date by the given `update_commit_ts`) and `current_reads` (an ordered map from read timestamp to the number of running transactions that have it). Write:

- `add_txn(read_ts)`: a transaction begins. If `read_ts < commit_ts` return an `Execution` error whose message is `read ts < commit ts`. Otherwise count one more reader at `read_ts`.
- `remove_txn(read_ts)`: a transaction ends. Count one fewer at `read_ts`; when nobody is left at that timestamp, forget the timestamp.
- `get_watermark()`: the smallest timestamp somebody reads at, or `commit_ts` when nobody does.

The tests: exact scenarios (no readers: the last commit; the smallest read timestamp; removing the smallest moves it up; removing another reader leaves it; two readers at one timestamp are counted; an older reader is refused; a million transactions in either order), and a property: **after any sequence of readers arriving, leaving and commits the watermark equals the minimum of the live read timestamps** (or the last commit when none), checked after every operation against a plain `Vec`.

## Your freedom

The container (`BTreeMap<Timestamp, usize>` is the intended one; a heap with lazy deletion or a sorted `Vec` also pass), and how you report the error.

## The Rust toolbox

**`BTreeMap` as an ordered multiset.** `*map.entry(ts).or_insert(0) += 1` counts; `map.keys().next()` or `map.first_key_value()` is the smallest key in `O(log n)`.

**Counting down and removing.** `if let Some(n) = map.get_mut(&ts) { *n -= 1; if *n == 0 { map.remove(&ts); } }`, or `Entry::Occupied` and `remove_entry`.

**An error value.** `Err(Exception::new(ExceptionType::Execution, "read ts < commit ts"))`.

**Mutability by `&mut self`.** The manager keeps the watermark behind a `Mutex`; the methods take `&mut self` because the manager holds the lock for you.

## If this is new

- [S4 Maps & sets](/t/s4-maps-sets): `BTreeMap`, `entry`, ordered iteration.
- L8 Error design: returning `Err` with a message.
- [C1 Threads & shared state](/t/c1-threads-shared-state): why a `Mutex` guards the watermark (the next stage uses it).
- The optional *watermarks and garbage collection* concept.
- Y5 Testing & verification: Build it: a model of versions; random histories and sessions.

## Tests

- No readers; smallest read timestamp; removal of the smallest and of another; duplicates; refusal of an older reader; a million transactions in both orders.
- Property: watermark equals the minimum of live readers after every operation.

## Hints

### The watermark is a derived value

Keep only the counts; compute the answer each time from the smallest key. A cached "current minimum" has to be repaired on every removal.

### Duplicates

Two transactions can begin between the same two commits and share a read timestamp. Removing one must not forget the timestamp.

## Performance

`add`, `remove` and `get` are `O(log n)` with a `BTreeMap`; the million-transaction test takes about a second in debug mode. A heap with lazy deletion has a cheaper `add` but a `remove` that must be repaired later.

**Measure it.** Time the million-transaction test with `BTreeMap` and with a sorted `Vec`; explain the difference.

## Experiment

Optional. Predict first, then run.

1. **Forget the count.** Store a `BTreeSet` of timestamps instead of counts. Which test fails?
2. **A stuck reader.** Add one reader at ts 0 and never remove it, then commit a thousand transactions. What does the watermark do, and what would garbage collection (module 4b) be unable to free?

## Other designs

- **Ordered map of counts (ours, BusTub's).**
- **Min-heap with lazy deletion.**
- **A per-thread minimum** and a global minimum of those (low-contention engines).
- **Epoch-based reclamation:** the same idea with epochs instead of timestamps.

## In BusTub

Project 4 of the 2025 course: `watermark.cpp`, `transaction_manager.cpp` (`Begin`, `Commit`, `Abort`), `execution_common.cpp` (`ReconstructTuple`, `CollectUndoLogs`, `GenerateNewUndoLog`, `GenerateUpdatedUndoLog`) and the transactional path of `seq_scan_executor.cpp`. BusTub's MVCC keeps the newest version in the table and the older ones as **undo logs**, each a delta that turns a version into the previous one, chained from the tuple.

## The C/C++ way

| C / C++ | Rust |
|---|---|
| `std::map<timestamp_t, int> current_reads_` | `BTreeMap<Timestamp, usize>` |
| `current_reads_.begin()->first` | `current_reads.keys().next()` |
| `throw Exception("read ts < commit ts")` | `Err(Exception::new(ExceptionType::Execution, ..))` |

**Port rule:** an ordered `std::map` is a `BTreeMap`; a thrown exception is an `Err`.

## Learn more

- [`BTreeMap`](https://doc.rust-lang.org/std/collections/struct.BTreeMap.html) · BusTub's [Project 4 page](https://15445.courses.cs.cmu.edu/fall2025/project4/)
