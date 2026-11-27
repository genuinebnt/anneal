A challenge: no walkthrough, no hints, no solution. Solve it with what this module and the ones before it taught. It is extra practice and does not count towards the course.

## What to build

`semi_join`, `anti_join` and `not_in` in `src/execution/semi_join.rs`: given a list of left keys and a list of right keys (both `Option<i64>`, `None` is NULL), return the **indexes of the left rows** that survive. `semi_join` is `x IN (right)` and `EXISTS`; `anti_join` is `NOT EXISTS (... WHERE r = x)`; `not_in` is `x NOT IN (right)`, which is **not** the same.

## Why

`NOT IN` with a NULL in the subquery returns no rows at all: for every `x`, `x <> NULL` is unknown, so no row can be proven to be absent. This is the most famous trap in SQL, and the reason `NOT EXISTS` exists. A hash anti join must implement both, and know which one the query asked for.

## The contract

- `semi_join(left, right)`: rows whose key is non-NULL and equals some non-NULL right key.
- `anti_join(left, right)` (NOT EXISTS): rows with **no** matching right key; a NULL left key matches nothing, so it is kept.
- `not_in(left, right)`: if `right` is empty, every row (even NULL keys) is kept; otherwise, if `right` contains a NULL, no row is kept; otherwise rows with a non-NULL key that is not in `right`.

## Invariants

These must hold after every step, whatever the input:

- Results are indexes in increasing order, without repeats.
- `semi_join` and `anti_join` partition the left rows.

## Relations between input and output

How the answer must change when the input changes. The property tests check these on random inputs:

- `anti_join` and `not_in` agree when `right` has no NULLs and `left` has no NULLs.
- Adding a NULL to `right` never changes `semi_join` or `anti_join`, and makes `not_in` empty (unless `right` was empty).
- `semi_join(left, right)` is a subset of `semi_join(left, right + more)`.

## Examples

Worked cases (the tests include them):

```text
left [1, 2, NULL], right [2, 3]: semi [1]; anti [0, 2]; not_in [0]
right [2, NULL]: semi [1]; anti [0, 2]; not_in []
```

## What the tests check

- Each operation with and without NULLs.
- The empty right side.
- A property against a three-valued-logic model.

## Done when

All the `s3f_c2` tests pass.

## Further reading

In the cookbook. Some of these pages show a complete solution, so they are best read after your own attempt.

- [Modern SQL](/cookbook/p/modern-sql): the NOT IN trap: a NULL in the list means no row is ever TRUE.
- [Unnesting subqueries](/cookbook/p/unnesting-subqueries): why NOT IN needs a null-aware anti-join when it is unnested.
- [Join algorithms compared](/cookbook/p/join-algorithms-code): hash-based semi and anti joins for EXISTS and NOT EXISTS.
