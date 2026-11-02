Every update leaves a log; every transaction stays in the transaction manager's map as long as its logs might be read. Nothing ever shrinks unless somebody works out what is no longer needed. This stage writes that somebody. In BusTub it is **stop-the-world**: you may assume no transaction is executing a statement while it runs.

> [!CHECK] The watermark is 10. A tuple with timestamp 15 has an undo chain: a log owned by T1 (ts 12), then one owned by T2 (ts 9), then one owned by T3 (ts 4). Which transactions must garbage collection keep, and why does it stop where it does?
> ||T1 and T2. The log at 12 is newer than the watermark, so a reader may need it; the log at 9 is the first at or below the watermark, i.e. the oldest version anybody can still ask for, so the walk includes it and stops. T3's log can never be needed.||
>
> - Which version does a reader at timestamp 10 get?
> - What does the oldest reader need: logs newer than itself, and one more?
> - What if the tuple's own timestamp were 8?

## The task

In `src/concurrency/transaction_manager.rs`, `garbage_collection(&self)`, the region marked `4b-05`.

`garbage_collection` leaves in `txn_map` exactly the transactions that are `Running` or `Tainted`, or that own an undo log some reader could still need. A tuple stamped at or below the watermark needs no chain: its link is cleared. For any other tuple the chain is followed from the head, noting each log's **owner** as needed, up to and including the first log at or below the watermark (the oldest version anybody can ask for), or until a log cannot be fetched. Every other transaction is dropped.

> [!ASIDE] The steps, if you would rather not work them out
> 1. `watermark = self.get_watermark()`.
> 2. For every table in the catalog (`catalog.get_table_names()`, `get_table(name)`), for every tuple (`make_eager_iterator`): read its metadata and head link (`get_tuple_and_undo_link`).
>    - If the tuple's timestamp is at or below the watermark, no reader can need anything below it: clear its link (`update_undo_link(rid, None, None)`) and go on.
>    - Otherwise walk its chain from the head: for each log note the **transaction that owns it** as *needed*; stop after the first log whose `ts` is at or below the watermark (it is the oldest version anybody can ask for). A log that can't be fetched ends the walk.
> 3. Keep in `txn_map` exactly the transactions that are `Running` or `Tainted`, or *needed*; drop the rest.

The tests: exact scenarios (a finished transaction without logs is forgotten; one whose logs a reader may still need stays; only the logs below the oldest needed version go; tainted and running transactions are never collected; collecting twice changes nothing and every snapshot still reads right), and a property: **the same sessions with a garbage collection at random moments**: collecting never changes what any running transaction (or a new one) sees.

## Your freedom

How you walk the tables and chains, and what you keep per tuple while walking; the set of kept transactions is fixed.

## The Rust toolbox

**A set of needed owners.** `let mut needed: HashSet<TxnId> = HashSet::new();` filled while you walk the chains.

**`retain`.** `map.retain(|id, t| keep(t) || needed.contains(id))` removes the others in one pass under the write lock.

**Eager iteration.** `table.table.make_eager_iterator()` visits every tuple including those added during the walk, which is fine here (nothing runs).

```rust
let watermark = self.get_watermark();
let mut needed: HashSet<TxnId> = HashSet::new();
let catalog = self.catalog.read().unwrap();
for name in catalog.get_table_names() {
    let Some(table) = catalog.get_table(&name) else { continue };
    let mut iter = table.table.make_eager_iterator();
    while !iter.is_end() { let rid = iter.get_rid(); iter.advance(); ... }
}
self.txn_map.write().unwrap().retain(|id, t| matches!(t.state(), Running | Tainted) || needed.contains(id));
```

## Design notes

**Needed means reachable at or above the watermark.** The chain from a tuple whose timestamp is above the watermark leads, newest first, through versions some running reader may want. The reader with the smallest read timestamp (the watermark) wants the first version with `ts <= watermark`; everything older is garbage. All of the logs *up to and including* that one are needed; the transactions that own them cannot be forgotten.

**One transaction, many chains.** A transaction owns logs in many chains. It is needed if *any* of them is. That is why you collect a set across all tuples first and delete afterwards.

**Running and tainted stay for their own reasons.** They may yet write, or abort and need their logs; and their *read* state matters to the watermark.

**What is not freed.** Tombstones stay in the table. Real systems also reclaim table space and index entries after garbage collection (PostgreSQL's `VACUUM`); this exercise only forgets transactions and chain links.

## If this is new

- [S4 Maps & sets](/t/s4-maps-sets): `HashSet`, `retain`.
- S6 Iterators: an iterator you advance by hand.
- The optional *watermarks and garbage collection* concept.
- Y5 Testing & verification: Build it: sessions of interleaved transactions against a model; an oracle that tries every serial order.

## Tests

- Forgetting finished transactions; keeping needed ones; cutting a chain below the oldest needed version; never collecting running or tainted ones; idempotence.
- Property: random sessions with collections change nothing for any reader.

## Hints

### The first log at or below the watermark is included

Stop *after* adding its transaction to the set, not before. The reader at the watermark needs it.

### Collect first, delete second

A transaction is needed if any chain still reaches it. Deleting while walking would forget transactions whose logs are reached by a chain visited later.

### A tuple at or below the watermark ends its chain

Its timestamp is already visible to every possible reader, so even its head link is dead weight. Clear it so that no link dangles to a forgotten transaction.

## Performance

One pass over every tuple of every table, plus chain walks that stop at the watermark: linear in the size of the database (plus the length of the chains still in use). That is why real systems collect per page in the background; the stop-the-world version is a stand-in for the algorithm, not a way to run a server.

**Measure it.** With 20 000 rows each updated ten times, one `garbage_collection()` call takes about 6 ms (release mode): a pass over the table and the short chains. Time it again with a long-running old reader holding the watermark back, and see how many transactions survive.

## Experiment

Optional. Predict first, then run.

1. **Off by one.** Stop the walk one log earlier (`<` instead of `<=`). Which test notices?
2. **Collect while running.** Run `garbage_collection` while a thread executes statements. What can go wrong?

## Other designs

- **Stop-the-world (ours, BusTub's).**
- **Background vacuum** (PostgreSQL's autovacuum) and **cooperative** collection by readers.
- **Epoch-based reclamation:** retire versions to an epoch and free them when no thread is in an older epoch.

## In BusTub

`transaction_manager.cpp`: "`/** @brief Stop-the-world garbage collection. Will be called only when all transactions are not accessing the table heap. */ void TransactionManager::GarbageCollection() { UNIMPLEMENTED("not implemented"); }`". `TxnExecutorTest.GarbageCollection` ("A: first GC", "B: second GC (yes, we call it twice without doing anything)", "C" to "F") and `GarbageCollectionWithTainted` run it with `EnsureTxnGCed` / `EnsureTxnExists`.

## The C/C++ way

| C / C++ | Rust |
|---|---|
| `std::unordered_set<txn_id_t>` and erasing from `txn_map_` in a loop | `HashSet<TxnId>` and `HashMap::retain` |
| a shared lock on the catalog while iterating tables | `self.catalog.read().unwrap()` held for the whole walk |

**Port rule:** "erase while iterating" in C++ is `retain` in Rust: one pass with a predicate, no iterator invalidation.

## Learn more

- [`HashMap::retain`](https://doc.rust-lang.org/std/collections/struct.HashMap.html#method.retain) · [PostgreSQL: routine vacuuming](https://www.postgresql.org/docs/current/routine-vacuuming.html) · [Garbage collection in Wu et al.](https://www.vldb.org/pvldb/vol10/p781-Wu.pdf)
