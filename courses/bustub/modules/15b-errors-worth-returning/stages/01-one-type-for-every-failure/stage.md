Every layer you have built fails in its own way: the disk manager with an `io::Error`, the engine with an `Exception`, the lexer, the catalog and the page allocator with enums of their own. A client of the database does not care which layer it was. It wants one type it can print, log, hand to another thread and ask *what was this?* of. A function that crosses three layers should be able to write `?` and not think about it.

This stage is the start of that type, `DbError`. **You design it.** The tests do not look inside; they ask four things of it: it says what its cause says, it keeps the cause as a value, it works as a standard error, and it moves across threads.

> [!CHECK] A function returns `Result<T, DbError>` and calls `std::fs::read(path)?`. What makes that compile? And why can you not write one `impl<E: std::error::Error> From<E> for DbError` and be done with every layer?
> ||A `From<io::Error> for DbError`: `?` calls it to convert the error. The blanket version fails because the standard library already has `impl<T> From<T> for T` (every type converts into itself), and once `DbError` is itself an `Error` the blanket impl would also cover `From<DbError> for DbError`: two impls for the same case. Crates such as `anyhow` get their blanket impl by making their error type *not* implement `Error`. Here the error must be a real `Error`, so that it can be boxed, downcast and put in another chain, and the price is one small `From` impl per layer.
>
> - Which conversion does `?` call, and where does it look for it?
> - What would `From<DbError> for DbError` mean, and where does it already exist?
> - What do you give up by not implementing `Error`, and what do you gain?

## The task

In `src/common/db_error.rs`:

- Design `DbError`: the fields are yours. Its job is to say what went wrong and to keep the error that caused it.
- **`Display`:** an error made from a cause says exactly what the cause says.
- **`Error`:** `source()` returns the cause the error keeps, so that a walk of the chain finds it.
- **`From<io::Error>` and `From<Exception>`:** the cause is kept as a **value**: a caller can still ask the chain "are you an `io::Error`, and which kind?". Flattening it to text is the mistake this stage exists to prevent.
- `DbError` is `Send + Sync + 'static`, and converts into `Box<dyn Error + Send + Sync>`.

## Your freedom

Everything about the type: a struct with a message and a boxed cause, an enum with a variant per layer, a wrapper around `Box<dyn Error>`. Whether the message is built when the error is made or when it is shown. How the cause is stored. The tests use `From`, `Display`, `source()` and the chain only, so any of these passes.

## The Rust toolbox

**`Box<dyn Error + Send + Sync>`.** The standard way to hold "some error, I do not know which" so that it can cross threads. `downcast_ref::<T>()` on a `&dyn Error` asks it which type it really is.

**`Display` and `Debug` are different jobs.** `Display` is for people (one line, no trailing newline); `Debug` is for developers and is usually derived.

**`Send + Sync` are checked, not written.** A type is `Send + Sync` if all its parts are. The tests prove it with a function that only accepts such types: if your type holds an `Rc` or a plain `Box<dyn Error>`, it stops compiling.

## If this is new

- [C1 Threads & shared state](/t/c1-threads-shared-state): why an error that crosses a thread must be `Send`.
- [S6 Trait objects](/t/s6-trait-objects): `dyn Error`, and asking a trait object what it is.

## Tests

- An `io::Error` and an `Exception` convert, say what they said, and are still in the chain as values.
- `source()` is the cause, and the chain ends there.
- The error crosses a thread, boxes into `Box<dyn Error + Send + Sync>`, and comes back out as a `DbError`.
- `?` in a function that returns `Result<_, DbError>` converts an `io::Error`.

## Hints

### Start from `source()`

Ask what a caller needs from the chain: to find the original error and look at its kind. That decides how you store the cause: as the error itself, behind a trait object, not as a `String`.

### The message and the cause are two things

An error made from a cause can say what the cause says, and also hold the cause. Say the message once, when the error is made, so that `Display` has nothing to compute.

### `Send + Sync` fails at compile time

If the test file does not compile because of `Send`, a field is not `Send`. The usual cause is a trait object without the `+ Send + Sync` bounds.

## Performance

An error costs two allocations, the boxed cause and the message. That is nothing for failures, which are rare, and a lot if an error is built on a path that usually succeeds: never format an error message before you know you need it.

**Measure it.** Convert a million `io::Error`s into `DbError`s and time it; then do the same with an enum that holds the `io::Error` directly. The difference is the price of a uniform type.

## Experiment

Optional. Predict first, then run.

1. **Keep the cause as a `String`.** Which tests fail, and what could a caller no longer do?
2. **Make `source()` return `None`.** Which test fails? What would an operator reading a log lose?

## Other designs

- **An enum with a variant per layer:** callers `match` on it, and it grows with every layer; the compiler tells you where a new layer needs handling.
- **A wrapper around `Box<dyn Error>`** (what `anyhow` does): opaque and easy, no `match`, one blanket `From` if the type does not implement `Error`.
- **`thiserror`:** derives `Display`, `Error` and `From` from attributes; the code you write here is what it generates.

## In BusTub

BusTub throws C++ exceptions with a type and a message; there is no cause and no code. This module is not part of BusTub. It is closer to PostgreSQL's `ereport`, which carries a code, a message and, in the server log, more.

## The C/C++ way

| C / C++ | Rust |
|---|---|
| `throw bustub::Exception(type, msg)` | `return Err(DbError::from(..))` |
| `e.what()` | `Display` |
| `std::nested_exception` | `source()` |
| `dynamic_cast<const IoError *>(&e)` | `downcast_ref::<io::Error>()` |

**Port rule:** keep the original error; never rebuild it from its message.

## Learn more

- [`std::error::Error`](https://doc.rust-lang.org/std/error/trait.Error.html) · [Rust API Guidelines: C-GOOD-ERR](https://rust-lang.github.io/api-guidelines/interoperability.html#error-types-are-meaningful-and-well-behaved-c-good-err)
