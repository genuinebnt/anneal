A challenge: no walkthrough, no hints, no solution. Solve it with what this module and the ones before it taught. It is extra practice and does not count towards the course.

## What to build

`fold` in `src/optimizer/const_fold.rs`: simplify an expression tree over integer and boolean constants and column references, **without changing its value on any row**. Evaluate every subtree that has no column in it; apply the identities `x + 0`, `0 + x`, `x * 1`, `1 * x` -> `x`, `x * 0`, `0 * x` -> `0`, `true AND x` -> `x`, `false AND x` -> `false`, `false OR x` -> `x`, `true OR x` -> `true`, `NOT NOT x` -> `x`. "Without changing its value" includes failures: a subtree that overflows or mixes types fails before folding and must still fail after. So an identity that keeps `x` needs `x` to have the right type, and one that drops `x` (`* 0`, `false AND`, `true OR`) is allowed only when `x` cannot fail at all.

## Why

`WHERE price * 1.0 > 10 + 5` should not multiply by one and add five a million times. Folding is the oldest and cheapest optimiser rule, and the one every other rule relies on (a predicate that folds to `false` makes the whole scan empty). Its correctness condition is exact and testable: the folded tree must evaluate to the same value on every row.

## The contract

- `fold` is applied bottom-up: fold the children, then apply the rules to the node.
- The language has `Int`, `Bool`, `Col(i)`, `Add`, `Mul`, `Lt` (integer comparison to a boolean), `And`, `Or`, `Not`; all values are non-NULL here.
- A node with only constant children becomes a constant.

## Invariants

These must hold after every step, whatever the input:

- `eval(fold(e), row) == eval(e, row)` for every row where `eval(e, row)` is defined.
- `fold(fold(e)) == fold(e)`.
- The result contains no subtree without a column that is bigger than a literal.

## Relations between input and output

How the answer must change when the input changes. The property tests check these on random inputs:

- Folding never increases the number of nodes.
- A tree with no columns folds to a single literal.
- Substituting values for columns and then folding gives the same literal as evaluating.

## Examples

Worked cases (the tests include them):

```text
(1 + 2) * x -> 3 * x
x * (2 - 2... ) -> 0 when a factor folds to 0
true AND (x < 5) -> x < 5
NOT NOT (x < 5) -> x < 5
```

## What the tests check

- Each rule.
- Nested folding.
- A property: equal value on random rows; idempotence; no growth.

## Done when

All the `s3h_c1` tests pass.
