`ORDER BY a DESC NULLS LAST, b` sorts rows by two **keys**, each with a direction and a rule for NULLs. Every sorting algorithm in this module (the external merge sort, top-N, window functions with an `ORDER BY`) needs the same two small things, and this stage builds them. First the **sort key** of a row: the values of the `ORDER BY` expressions, evaluated once, so that sorting compares plain `Value`s and never re-evaluates an expression. Then the **comparator** that orders two keys, which has to get three rules right: the first key that differs decides; `DESC` reverses the values; and NULL is the smallest value (first when ascending, last when descending) unless `NULLS FIRST` or `NULLS LAST` says otherwise.

> [!CHECK] Sort `[3, NULL, 1]` with `ORDER BY a`, with `ORDER BY a DESC`, with `ORDER BY a DESC NULLS FIRST`. Write the three results. Then: is "comparator returns `Less` for `NULL` vs `NULL`" a bug, and what can it do to a sort? What does the comparator return for two rows that are equal on every key, and why does a *stable* sort care?
> ||`[NULL, 1, 3]`; `[3, 1, NULL]`; `[NULL, 3, 1]` (NULLS FIRST wins over the direction). Two NULLs must compare `Equal`: a comparator that says `Less` both ways breaks antisymmetry, and Rust's `sort_by` may panic ("does not correctly implement a total order") or return a wrong order. Rows equal on every key compare `Equal`; a stable sort keeps them in input order, which is what makes `ORDER BY a` reproducible and lets a query sort by one key after another.||
>
> - What does `DESC` do to the NULL rule?
> - Do you compare `Value`s with `==` or with `compare_equals`/`compare_less_than`?
> - What if the key list is empty?

## The task

In `src/execution/execution_common.rs`:

- `generate_sort_key(tuple, order_bys, schema) -> Result<SortKey>`: evaluate each `ORDER BY` expression on the tuple, in order.
- `TupleComparator::compare_keys(&self, a: &SortKey, b: &SortKey) -> Ordering` (`compare` on whole entries, given, calls it): go through the order-by items in turn; the **first key that differs decides**; `Equal` if all are equal. For one key: two NULLs are equal; one NULL comes first when NULLs go first (`NULLS FIRST`, and by default for an ascending key) and last for `NULLS LAST` and, by default, for a descending key; two values compare by value (`Value::compare_equals`, then `compare_less_than`), reversed for `DESC`.

The tests: exact scenarios (the sort key is the values of the `ORDER BY` expressions; ascending, descending and default directions; the first differing key decides; NULL smallest by default; `NULLS FIRST/LAST` win over the direction; strings compare bytewise and a vector sorts), and a property: **for random rows with NULLs, several keys, both directions and every NULL rule, the comparator equals a model that spells out the rules**, is antisymmetric, and sorting with it gives the model's stable sort.

## Your freedom

How you write the comparison (a loop over the keys with an early return, `Iterator::find`, `then_with` chains), and how you encode the NULL rule.

## The Rust toolbox

**`Ordering` and `then_with`.** `a.cmp(&b).then_with(|| c.cmp(&d))` chains keys: the second comparison runs only when the first is `Equal`. `ord.reverse()` flips `Less` and `Greater` (that is `DESC`).

**A `match` on a pair of booleans.** `match (x.is_null(), y.is_null()) { (true, true) => Equal, (false, false) => values, (x_null, _) => ... }`: all four cases in one place.

**An early `return`.** `if ord != Ordering::Equal { return ord; }` is the "first difference decides" rule.

**`Value` comparison returns a `CmpBool`/`Result`.** Module 3a's `compare_less_than` may report an error for incomparable types; here the types agree, so `unwrap` on a checked path is acceptable, or map the error.

**Antisymmetry for free.** If you implement one `compare(a, b)` and never a second one for `(b, a)`, `compare(b, a) == compare(a, b).reverse()` holds as long as every branch does.

## If this is new

- S8 The core traits: `Ord`, `Ordering`, what a total order requires.
- L7 Enums & pattern matching: `match` on a tuple of booleans.
- [S3 Vec & slices](/t/s3-vec-slices): `sort_by`, stability.
- The optional *sort keys and NULL ordering* concept.
- Y5 Testing & verification: Build it: a sorted vector as the oracle for sort, limit, top-N and rank.

## Tests

- The sort key holds the `ORDER BY` values in order; an empty list gives an empty key.
- Ascending, descending, default directions; the first key that differs decides; ties are `Equal`.
- NULL smallest by default; `NULLS FIRST/LAST` over the direction; strings bytewise; a vector sorts.
- Property: the comparator against a model of the rules, antisymmetry, stable sort.

## Hints

### Write the NULL rule as one boolean

"Do NULLs go first for this key?": `NullsFirst` yes, `NullsLast` no, otherwise yes exactly when the key is ascending. Then a NULL against a value is `Less` if NULLs go first (for the NULL one) and `Greater` otherwise.

### Reverse only the values

`DESC` reverses the order of two *values*; it must not reverse the NULL rule (that is what `NULLS FIRST/LAST` is for).

## Performance

A comparison of two integer keys is a handful of instructions; a `Value` comparison goes through an enum `match` and is ten to fifty times slower, which is why real engines compile sort keys into raw bytes whose `memcmp` order is the sort order. The sort key here is evaluated once per row (`n` evaluations, not `n log n`).

**Measure it.** Sort a million rows by evaluating the `ORDER BY` expression inside the comparator, and then with precomputed keys; compare.

## Experiment

Optional. Predict first, then run.

1. **Two NULLs `Less`.** Make `(true, true)` return `Less`. Which tests fail and what does `sort_by` do?
2. **Encode keys as bytes.** Encode an `(INTEGER, ascending)` key as a big-endian `u32` with the sign bit flipped. Does the model property still hold with `memcmp`?

## Other designs

- **Compare `Value`s (ours, BusTub's).**
- **Normalised key bytes:** one `memcmp` per comparison (PostgreSQL's abbreviated keys, DuckDB).
- **Compile the comparator** per query (JIT).
- **Radix sort** on fixed-size keys: no comparisons at all.

## In BusTub

`external_merge_sort_executor.cpp`, `limit_executor.cpp`, `topn_executor.cpp` and `window_function_executor.cpp` are stubs in Project 3 (`UNIMPLEMENTED("TODO(P3): Add implementation.")`). The 2025 version of the project asks for an external merge sort (`MergeSortRun`, `ExternalMergeSortExecutor<K>`), a top-N executor with a bounded heap, and window functions.

## The C/C++ way

| C / C++ | Rust |
|---|---|
| `struct TupleComparator { bool operator()(const SortEntry &a, const SortEntry &b) const; }` | `TupleComparator::compare(&self, a, b) -> Ordering` |
| `order_by_type == OrderByType::DESC` | `ob.order_type == OrderByType::Desc` |
| `std::sort(v.begin(), v.end(), cmp)` (not stable) / `std::stable_sort` | `v.sort_by(\|a, b\| cmp.compare(a, b))` (stable) |

**Port rule:** a comparison functor becomes a method returning `Ordering`; `std::stable_sort` is Rust's default `sort_by`.

## Learn more

- [`Ordering`](https://doc.rust-lang.org/std/cmp/enum.Ordering.html) · PostgreSQL's [ORDER BY](https://www.postgresql.org/docs/current/queries-order.html)
