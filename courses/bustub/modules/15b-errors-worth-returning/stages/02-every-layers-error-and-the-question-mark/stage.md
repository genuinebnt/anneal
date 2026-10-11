Eight error types in this engine have nothing but `Debug`: the integer parser, the integer operations, `LIKE`, the name catalog, the lexer, the column resolver, the page bitmap and the checksum. They tell a developer which variant they are and tell a user nothing. To become causes in a chain they must be real errors, and a real error says what it means in words.

This is a short stage of one idea repeated eight times: say each failure in a sentence, and convert each layer into the one type.

> [!CHECK] `IntError` has three variants (empty, invalid, out of range) and `NameError` three (exists, missing, invalid). Write the message of each in one sentence. Which of them needs more than the variant's name, and what does the lexer's error need that the others do not?
> ||An empty input, an invalid integer and an out-of-range one each say what is wrong with the *text*; a name that exists, is missing or is invalid says what is wrong with the *name*. The lexer's error is the one that must say **where**: a position, because "unexpected character" is useless without it. A message should be readable alone, in a log line, without the code that produced it. The same word in two variants ("invalid" for an integer and for a name) is fine; two variants of one type with the same message are not, because the log can no longer tell them apart.
>
> - Which messages would you not understand if you saw them alone in a log?
> - Which variant carries data that the message should show?
> - Why must two variants of one type never share a message?

## The task

In `src/common/db_error.rs`:

- Give `IntError`, `IntOpError`, `LikeError`, `NameError`, `LexError`, `ResolveError`, `FreeError` and `SealError` a `Display`. They already implement `Error`. The messages must contain the phrases below (lower-case, any wording around them).
- Add a `From` for each, so that `?` converts them into `DbError`. Each keeps its cause as a value, like the two of the last stage. The template has eight stub impls; replace them (by hand or with one macro) and delete the stubs.
- A `DbError` made from one of these says exactly what its cause says.

| error | phrase the message must contain |
|---|---|
| `IntError` | `empty`, `invalid`, `out of range` |
| `IntOpError` | `overflow`, `division by zero` |
| `LikeError` | `escape` |
| `NameError` | `already exists`, `does not exist`, `invalid` |
| `LexError` | by kind: `unterminated string`, `unterminated` (a quoted identifier), `unterminated comment`, `unexpected character`, `out of range` (an integer literal); every one also shows the position as a number |
| `ResolveError` | `unknown table`, `unknown column`, `ambiguous` |
| `FreeError` | `already free`, `out of range` |
| `SealError` | `too short`, `checksum` |

If you already wrote a `Display` for one of these types in an earlier challenge, keep one and delete the other: two impls for one type do not compile.

## Your freedom

The wording around the phrases. Whether the eight `From`s are written by hand or by a macro. Whether `Display` lives here or next to each type.

## The Rust toolbox

**A macro for repeated impls.** Eight conversions with one shape are what `macro_rules!` is for: `$(impl From<$t> for DbError { .. })*`. Write one by hand first; write the macro when you see the third copy.

**`?` calls `From::from`.** Nothing else happens: the error type of the function is the target, the error type of the expression is the source.

**`Display` for data-carrying variants.** `ResolveError::Ambiguous(Vec<usize>)` has a payload; `LexError` has a position. `write!(f, ...)` shows them.

## If this is new

- [S6 Trait objects](/t/s6-trait-objects): why a type needs `Display` and `Debug` before it can be an `Error`.

## Tests

- Each of the eight types says what it should, with the phrases above, and its `DbError` says the same.
- Each is kept as a value in the chain.
- Different variants of one type have different messages.
- `?` converts every one of them.

## Hints

### Start with one table

Write the message of every variant as a list first. If two entries are the same, one of them is not saying what is different about it.

### The position is data

The lexer's error has a position field; the message must show it. A message that says only "unexpected character" is what a developer gets from `Debug` already.

### A macro is not needed to start

Write the first two `From`s by hand. When the third looks the same, replace them with a macro; the tests do not care.

## Performance

A `Display` is formatted only when someone prints the error. The conversion itself, which is on the failure path, boxes the cause and copies its message once.

**Measure it.** Time converting and printing a million `IntError`s. Then build the message lazily (store the cause and format on demand) and see what changes: the cost moves, it does not vanish.

## Experiment

Optional. Predict first, then run.

1. **Give two `NameError` variants the same message.** Which test notices? What would you see in a log?
2. **Leave the position out of the lexer message.** Which test fails, and how would a user find the mistake in a 40-line statement?

## Other designs

- **One `Display` per layer by derive:** `thiserror` writes it from a message attribute on each variant.
- **Error codes first, messages later:** PostgreSQL defines the code and the message template together, in one table.

## In BusTub

Not part of BusTub. BusTub's `Exception` already has a message; this module gives every other layer one too.

## The C/C++ way

| C / C++ | Rust |
|---|---|
| `std::exception::what()` per class | `Display` per type |
| a base class so that all exceptions share one catch | `From` per layer into one type, converted by `?` |

**Port rule:** a layer's error is not an error until it can say what it means.

## Learn more

- [`macro_rules!`](https://doc.rust-lang.org/reference/macros-by-example.html) · [`thiserror`](https://docs.rs/thiserror)
