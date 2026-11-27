A challenge: no walkthrough, no hints, no solution. Solve it with what this module and the ones before it taught. It is extra practice and does not count towards the course.

## What to build

`print_expr` in `src/sql/mini_expr.rs`: print an arithmetic expression tree as text with **as few parentheses as possible**, such that parsing the text (a parser is given) gives the same tree back. `+ -` bind weaker than `* /`, all four are left-associative, unary minus binds tightest.

## Why

`EXPLAIN`, error messages, view definitions and logged plans all print expressions, and a printer that parenthesises everything is unreadable while one that parenthesises too little changes the meaning. Knowing exactly when a parenthesis is needed is the same knowledge as writing the parser.

## The contract

- `Expr` is `Num(i64)`, `Neg(Box<Expr>)` or `Bin(Box<Expr>, Op, Box<Expr>)` with `Op` one of `+ - * /`.
- Print a child in parentheses exactly when needed: a child of lower precedence than its parent; or of the **same** precedence on the **right** of a left-associative operator; or a `Bin` under `Neg`; a negative number on the right of an operator needs none beyond the `-` sign, but `Neg(Num(3))` prints `-3`.
- Operators are printed with one space on each side.

## Invariants

These must hold after every step, whatever the input:

- `parse(print(e)) == Some(e)` for every tree.
- The printed text has no pair of parentheses that could be removed without changing the parse.

## Relations between input and output

How the answer must change when the input changes. The property tests check these on random inputs:

- Wrapping any subtree in redundant parentheses is never produced.
- The number of parentheses pairs is the same for trees that differ only in the numbers.
- `print` is deterministic and total.

## Examples

Worked cases (the tests include them):

```text
Bin(1 - Bin(2 - 3)) -> 1 - (2 - 3)
Bin(Bin(1 - 2) - 3) -> 1 - 2 - 3
Bin(1 + Bin(2 * 3)) -> 1 + 2 * 3
Neg(Bin(1 + 2)) -> -(1 + 2)
```

## What the tests check

- Parentheses on the right and the left, by precedence.
- Negation.
- A property: round trip and minimality over random trees.

## Done when

All the `s3d_c2` tests pass.

## Further reading

In the cookbook. Some of these pages show a complete solution, so they are best read after your own attempt.

- [One expression language, four parsers](/cookbook/p/expression-parsing-three-ways): a minimal-parenthesis printer used as an oracle for four parsers.
