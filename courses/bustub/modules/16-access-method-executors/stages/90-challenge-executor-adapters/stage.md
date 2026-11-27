A challenge: no walkthrough, no hints, no solution. Solve it with what this module and the ones before it taught. It is extra practice and does not count towards the course.

## What to build

`Filter`, `Project`, `Limit` and `Concat` in `src/execution/adapters.rs`: pull-based operators over the given `Executor` trait (`next() -> Option<Row>`, where a row is a `Vec<i64>`). Each wraps a child (or two); `Limit` takes a limit and an offset and must **stop pulling** from its child once it has what it needs.

## Why

These four, and a scan under them, are the whole of a Volcano-style executor in miniature. The laziness is the point: `SELECT * FROM big LIMIT 10` must read ten rows, not ten million, and an operator that pre-reads its input to be safe makes every query pay for it.

## The contract

- `Filter::new(child, pred)` yields child rows for which `pred(&row)` is true, in order.
- `Project::new(child, cols)` yields each row restricted to the columns `cols` (in that order).
- `Limit::new(child, limit, offset)` skips `offset` rows, yields at most `limit` rows, and pulls nothing more from the child once `limit` rows have been yielded.
- `Concat::new(a, b)` yields all of `a`, then all of `b`; it never touches `b` before `a` is exhausted.

## Invariants

These must hold after every step, whatever the input:

- Row order is the order the children produce them.
- Every operator is exhausted after it returns `None` once (it keeps returning `None`).
- The number of rows pulled from a child never exceeds what the operator needs.

## Relations between input and output

How the answer must change when the input changes. The property tests check these on random inputs:

- `Limit(n)` over a scan of `m` rows pulls `min(m, offset + n)` rows from the scan.
- `Filter` then `Project` equals the same on a `Vec`.
- Composing operators in a `Box<dyn Executor>` tree equals the corresponding iterator chain.

## Examples

Worked cases (the tests include them):

```text
scan [1..10] -> Filter(even) -> Limit(2, offset 1) -> 4, 6; the scan was pulled 6 times
```

## What the tests check

- Each operator alone.
- Composition, and the laziness counters.
- A property against an iterator chain.

## Done when

All the `s3e_c1` tests pass.

## Further reading

In the cookbook. Some of these pages show a complete solution, so they are best read after your own attempt.

- [One query, four execution models](/cookbook/p/volcano-vectorized-push): Box<dyn Operator> pipelines, and LIMIT stopping its child early.
- [Access-method executors](/cookbook/p/access-method-executors): Volcano executors for scan, filter, projection and limit.
