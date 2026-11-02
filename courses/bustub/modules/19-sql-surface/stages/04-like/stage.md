`name like 'a%'` looks like the simplest thing a text column can do, and a first implementation is three lines of recursion. It is also the way to give a database a query that never finishes: `'aaaa…a' like '%a%a%a%a%a%a%b'` makes the obvious matcher try every way of splitting the text between the percents. A database that takes patterns from users has to match them in time it can promise. This stage is a small, complete algorithm with a trap, a model to test it against, and a type rule, wired into the planner as one more expression.

> [!CHECK] `'abcabd' like '%ab%d'`. The matcher is at the first `%`, has matched `ab` after skipping nothing, and then sees `c` where the pattern wants `d`. What does it do next, and which positions does it never need to revisit? What is the least it must remember?
> ||It lets the *same* `%` swallow one more character and tries the rest of the pattern again from there. The only thing it needs to remember is the pair (position in the pattern just after the last `%`, position in the text where that `%` started matching). Earlier percents never need revisiting: whatever an earlier `%` could have swallowed, the later one could swallow too. That is why the work is at most text length times pattern length, however many percents there are.||
>
> - How many characters can `_` match? Does `''` match `_`?
> - What does a trailing `%` change about the final check when the text is used up?
> - Is `'é' like '_'` true? How many bytes is `é`?

## The task

- `text like pattern` and `text not like pattern` in SQL: `%` matches any run (including none), `_` exactly one character, a backslash makes the next character literal (`\%`, `\_`, `\\`). A pattern that ends in a lone backslash matches a backslash.
- Case sensitive, and by **characters**: `_` matches one Unicode scalar value, not one byte.
- `NULL like x`, `x like NULL` and the `not like` forms are NULL. Both operands must be VARCHARs (else `NotImplemented`).
- The matcher `like_matches(text, pattern) -> bool` must be at most proportional to `text length * pattern length`: the tests include a 4000-character text and a pattern with twelve percents and give it ten seconds.

Where to work: the parser's comparison loop (in the expression grammar from 3d-05: `like` and `not like` produce a binary operator named `like`), `get_binary_expression_from_factory` in `src/planner/planner.rs`, and `src/execution/expressions/like_expression.rs`.

## Your freedom

The algorithm (the remembered-percent loop, a dynamic-programming table, a compiled state machine) and how the pattern is represented. If you did the optional LIKE challenge of module 3a you already have a matcher: you may reuse it, as long as it meets the bound.

## The Rust toolbox

**`chars()` and `Vec<char>`.** Collect the text and the pattern into `Vec<char>` and index those: indexing a `&str` by byte position is a bug waiting for the first `é`.

**An enum for pattern tokens.** `Lit(char) | One | Many` after a pass that resolves escapes keeps the matching loop free of special cases.

**A watchdog, not a hope.** A hang is the failure mode of a bad matcher; the test runs the matcher in a thread and fails after ten seconds instead of stalling the suite.

## If this is new

- [S2 Strings and text](/t/s2-strings-text): bytes, chars and `char_indices`.
- Y5 Testing & verification: a slow, obviously-right model.

## Tests

- `%` and `_` in every position, the empty string, the whole text must match.
- Escapes; unicode characters; case sensitivity.
- LIKE and NOT LIKE through SQL, with a pattern taken from an expression; NULLs and type errors.
- The hostile pattern finishes; a model property against try-every-split recursion.

## Hints

### Remember one position, not all of them

The recursion branches at every `%` because it keeps all the alternatives alive. You only ever need to go back to the most recent one: store `(pattern index after it, text index it started at)` and restore it on a mismatch.

### The end of the text is not the end of the pattern

When the text is used up, the pattern may still have `%`s left, and they match the empty string. Anything else left over is a mismatch.

### Decide the escape rules before you code

Write down what `\` followed by a normal letter means (here: the letter, literally) and what a lone trailing backslash means. The model in the tests uses the same rules.

## Performance

The matcher is the whole cost of the operator, so it is worth knowing its bound: **O(n·m)** worst case, close to **O(n + m)** for patterns with one or two percents. A pattern with no wildcards is a string comparison and a pattern `'abc%'` is a prefix test; recognising those shapes is how a real engine makes the common case fast (and how an index range scan becomes possible).

**Measure it.** Match `'%a%a%a%a%a%a%a%a%b'` against a text of 1000, 2000 and 4000 `a`s. The time should grow quadratically at worst, not exponentially. What does it do with 8000?

## Experiment

Optional. Predict first, then run.

1. **Write the recursive version** (try every suffix at each `%`) and run the hostile test with a 20 second watchdog. At what text length does it stop finishing?
2. **Make `_` match one byte** (iterate `as_bytes()`). Which test fails, and what would a user with a Greek or Japanese column see?

## Other designs

- **Translate to a regular expression** and use a regex engine: simple, and pays an automaton construction per pattern unless cached; a backtracking regex engine reintroduces the exponential case.
- **Compile the pattern once** per query (a constant pattern is the common case) into a prefix, a list of inner literal runs and a suffix, and use `str::find` for the runs: fast, and the shape an optimizer needs to turn `like 'abc%'` into a range scan.

## In BusTub

BusTub has no `LIKE`: its string functions are `lower` and `upper` (module 3d). PostgreSQL's own matcher (`like_match.c`) is the algorithm of this stage, with a comment about exactly the exponential case.

## The C/C++ way

| C / C++ | Rust |
|---|---|
| `fnmatch(3)` with `*` and `?` | the same two wildcards, written by hand here |
| `std::regex` | not a good fit: slower, and backtracking |
| `strlen` and byte indexing on UTF-8 | `chars()`: a character is not a byte |

**Port rule:** a pattern is a program. Give it a cost bound, or someone will find the input that exceeds it.

## Learn more

- [PostgreSQL: pattern matching](https://www.postgresql.org/docs/current/functions-matching.html) · [Rust: `str::chars`](https://doc.rust-lang.org/std/primitive.str.html#method.chars)
