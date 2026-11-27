A challenge: no walkthrough, no hints, no solution. Solve it with what this module and the ones before it taught. It is extra practice and does not count towards the course.

## What to build

`tokenize` in `src/sql/mini_lexer.rs`: split SQL text into tokens. Identifiers (`[A-Za-z_][A-Za-z0-9_]*`, kept as written), integers, single-quoted strings in which `''` is one quote, double-quoted identifiers in which `""` is one quote, the symbols `( ) , ; * + - / = < > .` and the two-character `<= >= <> !=`, and whitespace and comments (`-- to end of line`, `/* ... */`) which produce **no** tokens. Errors carry the byte position where the bad token starts.

## Why

A lexer is the first thing a hostile string meets. The details that matter are exactly the ones a happy-path lexer skips: a quote inside a string, a comment inside a string (not a comment), a comment that never ends, a character that belongs to no token, a multi-byte character. A good lexer says where the problem is and never panics.

## The contract

- `tokenize(sql)` returns `Ok(Vec<Tok>)` or `Err(LexError { pos, kind })`.
- `''` inside a string is a quote character; the token holds the unescaped text. `""` likewise in a quoted identifier.
- `-- ...` runs to the end of the line; `/* ... */` does not nest. `--` and `/*` inside a string or quoted identifier are just text.
- Kinds: `UnterminatedString`, `UnterminatedIdent`, `UnterminatedComment` (position of the opening), `BadChar` (position of the character), `IntegerOverflow`.

## Invariants

These must hold after every step, whatever the input:

- Every error position is a byte offset of a character boundary in the input.
- The function never panics, on any input.
- Tokens appear in the order of the input.

## Relations between input and output

How the answer must change when the input changes. The property tests check these on random inputs:

- Inserting whitespace or a comment between tokens never changes the tokens.
- Concatenating the text of a string token (re-escaped) reproduces the literal.
- Cutting a valid statement in the middle of a string or comment gives the matching `Unterminated...` error.

## Examples

Worked cases (the tests include them):

```text
SELECT 'it''s' -> Ident(SELECT) Str(it's)
a<=b -> Ident(a) Sym(<=) Ident(b)
'abc -> UnterminatedString at 0
/* x -> UnterminatedComment at 0
```

## What the tests check

- Each token class; comments and quotes inside strings.
- Each error kind and its position, including after multi-byte characters.
- A property: random text never panics and error positions are character boundaries.

## Done when

All the `s3d_c1` tests pass.

## Further reading

In the cookbook. Some of these pages show a complete solution, so they are best read after your own attempt.

- [SQL front end](/cookbook/p/sql-front-end): a SQL lexer that handles '' inside strings and folds identifiers.
- [A tiny language](/cookbook/p/edge-dsl-parser): a tokenizer that keeps a byte span on every token for error reporting.
