For an **equality** join the nested loop does `n × m` work that a hash table avoids: **build** a table from one input keyed by the join columns, then **probe** it with each row of the other. The cost becomes `n + m`. The key is the same `AggregateKey` of stage 1 (so a `TINYINT 5` joins a `BIGINT 5`), with one difference from grouping that matters: in a join a NULL key matches **nothing**, not even another NULL, because the predicate `a.k = b.k` is not true for NULLs. For a left join an unmatched left row, or a left row with a NULL key, is still output once, padded.

> [!CHECK] Which input do you build the table from, and what happens to the memory and the work if you pick the wrong one? A left row has the key `5`, and the table holds three right rows with key `5`: how many output rows? A right row has a NULL key: is it in the table?
> ||Build from the input you expect to be smaller (it must fit in memory), probe with the larger (streamed); the wrong choice still gives the right answer but may not fit. Three output rows: one per matching pair. A right row with a NULL key is not put in the table (it could never be found: no probe key equals a NULL), which saves memory and avoids matching NULL with NULL.||
>
> - Why must the build side be complete before the first probe?
> - What does the hash table store: whole tuples or something smaller?
> - How does a left join know a left row matched nothing?

## The task

In `src/execution/executors/hash_join_executor.rs` (the struct, `new`, the `TupleStream`s are given; the key type is stage 1's `AggregateKey`):

- `join_key(exprs, tuple, schema) -> Result<Option<AggregateKey>>`: evaluate each key expression on the tuple; if **any** value is NULL the result is `None`; otherwise `Some(key)`.
- `init`: initialise both children; clear the table and the pending output; **build** from the right: for every right tuple whose key is not NULL, push it onto `self.table[key]` (the right key expressions and the right schema).
- `next`: until the batch is full: hand out `pending` first; take the next left tuple (none: stop); compute its key (the left key expressions, the left schema) and look it up; for every right tuple found queue `left values ++ right values` with the plan's output schema; with no match, or a NULL key, queue `unmatched_output(..)` if it returns something.
- `unmatched_output(left) -> Option<Tuple>`: for a **LEFT** join the left values followed by a typed NULL for every right column; `None` for an inner join.

The tests (the hash join is run from hand-built plans: the optimizer rule that chooses it is module 3h's): exact scenarios (matching pairs by hashing; the same rows as the nested loop join; NULL keys match nothing; duplicates on both sides; several key columns; keys of different integer widths; a left join pads unmatched and NULL-key rows; more matches than a batch), and a property: **for random tables with NULLs and duplicate keys**, a hash join on one or two key columns, inner or left, returns exactly the pairs with all keys equal (and non-NULL) and the padded unmatched left rows of a left join.

## Your freedom

What the table holds (whole right tuples, or row positions), whether you build in `init` or at the first `next`, and the order of output rows within a batch.

## The Rust toolbox

**`HashMap<AggregateKey, Vec<Tuple>>`.** `self.table.entry(key).or_default().push(tuple)` builds a multimap; `self.table.get(&key)` finds the matches.

**`Option<Key>` for "no key".** `let Some(key) = self.join_key(..)? else { queue unmatched; continue }` handles NULL keys and misses with the same path.

**The `entry` API.** `entry(key).or_default()` inserts an empty `Vec` if the key is new and returns a `&mut Vec`.

**Cloning tuples out of the table.** `matches.iter().map(|r| self.joined(&left, r))`: the table keeps its tuples for the next probe, so you clone or build new ones.

**Reuse.** The `pending` queue, `TupleStream`, `joined`, `values_of` and `nulls_for` are the same ones you used in stage 3.

## If this is new

- [S4 Maps & sets](/t/s4-maps-sets): `HashMap`, `entry`, `or_default`.
- L7 Enums & pattern matching: `let else` for early continues.
- S8 The core traits: why the key's `Eq` and `Hash` matter here.
- The optional *join algorithms* concept.
- S5 Queues & heaps: Understand: `VecDeque` as a buffer of pending output.
- [F4 Hashing & purpose-built structures](/t/f4-hashing-structures): Right structure for the job: hash tables for grouping and joining; when a scan or an index is better.
- Y5 Testing & verification: Build it: differential tests: three algorithms against each other and a naive one.

## Tests

- Matching pairs; the same rows as the nested loop join; NULL keys match nothing; duplicates on both sides; several key columns; integer widths; left join padding for unmatched and NULL-key rows; more matches than a batch.
- Property: random tables, one or two key columns, inner and left.

## Hints

### Build first, then probe

Everything from the right child is read in `init`. The left child is streamed in `next`: it may be much larger than memory.

### One path for "no match"

A left row with a NULL key and one whose key is not in the table both end in `unmatched_output`: for an inner join it returns `None` and the row is dropped.

## Performance

`O(n + m)` with one hash per row and a table probe: tens of nanoseconds each. Memory is the build side. A hash join of two million-row tables takes a fraction of a second; the nested loop takes a day.

**Measure it.** Rerun stage 3's measurement with your hash join (same tables and keys) and compare the growth.

## Experiment

Optional. Predict first, then run.

1. **Wrong build side.** Build from the larger table. What changes in memory, in the test results?
2. **Skew.** One key holds half of the right rows. What does a probe of that key cost, and what does it do to the batch?

## Other designs

- **In-memory hash join (ours).**
- **Grace hash join:** partition both sides to disk by hash, join partition pairs.
- **Hybrid hash join:** keep the first partition in memory.
- **Sort-merge join,** and **index nested loops** (stage 5).

## In BusTub

`aggregation_executor.cpp`, `nested_loop_join_executor.cpp`, `hash_join_executor.cpp` and `nested_index_join_executor.cpp` are stubs in Project 3 (`UNIMPLEMENTED("TODO(P3): Add implementation.")`); the header comments carry the contract. The executors are batched, as in module 3e.

## The C/C++ way

| C / C++ | Rust |
|---|---|
| `std::unordered_map<AggregateKey, std::vector<Tuple>>` | `HashMap<AggregateKey, Vec<Tuple>>` |
| `HashJoinKey`/`JoinKey` with `operator==` | the same `AggregateKey` as in stage 1 |
| `if (key.IsNull()) continue;` | `let Some(key) = .. else { .. }` |

**Port rule:** an `unordered_map` of vectors becomes a `HashMap<K, Vec<V>>` built with `entry().or_default()`.

## Learn more

- PostgreSQL's [hash joins](https://www.postgresql.org/docs/current/planner-optimizer.html) · *Database System Concepts*, "Hash join"
