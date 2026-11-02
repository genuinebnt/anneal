`UNION ALL` just concatenates. The other five forms (`UNION`, `INTERSECT [ALL]`, `EXCEPT [ALL]`) have to decide when two rows are *the same row*, and count them. The cleanest way to think about them is arithmetic on counts: if a row appears `na` times on the left and `nb` times on the right, the operation says how many times it appears in the answer. Write the six formulas once and the executor is a loop over a map. Then the part that surprises people: here, two NULLs **are** the same row.

> [!CHECK] Left rows: `(1)`, `(1)`, `(1)`, `(2)`. Right rows: `(1)`, `(1)`, `(3)`. Write the result of `UNION`, `UNION ALL`, `INTERSECT`, `INTERSECT ALL`, `EXCEPT` and `EXCEPT ALL`. Then: left is `(NULL)`, right is `(NULL)`; what do `INTERSECT` and `EXCEPT` give?
> ||UNION: 1, 2, 3. UNION ALL: seven rows (1 five times, 2, 3). INTERSECT: 1. INTERSECT ALL: 1, 1 (the smaller count, `min(3, 2)`). EXCEPT: 2 (every 1 is removed as soon as the right has any 1). EXCEPT ALL: 1, 2 (`3 - 2 = 1` copy of 1, and 2 stays). For NULL: INTERSECT returns the NULL row; EXCEPT returns nothing. Set operations treat two NULLs as equal, the way `GROUP BY` and `DISTINCT` do; `WHERE a.x = b.x` would not.||
>
> - What are the formulas for the six operations in terms of `na` and `nb`?
> - What do you use as the key of a row so that two NULLs collide but a NULL and a `0` do not, and `1` as integer and `1` as a decimal do not?
> - Which of the operators are symmetric?

## The task

Implement the rest of `SetOpExecutor::compute`:

- `UNION` returns every distinct row of either side once.
- `INTERSECT` returns each row found on both sides once; `INTERSECT ALL` returns it `min(na, nb)` times.
- `EXCEPT` returns each row of the left that is **not** on the right, once; `EXCEPT ALL` returns it `max(na - nb, 0)` times.
- Rows are equal when every column is equal, with two NULLs equal.
- `UNION` is also reachable as `UNION DISTINCT`.
- The result has the same row type as before; its order is not specified (the tests sort).

## Your freedom

The key of a row (a string of the values and types, a vector of values with a hash that treats NULL as a value, a sorted merge), the order in which the rows come out (first seen is the natural one), and whether the operators share one function or have one each.

## The Rust toolbox

**`HashMap<Key, usize>`.** Counting rows is a map from key to count. Both sides count, and the six formulas read the two counts.

**A key that is `Eq + Hash`.** `Value` may hold a float or not implement `Hash`; building a `String` or `Vec<u8>` from the values and their types is the quick way, and is what the given `row_key` does.

**`saturating_sub`.** `na - nb` on `usize` panics (debug) or wraps when `nb > na`; `saturating_sub` gives the 0 the formula wants.

## If this is new

- [S2 Collections](/t/s2-strings-text): `HashMap` entry API.
- Y5 Testing & verification: a model written in the style of the specification.

## Tests

- `UNION` removes duplicates across and within the sides; a table united with itself gives its distinct rows.
- Two NULL rows are one row, for all three operators.
- `INTERSECT` and `EXCEPT` on a worked example; `ALL` counts (`min`, difference never below zero).
- Symmetry: `UNION` and `INTERSECT` give the same rows with the sides swapped; `EXCEPT` does not.
- Strings of different declared lengths and mixed rows.
- A set operation inside a subquery in `FROM`.
- A property over random tables with NULLs and duplicates: every operator, with and without `ALL`, equals the formulas.

## Hints

### The formulas are the code

Transcribe the six formulas into a function from `(na, nb)` to a count, and let the executor loop over the union of the keys. Debug the arithmetic in the function, not in the executor.

### Keep the first row you saw

When you output a row `n` times you need a row to output. Store the first tuple with each key, or reconstruct it from the key; do not store the count alone.

### Types are part of the key

`1` as an integer and `1` as a decimal never meet in a query that passed the planner's check, but `0` and `NULL` must stay different: the key must say which one it is.

## Performance

Hashing both sides costs `O(|left| + |right|)` time and memory proportional to the number of *distinct* rows. A sort-based alternative uses `O(n log n)` time and can spill to disk with an external sort, which is what a database does when the distinct rows do not fit in memory. `UNION` is much more expensive than `UNION ALL` for this reason.

**Measure it.** Compare `select x from t union select x from t` and `... union all ...` with `explain analyze` on a 100 000-row table with 10 distinct values.

## Experiment

Optional. Predict first, then run.

1. **Make NULL a row that equals nothing** (skip it in the map). Which tests fail and what does `INTERSECT` give for the NULL row?
2. **Implement `EXCEPT ALL` as `na - nb` on signed integers without the floor.** What does the output look like?

## Other designs

- **Sort-based:** sort both sides on all columns, merge. Preserves order, spills to disk.
- **Join-based:** `INTERSECT` is a semi-join on null-safe equality, `EXCEPT` an anti-join; some optimizers rewrite them that way to use join algorithms.
- **Bitmaps for tiny domains.**

## In BusTub

BusTub has no set operations. The hash key you build here is the same idea as the key of the hash aggregation of project 3.

## The C/C++ way

| C / C++ | Rust |
|---|---|
| `std::unordered_map<Key, size_t>` with a custom hash | `HashMap<String, usize>` with a key you build |
| `max(0, na - nb)` on `size_t` (wraps) | `na.saturating_sub(nb)` |

**Port rule:** subtraction on unsigned counts is a bug until it is proven not to underflow.

## Learn more

- [PostgreSQL: set operations](https://www.postgresql.org/docs/current/queries-union.html) · [SQL standard note on NULL in set operations](https://modern-sql.com/concept/null)
