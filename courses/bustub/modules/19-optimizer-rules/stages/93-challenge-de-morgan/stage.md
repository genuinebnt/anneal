A challenge: no walkthrough, no hints, no solution. It is extra practice and does not count towards the course.

## What to fix

`push_not` in `src/optimizer/push_not.rs` pushes every `NOT` of a boolean expression down to the variables, using De Morgan's laws, so that the result has `Not` only directly on variables. One of the two laws is written wrong. Find the bug and fix it.

## Why

A rewrite is correct only if the result means the same thing, and the only convincing evidence is to compare the truth tables. An optimiser that gets it wrong still runs, returns rows, and returns the wrong ones.

> [!CHECK] Once the tests pass: what was the bug, in one sentence, and why did ordinary inputs not show it?
> ||Pushing a `NOT` through an `AND` kept the `AND`. De Morgan: `NOT (a AND b)` is `NOT a OR NOT b`, so a negated `AND` becomes an `OR` (and a negated `OR` an `AND`).||

## The contract

- `push_not(e)` returns an expression with the same truth table as `e` in which `Not` is applied only to variables, and no `Not Not` remains.
- The laws: `NOT (a AND b) = NOT a OR NOT b`, `NOT (a OR b) = NOT a AND NOT b`, `NOT NOT a = a`.

## Invariants

These must hold after every step, whatever the input:

- For every assignment of the variables, `eval(push_not(e)) == eval(e)`.
- `Not` appears only on `Var` nodes in the result.

## Relations between input and output

How the answer must change when the input changes. The property tests check these on random inputs:

- `push_not(push_not(e)) == push_not(e)`.
- `push_not(Not(e))` is the negation of `push_not(e)` on every assignment.
- The number of variables mentioned is unchanged.

## Examples

Worked cases (the tests include them):

```text
NOT (a AND b) -> NOT a OR NOT b
NOT (a OR NOT b) -> NOT a AND b
```

## What the tests check

- Each law.
- Nested negations.
- A property: equal truth tables on every assignment.

## Done when

All the `s3h_c4` tests pass, and you can say in one sentence what the bug was.
