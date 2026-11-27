A challenge: no walkthrough, no hints, no solution. Solve it with what this module and the ones before it taught. It is extra practice and does not count towards the course.

## What to build

`distinct_rows` in `src/storage/table/distinct.rs`: remove duplicate rows from a list, keeping the **first** occurrence of each and the original order. Rows are vectors of nullable integers, and for `DISTINCT` (and `GROUP BY`) two NULLs count as **equal**, even though `NULL = NULL` is unknown in a `WHERE` clause.

## Why

SQL has two notions of sameness: *equal* (three-valued, NULL is unknown) and *not distinct* (two-valued, NULL is the same as NULL). `DISTINCT`, `GROUP BY`, `UNION`, hash joins on `IS NOT DISTINCT FROM` and unique indexes (in most systems) use the second. A port that uses `==` on `Option<i64>` gets it right by luck; one that uses SQL `=` gets it wrong.

## The contract

- `distinct_rows(rows)` returns the rows with later duplicates removed, order preserved.
- `(None, 1)` and `(None, 1)` are duplicates; `(None, 1)` and `(Some(0), 1)` are not.
- `distinct_count(rows)` is the number of distinct rows.

## Invariants

These must hold after every step, whatever the input:

- The output has no two equal rows.
- Every output row appears in the input, and every input row equals some output row.
- The output is a subsequence of the input.

## Relations between input and output

How the answer must change when the input changes. The property tests check these on random inputs:

- `distinct(distinct(x)) == distinct(x)`.
- Permuting the input permutes which duplicates survive but never changes `distinct_count`.
- Appending a row already present changes nothing.

## Examples

Worked cases (the tests include them):

```text
[(1,NULL),(1,NULL),(2,3),(1,NULL)] -> [(1,NULL),(2,3)]
```

## What the tests check

- Duplicates with NULLs.
- Order of first occurrences.
- A property against a first-seen filter.

## Done when

All the `s3b_c5` tests pass.

## Further reading

In the cookbook. Some of these pages show a complete solution, so they are best read after your own attempt.

- [Modern SQL](/cookbook/p/modern-sql): SQL equality versus IS NOT DISTINCT FROM, and which one DISTINCT and GROUP BY use.
