A challenge: no walkthrough, no hints, no solution. Solve it with what this module and the ones before it taught. It is extra practice and does not count towards the course.

## What to build

`cmp_nullable` in `src/types/null_order.rs`: compare two values that may be NULL, for one sort key, with a direction (`Asc`/`Desc`) and a NULL placement (`First`/`Last`). The NULL placement is **independent of the direction**: `ORDER BY x DESC NULLS FIRST` puts NULLs first and the non-NULL values in descending order after them.

## Why

`ORDER BY` is in every report and every pagination query, and NULL handling is where databases differ (and where bugs in ports come from). The two options are orthogonal, and the comparator must be a consistent total preorder or a sort will misbehave or panic.

## The contract

- `cmp_nullable(a, b, dir, nulls)` returns an `Ordering` meaning "`a` sorts before / equal / after `b`".
- Two NULLs are equal. NULL against a value is decided by `nulls` alone.
- Two values compare by `dir`.

## Invariants

These must hold after every step, whatever the input:

- It is a total preorder: reflexive, transitive, and `cmp(a, b) == cmp(b, a).reverse()`.
- Sorting with it never panics and is stable for equal keys.

## Relations between input and output

How the answer must change when the input changes. The property tests check these on random inputs:

- Flipping `dir` reverses the order of the non-NULL values and leaves the NULLs where they were.
- Flipping `nulls` moves all NULLs to the other end and leaves the non-NULL order alone.
- `Asc/Last` equals `Option`'s natural order with `None` last.

## Examples

Worked cases (the tests include them):

```text
[3, NULL, 1] ASC NULLS LAST -> 1, 3, NULL
[3, NULL, 1] DESC NULLS FIRST -> NULL, 3, 1
[3, NULL, 1] ASC NULLS FIRST -> NULL, 1, 3
```

## What the tests check

- The four combinations on a small list.
- Equal NULLs; stability.
- Properties: antisymmetry, transitivity, and the effect of flipping each option.

## Done when

All the `s3a_c2` tests pass.

## Further reading

In the cookbook. Some of these pages show a complete solution, so they are best read after your own attempt.

- [SQL front end](/cookbook/p/sql-front-end): ORDER BY ... NULLS FIRST | LAST and where each engine sorts NULL.
- [Modern SQL](/cookbook/p/modern-sql): NULL ordering defaults that differ between PostgreSQL, SQLite and MySQL.
