Restart. The log holds every record that was flushed; the pages hold *some* of the changes, which ones nobody knows. Recovery's first two passes make that mess a known state. **Analysis** reads the log and decides who finished (a transaction with a `Commit` or an `Abort` record) and who did not (the **losers**, in the order they began). **Redo** then walks the log from the start and applies **every change**, in order, whether its transaction finished or not. After redo the pages are exactly as they were at the instant of the crash, whatever mixture they held before, because "put every logged change in place, in log order" gives the same final state from every starting point. That is *repeating history*, and it is what makes the next stage's undo well defined.

**The store.** The module's tests work on a small transactional **store** (`src/recovery/store.rs`): a few heap pages in the buffer pool holding fixed-size records of 16 bytes, each addressed by a `Rid`. Transactions insert, update and delete records; `Store::get` and `scan` read them. Changes are applied to the pages at once (no isolation: that was module 4a and 4b), and a record written by an active transaction cannot be written by another until the first one ends.

> [!CHECK] A transaction committed; the pages were never written. Another transaction changed a record, was still running at the crash, and its page *was* written. A third had its page written *and* its change redone already by an earlier, interrupted recovery. After redo, what does each page hold, and does the order in which you replay matter? Why does redo apply the *unfinished* transaction's change instead of skipping it?
> ||All three changes are in the pages, in the state the log describes (the last record for each slot wins). Order matters only within a slot: later records overwrite earlier ones, so replay in log order. Redo applies the unfinished change because the page may or may not hold it and cannot say which: applying everything is the one procedure that is right from every start. Removing the loser's effects is a separate, later step that works from the before-images in the log.||
>
> - What does analysis do with a transaction whose `Begin` it never reads?
> - Why can redo ignore `before`?
> - What would redo have to know to skip changes already on a page?

## The task

In `src/recovery/recover.rs` (`Recovery`, `Analysis` and `recover`, which runs analysis, redo, undo and a flush of every page, are given):

- `analyse(records: &[(Lsn, LogRecord)]) -> Analysis`: `redo_from` is `0` (stage 7 moves it); `finished` holds the transactions that have a `Commit` or `Abort` record (sorted); `losers` holds every other transaction that appears in the log, **in the order they first appear**. A transaction whose `Begin` is missing but that has changes still counts.
- `redo(store, records, from) -> StoreResult<usize>`: from `records[from]` on, **in order**, for every `Change` record put its slot in its `after` state (`Store::set_state`); return how many changes were applied.

(Stage 6 writes `undo`, so `recover` as a whole does not work yet; these stages' tests call `analyse` and `redo` directly.)

The tests: exact scenarios (a commit or an abort record means finished; losers are listed in the order they began; a change with no begin still belongs to a loser; redo puts back what no page ever saw, losers included; redo is harmless where the page already has the change; redo twice is redo once; redo starts where it is told to), and a property: **after any run (commits, rollbacks, transactions left open, page and log flushes) and a crash, `analyse` and `redo` bring the pages to exactly the state the durable log describes** (an independent replay of the records in the test), and doing it again changes nothing.

## Your freedom

How you collect the transactions (a `Vec` for order and a `HashSet` for membership, or a map with a first-seen index) and whether `redo` skips records that are already in place (it need not).

## The Rust toolbox

**Order plus membership.** `Vec<TxnId>` keeps first-seen order; `HashSet<TxnId>` answers "did it finish?" in `O(1)`; `seen.contains(&t)` on the `Vec` is fine for the sizes in the tests.

**Pattern matching the enum.** `match record { LogRecord::Begin { txn } | LogRecord::Change { txn, .. } => .., LogRecord::Commit { txn } | LogRecord::Abort { txn } => .., LogRecord::Checkpoint { .. } => .. }`: or-patterns bind the same name in several variants.

**Slicing.** `&records[from..]` is the tail without a copy; iterate it with `for (_, record) in ..`.

**`if let` on one variant.** `if let LogRecord::Change { rid, after, .. } = record { store.set_state(*rid, after)?; }` ignores every other record.

**`?` through `StoreResult`.** An error from `set_state` aborts recovery with the cause.

## If this is new

- L7 Enums & pattern matching: or-patterns, `..` in struct patterns.
- [S4 Maps & sets](/t/s4-maps-sets): `HashSet`, keeping first-seen order with a `Vec`.
- Y5 Testing & verification: an independent model of what redo must produce.
- The optional *ARIES recovery* concept.
- L8 Error design: Errors at scale: a recovery that never panics on a damaged log; `io::Result` and `?`.

## Tests

- Analysis: finished, losers and their order, changes without begins, an empty log.
- Redo: puts back changes no page saw; harmless on pages that have them; idempotent; honours `from`.
- Property: redo reproduces the state the durable log describes, from any page state.

## Hints

### Do not look at `before` in redo

Redo is "apply the `after` state". The `before` is for undo.

### A loser is anyone unfinished

A transaction that began, changed things and has neither `Commit` nor `Abort` is a loser, whether or not its begin is in the part of the log you read.

### Replaying twice

Because each record sets a slot to a state, replaying a log twice leaves the same state: that is the test for correctness of your `set_state`, not of your `redo`.

## Performance

Redo is one pass over the log: cost proportional to its length (hence checkpoints, stage 7). Real systems make it fast with parallel redo per page (records for different pages are independent) and by skipping records whose page LSN is already at or beyond the record's.

**Measure it.** Build a log of a million changes over 1 000 pages and time `redo` from the start; then with half the pages already up to date.

## Experiment

Optional. Predict first, then run.

1. **Skip the losers.** Make redo ignore changes of unfinished transactions. What state results, and which test shows why that is wrong?
2. **Redo backwards.** Replay in reverse order. Which test fails first?

## Other designs

- **Repeat history in log order (ours, ARIES).**
- **Redo only the winners** (some no-undo designs): needs the pages to contain no uncommitted data.
- **Parallel redo by page** after a single analysis pass.
- **Log-structured databases:** the log *is* the data; recovery is rebuilding the in-memory index.

## In BusTub

BusTub's 2025 projects stop before recovery: Project 4 is concurrency control. The lectures on logging and recovery (CMU 15-445) teach the write-ahead log and ARIES, and BusTub's older years had a `LogManager` and `LogRecovery` that students filled in. This module is new in this course: a small store that logs before it writes, and recovery you can crash at every step.

## The C/C++ way

| C / C++ | Rust |
|---|---|
| `std::unordered_set<txn_id_t> active_txn_` | `HashSet<TxnId>` and a `Vec` for order |
| a `switch (record.type_)` | `match record { .. }` with every variant listed |
| `for (auto &r : log_records)` | `for (_, r) in &records[from..]` |

**Port rule:** a `switch` on a tag with a default becomes a `match` with no wildcard if you can: the compiler then tells you when a new record kind is not handled.

## Learn more

- ARIES paper, section "Restart processing" · CMU 15-445 lecture "Database recovery"
