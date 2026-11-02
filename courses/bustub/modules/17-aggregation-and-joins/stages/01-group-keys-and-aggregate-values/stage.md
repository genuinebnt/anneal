`SELECT g, count(*), sum(v) FROM t GROUP BY g` needs a hash table with one entry per group, and each entry holds the **running values** of the aggregates, updated as rows arrive. Two small ideas make it work, and they are this stage. The **key** of a group is the list of the `GROUP BY` values: two rows are in the same group when their lists are *the same*, which for SQL is not `==` on `Value` (two NULLs are the same group, though `NULL = NULL` is not true; a `TINYINT 5` and a `BIGINT 5` are the same group; `0.0` and `-0.0` are the same group). And an **aggregate** folds a stream of values into a number with its own rules for NULL and for "nothing seen yet".

> [!CHECK] `HashMap` needs `Eq` and `Hash` on the key and silently misbehaves if they disagree. State the contract. Then say what each aggregate returns for the input `NULL, 4, NULL`, for the input `NULL, NULL`, and for no input at all: `count(*)`, `count(v)`, `sum(v)`, `min(v)`.
> ||The contract: `a == b` implies `hash(a) == hash(b)` (the converse can fail); a key is the same as itself, and `==` is symmetric and transitive. Over `NULL, 4, NULL`: `count(*)` is 3 (rows), `count(v)` is 1, `sum(v)`, `min(v)` are 4. Over `NULL, NULL`: `count(*)` is 2, and `count(v)`, `sum(v)`, `min(v)` are all NULL (BusTub's rule: nothing non-NULL was seen). Over no input at all, `count(*)` is 0 and the rest NULL.||
>
> - Which value do you hash for a NULL, so that NULLs of different types agree?
> - What does `count(*)` look at, if not the value?
> - Why does the first non-NULL input have to *seed* the running value for `min`?

## The task

In `src/execution/executors/aggregation_executor.rs`:

- `AggregateKey`'s `PartialEq` and `Hash` (a helper `canonical(value)` that turns a `Value` into a comparable, hashable shape is given): the same number of values, every pair "the same group", and a hash fed with the canonical form of every value so equal keys hash alike.
- `generate_initial_aggregate_value` (given: `count(*)` starts at 0, the other aggregates at NULL) and `combine_aggregate_values(&mut running, &input)`: for each aggregate, `count(*)` adds 1 for every row; for the others a NULL input changes nothing; `count(x)`: 1 if nothing seen yet, else +1; `sum`: `x` if nothing seen yet, else running + x (an overflow is an error); `min`/`max`: likewise with `Value::min`/`Value::max`.
- `SimpleAggregationHashTable::insert_combine(key, value)` (given) finds or creates a group and folds a value into it.

The tests: exact scenarios (equal keys are equal and hash alike; two NULLs are one group, whatever their type; integers of different widths; the number of values matters; decimal zeros; keys work in a `HashMap`; the initial values; each aggregate over a few values; NULL inputs; a group of only NULLs stays NULL; the first value seeds; sum overflow is an error; one running value per group), and two properties: **keys are equal exactly when their values mean the same**, and equal keys hash alike (random lists of values of mixed types and widths); and **folding random (group, value) rows into the table gives, per group, the counts, the sum, the minimum and the maximum a per-group Rust fold gives**.

## Your freedom

Not much here, because the contract is exact; the freedom is in how you write the `match` (one arm per aggregate, or helper methods) and how you hash (feed the canonical tuple, or hand-write a hash).

## The Rust toolbox

**`Eq` and `Hash` by hand.** `impl PartialEq for AggregateKey { fn eq(&self, other: &Self) -> bool { ... } }`, `impl Eq for AggregateKey {}`, `impl Hash for AggregateKey { fn hash<H: Hasher>(&self, state: &mut H) { ... } }`. A tuple of primitives already implements `Hash`, so `canonical(v).hash(state)` is one line.

**Comparing canonical forms.** Derive nothing for `Value`: `canonical(a) == canonical(b)` compares two plain tuples.

**`let else` and `continue`.** `_ if x.is_null() => continue` skips an input inside a `for` loop's `match`.

**`Value` arithmetic.** `running.add(x)?` is checked addition with module 3a's overflow rule; `running.min(x)?` / `running.max(x)?` compare by value.

**Mutable access by index.** `result.aggregates[i] = ...` replaces one running value; read the old one into a local first so you do not hold a borrow while assigning.

## If this is new

- S8 The core traits: `Eq`, `Hash` and their contract.
- [S4 Maps & sets](/t/s4-maps-sets): `HashMap`, keys that are structs.
- L7 Enums & pattern matching: `match` on the aggregate kind.
- The optional *Eq, Hash, Ord contracts* concept.
- Y5 Testing & verification: Build it: differential tests: three algorithms against each other and a naive one.

## Tests

- Keys: equal and hash alike; NULLs together; integer widths; the number of values; decimal zero; usable in a `HashMap`.
- Values: initial values; each aggregate over a few inputs; NULL inputs ignored except by `count(*)`; only NULLs stays NULL; the first value seeds; sum overflow is an error; one running value per group.
- Properties: key equality means "same value"; the table equals per-group folds.

## Hints

### Why not derive `PartialEq` on `Value`?

`Value::Null == Value::Null` can be true in Rust, but `Integer(5) == BigInt(5)` is false, `Decimal(0.0) == Decimal(-0.0)` is true while their bits differ and so would their hashes, and NaN is not equal to itself. SQL grouping has its own rule: that is why `canonical` exists.

### The first value

`min` and `max` cannot start at 0 (the minimum of positive numbers is not 0). Keep NULL for "nothing seen yet" and let the first non-NULL input replace it.

## Performance

A hash aggregation is one hash and one table probe per row: about 50 to 100 ns with a good hash, so ten million rows a second per core. Hashing `canonical(v)` allocates nothing for integers; for strings it hashes the bytes.

**Measure it.** Aggregate a million rows into 10, 1 000 and 1 000 000 groups; plot rows per second and explain the drop (cache misses on the table).

## Experiment

Optional. Predict first, then run.

1. **A broken hash.** Make `hash` feed a constant. Which test fails, and how slow does a group-by of a hundred thousand groups become?
2. **A broken equality.** Compare `Value`s with the derived `==`. Which of the two properties finds it first?

## Other designs

- **A hash table of running values (ours, BusTub's).**
- **Sort-based aggregation:** sort by the key, fold runs; no hash, but a sort (module 3g).
- **Partial aggregation:** each thread aggregates its part, then the partial results merge.
- **Streaming aggregation over an index** (rows already in key order): one running group at a time.

## In BusTub

`aggregation_executor.cpp`, `nested_loop_join_executor.cpp`, `hash_join_executor.cpp` and `nested_index_join_executor.cpp` are stubs in Project 3 (`UNIMPLEMENTED("TODO(P3): Add implementation.")`); the header comments carry the contract. The executors are batched, as in module 3e.

## The C/C++ way

| C / C++ | Rust |
|---|---|
| `bool operator==(const AggregateKey &other) const` + `std::hash<AggregateKey>` | `impl PartialEq`, `impl Eq`, `impl Hash` |
| `std::unordered_map<AggregateKey, AggregateValue>` | `HashMap<AggregateKey, AggregateValue>` |
| `CmpBool::CmpTrue` of `CompareEquals` | `canonical(a) == canonical(b)` |

**Port rule:** a custom hash functor and equality operator become the `Hash` and `PartialEq` impls of the key type.

## Learn more

- [`Hash`](https://doc.rust-lang.org/std/hash/trait.Hash.html) · PostgreSQL's [aggregate functions](https://www.postgresql.org/docs/current/functions-aggregate.html)
