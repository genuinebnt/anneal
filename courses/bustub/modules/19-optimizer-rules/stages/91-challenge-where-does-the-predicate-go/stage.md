A challenge: no walkthrough, no hints, no solution. Solve it with what this module and the ones before it taught. It is extra practice and does not count towards the course.

## What to build

`conjuncts` and `split_for_join` in `src/optimizer/pushdown.rs`: `conjuncts(p)` flattens nested `AND`s into a list of predicates; `split_for_join(p, left_cols)` divides them for a join of two inputs whose combined row is `[left columns..., right columns...]`: those that mention only left columns (can be applied below the left input), only right columns (below the right), and the rest (must stay above the join).

## Why

`SELECT ... FROM a JOIN b ON ... WHERE a.x = 1 AND b.y > 3 AND a.z < b.w` should filter `a` and `b` *before* joining them, and only the last conjunct needs both sides. Pushdown is among the most valuable rewrites there is, and its safety rule is simple: split only at `AND`, never inside an `OR` or under a `NOT`.

## The contract

- `Pred` is `Cmp(col, op, constant)`, `ColCmp(col, op, col)`, `And(Vec<Pred>)`, `Or(Vec<Pred>)`, `Not(Box<Pred>)`.
- `conjuncts` flattens `And` recursively, keeping order, and returns anything else as one conjunct; an empty `And` has none.
- `split_for_join` returns `(left_only, right_only, both)`; columns below `left_cols` are left, the rest right. A conjunct that mentions no column counts as `left_only`.

## Invariants

These must hold after every step, whatever the input:

- Each conjunct goes to exactly one of the three lists, in original order within a list.
- `left_only` uses only columns `< left_cols`; `right_only` only columns `>= left_cols`.

## Relations between input and output

How the answer must change when the input changes. The property tests check these on random inputs:

- The conjunction of the three lists is equivalent to the input predicate on every row.
- An `Or` or `Not` is never split.
- Pushing nothing (all three merged back) gives the input's conjuncts.

## Examples

Worked cases (the tests include them):

```text
a.x=1 AND b.y>3 AND a.z<b.w with left_cols=2 -> left [a.x=1], right [b.y>3], both [a.z<b.w]
```

## What the tests check

- Flattening nested `AND`s.
- The three-way split.
- `OR` kept whole.
- A property: equivalence on random rows.

## Done when

All the `s3h_c2` tests pass.

## Further reading

In the cookbook. Some of these pages show a complete solution, so they are best read after your own attempt.

- [Plan transformations and rule soundness](/cookbook/p/plan-transformations-and-rule-soundness): pushing filters through joins, and the wrong push into an outer join's null side.
- [Query planning and rewriting](/cookbook/p/query-planning-and-rewriting): predicate pushdown in the optimizer pipeline.
- [A cost-based query optimizer in miniature](/cookbook/p/query-optimizer): conjunction splitting and predicate pushdown in a small rule engine.
