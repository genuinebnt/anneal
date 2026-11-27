A challenge: no walkthrough, no hints, no solution. Solve it with what this module and the ones before it taught. It is extra practice and does not count towards the course.

## What to build

`InSet` in `src/execution/in_set.rs`: built once from a list of constants (some of which may be NULL), it answers `contains(x)` for a value that may be NULL with TRUE, FALSE or NULL, as `x IN (list)` does, and `not_in(x)` as `x NOT IN (list)` does, without scanning the list for every row.

## Why

A parser rewrite turns `a IN (1, 2, 3)` into a chain of ORs, which costs a comparison per item per row. A long list of constants (a thousand ids from an application) makes that the slowest part of the query. A hash set answers in one probe, and the part to get right is not the set but the answers around it: a NULL in the list changes what a miss means.

## The contract

- `new(list)` takes the constants as `Option<i64>`; duplicates and NULLs are allowed.
- `contains(x)`: TRUE if `x` is not NULL and is in the list; otherwise NULL if `x` is NULL and the list is not empty, or if the list contains a NULL; otherwise FALSE.
- `not_in(x)` is the three-valued negation of `contains(x)`.
- `len()` is the number of distinct non-NULL values.

## Invariants

These must hold after every step, whatever the input:

- `contains` agrees with the OR chain `x = v1 OR x = v2 ...` evaluated in three-valued logic.
- `not_in(x) == not contains(x)` for every `x`.

## Relations between input and output

How the answer must change when the input changes. The property tests check these on random inputs:

- Adding a value that is already there changes nothing.
- Adding a NULL to a list turns every FALSE of `contains` into NULL and changes no TRUE.
- An empty list answers FALSE for every `x`, a NULL `x` included.

## Examples

Worked cases (the tests include them):

```text
list [1, 2]: contains(Some(2)) = TRUE, contains(Some(3)) = FALSE, contains(None) = NULL
list [1, NULL]: contains(Some(3)) = NULL, not_in(Some(1)) = FALSE
```

## What the tests check

- Hits, misses and NULL on the left.
- A NULL in the list.
- The empty list and duplicates.
- A property against the OR chain.

## Done when

All the `s3i_c1` tests pass.

## Further reading

In the cookbook. Some of these pages show a complete solution, so they are best read after your own attempt.

- [Modern SQL](/cookbook/p/modern-sql): x IN (1, NULL): TRUE on a match, otherwise UNKNOWN.
- [SQL front end](/cookbook/p/sql-front-end): the OR-chain desugaring of IN that a hash set replaces.
