Every layer you have built fails in its own way: the disk manager with an `io::Error`, the engine with an `Exception`, the lexer, the catalog and the page allocator with enums of their own. A client of the database does not care which layer it was. It wants one type it can print, log, hand to another thread and ask *what was this?* of. A function that crosses three layers should be able to write `?` and not think about it.

And when the error reaches the top, `no such file` is a true message and a useless one. Which file? Opened by whom, for what? Six layers up, the code that called `open` knew: it was starting the log. The layers in between knew nothing and should not have to. The answer is **context**: each layer, as the error passes up through it, adds one sentence about what it was doing, and the original error stays underneath, untouched, for a program that wants to know what really happened.

This stage builds that type, `DbError`, and **you design it**. The tests do not look inside. They ask that it says what its cause says, keeps the cause as a value, works as a standard error, moves across threads, takes every layer's error through `?`, and grows one layer of context at a time.

> [!CHECK] `read_page(id)` fails with an I/O error "no such file". Three functions up, the log should say: "starting up: opening the log: reading page 7: no such file". Which of those pieces does each function know? And what must `Display` show for the top error, if the log line is the *report*?
> ||Each function knows only what *it* was doing: `read_page` knows the page, its caller the log, the top one that the database was starting. The I/O error knows only the file problem. `Display` shows **one layer**, the outermost ("starting up"): it is what a status line prints. The log line is the **report**, which joins every layer's message from the outside in. If `Display` printed everything, the report would repeat the cause once per layer.||
>
> - Which function adds which piece, and in what order does the report read them?
> - What would a status line look like if `Display` showed the whole chain?
> - What is the root cause here, and who needs it?

## The task

In `src/common/db_error.rs`. After this stage:

- **`DbError` keeps its cause as a value.** `From<io::Error>` and `From<Exception>` make an error that says exactly what its cause says (`Display`) and keeps the cause (`source()`), so a caller can still ask the chain "are you an `io::Error`, and which kind?". Flattening the cause to text is the mistake this stage exists to prevent. `DbError` is `Send + Sync + 'static` and converts into `Box<dyn Error + Send + Sync>`.
- **Every layer converts with `?`.** `LexError`, `IntError`, `IntOpError`, `LikeError`, `NameError`, `ResolveError`, `FreeError` and `SealError` each convert into a `DbError` that keeps them in the same way; the template has eight stub `From` impls to replace. Seven of them already say what they mean (their `Display` is given in this file). The lexer's error is yours: it must say what went wrong and **where**, with the phrase below and its position as a number.
- **Each layer can add context.** `context(message)` is a new error whose `Display` is `message` and nothing else, and whose cause is the old error. `chain()` iterates over the error, its cause, the cause's cause, to the innermost; `root_cause()` is the last of them; `report()` joins every layer's message, outermost first, with `": "`, and a layer that says exactly what the one below it says (an error made by `From`) is shown once.
- **Context on a result.** `.context(..)` and `.with_context(|| ..)` on any `Result` whose error converts into a `DbError` convert, then add the layer; `with_context` builds its message only if there is an error.

| `LexErrorKind` | phrase the message must contain, with the position |
|---|---|
| `UnterminatedString` | `unterminated string` |
| `UnterminatedIdent` (a quoted identifier) | `unterminated` |
| `UnterminatedComment` | `unterminated comment` |
| `BadChar` | `unexpected character` |
| `IntegerOverflow` (an integer literal) | `out of range` |

If you gave one of these error types a `Display` of your own in a challenge, keep one of the two impls and delete the other: two impls for one type do not compile.

> [!ASIDE] The steps, if you would rather not work them out
> 1. A struct with a `message: String` and a `source: Option<Box<dyn Error + Send + Sync + 'static>>`, and a private helper that builds one from any cause: the cause's `to_string()` as the message, the cause boxed as the source.
> 2. `Display` writes the message; `source()` returns the boxed cause as a `&(dyn Error + 'static)`.
> 3. `From<io::Error>` and `From<Exception>` call the helper. A `macro_rules!` that writes one `From` per type does the other eight.
> 4. `Display` for `LexError`: a `match` on the kind for the phrase, then `write!(f, "{what} at position {}", self.pos)`.
> 5. `context` builds a `DbError` with the new message and `self` boxed as the source. `chain` is `std::iter::successors` over `source()`, boxed; `root_cause` is its `last()`; `report` pushes each message unless it equals the last one pushed, then joins.
> 6. `Context` for `Result`: `map_err` to `DbError::from(e).context(..)`; `with_context` calls the closure inside the `map_err`.

## Your freedom

Everything about the type: a struct with a message and a boxed cause, an enum with a variant per layer, a wrapper around `Box<dyn Error>`. Whether the message is built when the error is made or when it is shown. How the chain is walked (`successors`, an iterator of your own, a loop) and how the layers are stored. Whether the eight conversions are written by hand or by one macro, and whether the repeated-message rule is a comparison of strings or something smarter. `chain()` returns a boxed iterator so that the stub compiles before you write it. The tests use `From`, `Display`, `source()`, `?` and the public methods only, so any of these designs passes.

## The Rust toolbox

**`Box<dyn Error + Send + Sync>`.** The standard way to hold "some error, I do not know which" so that it can cross threads. `downcast_ref::<T>()` on a `&dyn Error` asks it which type it really is.

**`Display` and `Debug` are different jobs.** `Display` is for people (one line, no trailing newline); `Debug` is for developers and is usually derived. `write!(f, ...)` shows a variant's data, such as a position.

**`Send + Sync` are checked, not written.** A type is `Send + Sync` if all its parts are. The tests prove it with a function that only accepts such types: if your type holds an `Rc` or a plain `Box<dyn Error>`, the test file stops compiling.

**`?` calls `From::from`.** Nothing else happens: the error type of the function is the target, the error type of the expression is the source.

**A macro for repeated impls.** Eight conversions with one shape are what `macro_rules!` is for: `$(impl From<$t> for DbError { .. })*`. Write one by hand first; write the macro when you see the third copy.

**An extension trait on `Result`.** `Context<T>` is implemented for every `Result<T, E>` whose `E` converts into a `DbError`, so `parse(x).context("...")?` works on any layer's result and on a `DbResult` alike.

**`successors` and a closure for laziness.** `std::iter::successors(first, |e| next)` turns "and then ask for the next one" into an iterator; `|&e| e.source()` copies the reference out. `with_context(|| format!(..))` passes code, not a value: the message is not built unless it is needed.

## If this is new

- [C1 Threads & shared state](/t/c1-threads-shared-state): why an error that crosses a thread must be `Send`.
- S6 Trait objects: `dyn Error`, asking a trait object what it is, walking a chain of `&dyn Error`.
- [S3 Vec & slices](/t/s3-vec-slices): joining pieces into a report string.

## Tests

- An `io::Error` and an `Exception` convert, say what they said and stay in the chain as values; `source()` is the cause; `?` converts in a function that returns a `DbError`.
- The error crosses a thread, boxes into `Box<dyn Error + Send + Sync>` and comes back out as a `DbError`.
- Every lower layer's error converts with `?` and is kept as a value; a lexer error says what went wrong and at which position.
- A context shows only its own message and wraps the old error; `chain()` counts every layer, `root_cause()` is the original, `report()` reads outermost first with the cause once; `.context` and `.with_context` on a result convert and wrap, the second building its message only on failure.

## Hints

### Start from `source()`

Ask what a caller needs from the chain: to find the original error and look at its kind. That decides how you store the cause: as the error itself, behind a trait object, not as a `String`. Then say the message once, when the error is made, so that `Display` has nothing to compute.

### Why not one blanket `From`?

`impl<E: Error> From<E> for DbError` would cover every layer at once, and it does not compile: the standard library already has `impl<T> From<T> for T`, and once `DbError` is itself an `Error` the blanket impl also covers `From<DbError> for DbError`. Crates such as `anyhow` get their blanket impl by making their error type *not* implement `Error`. Here it must be a real `Error` (boxed, downcast, put in another chain), and the price is one small `From` per layer.

### `Send + Sync` fails at compile time

If the test file does not compile because of `Send`, a field is not `Send`. The usual cause is a trait object without the `+ Send + Sync` bounds.

### Count the elements by hand first

An error made by `From` from an `io::Error`, with two contexts on top: write down the four elements of the chain and what each says. The report has three sentences, not four, because the `From` layer says what its cause says.

### The chain is the whole design

`source()` already gives one step. `chain()` is "step until it runs out", and `root_cause()` and `report()` are one line each on top of it. `with_context` is `context` with a closure: one calls the other, or both call a helper; what differs is only when the message is built.

## Performance

An error costs two allocations, the boxed cause and the message, and each context layer one more, all on the failure path. That is nothing for failures, which are rare, and a lot if an error is built on a path that usually succeeds: never format a message before you know you need it. `report()` walks the chain once; a chain of a thousand layers is a thousand string pushes, not a thousand re-scans.

**Measure it.** Convert a million `io::Error`s into `DbError`s and time it; then do the same with an enum that holds the `io::Error` directly: the difference is the price of a uniform type. Then build an error with 2 000 layers and time `report()`; if it grows faster than the number of layers, something rescans.

## Experiment

Optional. Predict first, then run.

1. **Keep the cause as a `String`.** Which tests fail, and what could a caller no longer do?
2. **Let `Display` show the whole chain.** Which tests fail? What does a status line look like now?
3. **Leave the position out of the lexer message.** Which test fails, and how would a user find the mistake in a 40-line statement?

## Other designs

- **An enum with a variant per layer:** callers `match` on it, and it grows with every layer; the compiler tells you where a new layer needs handling.
- **A wrapper around `Box<dyn Error>`** (what `anyhow` does): opaque and easy, one blanket `From` because the type does not implement `Error`; its `{:#}` prints the chain on one line and `{:?}` as a list ("Caused by:").
- **`thiserror`:** derives `Display`, `Error` and `From` from attributes; the code you write here is what it generates.
- **Structured context:** key-value fields (`table = "orders"`, `page = 7`) instead of sentences, so a log can be searched; PostgreSQL has detail and context fields.

## In BusTub

Not part of BusTub, which throws C++ exceptions with a type and a message: no cause and no code. The nearest C++ tool is `std::nested_exception`, which few codebases use. This module is closer to PostgreSQL's `ereport`, which carries a code, a message and, in the server log, more.

## The C/C++ way

| C / C++ | Rust |
|---|---|
| `throw bustub::Exception(type, msg)` | `return Err(DbError::from(..))` |
| `e.what()` per class | `Display` per type |
| a base class so that all exceptions share one catch | `From` per layer into one type, converted by `?` |
| `std::throw_with_nested(Exception(..))`, `std::rethrow_if_nested` in a loop | `.context("..")`, `chain()` |
| `dynamic_cast<const IoError *>(&e)` | `downcast_ref::<io::Error>()` |

**Port rule:** keep the original error and never rebuild it from its message; an error crossing a layer gains a sentence, it does not lose its cause.

## Learn more

- [`std::error::Error`](https://doc.rust-lang.org/std/error/trait.Error.html) · [Rust API Guidelines: C-GOOD-ERR](https://rust-lang.github.io/api-guidelines/interoperability.html#error-types-are-meaningful-and-well-behaved-c-good-err)
- [`macro_rules!`](https://doc.rust-lang.org/reference/macros-by-example.html) · [`std::iter::successors`](https://doc.rust-lang.org/std/iter/fn.successors.html) · [`anyhow::Context`](https://docs.rs/anyhow/latest/anyhow/trait.Context.html) · [`thiserror`](https://docs.rs/thiserror)
