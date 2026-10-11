The message of an error is for a person and changes whenever someone improves the wording. A program that retries on a conflict, a driver that turns a missing table into an exception of its own, a dashboard that counts failures by kind: none can be built on English. They need a **code**. SQL standardised one, SQLSTATE: five characters, the first two the class. PostgreSQL, MySQL and most client libraries carry it.

This stage gives `DbError` a code, and a second face for the client: what a stranger on the other end of the connection may be shown.

> [!CHECK] A `DbError` has three context layers on top of a division by zero. Where should its code come from, and what does that guarantee when someone adds a fourth layer? Then: an I/O error says "/var/lib/bustub/secret.db: permission denied". What should the client see?
> ||From the **root cause**: the innermost error, the thing that really went wrong. A context layer says what the caller was doing, not what happened, so adding one cannot change the code, and this holds by construction rather than by everyone's care. The client should see the code and the words "internal error": a path in a message tells a stranger how the system is laid out, and a pointer value or a table name tells an attacker where to look. The log keeps everything.
>
> - Which element of the chain decides the code, and why not the outermost?
> - Which failures are the client's mistakes, and which are the system's?
> - What would an attacker learn from the original message?

## The task

In `src/common/db_error.rs`:

- **`code()`:** a five-character code, digits and capital letters, read from the **innermost cause**. Anything not in the table is `XX000`.
- **`client_message()`:** `ERROR <code>: <message>` where the message is this error's own `Display`; but for codes of class `58` and class `XX` it is `ERROR <code>: internal error`, with nothing else.

| cause | code |
|---|---|
| an `io::Error`; an `Exception` of kind `Io` | `58030` |
| `Exception`: `Invalid` | `22023` |
| `Exception`: `OutOfRange`, `Decimal`; `IntError::OutOfRange`; `IntOpError::Overflow` | `22003` |
| `Exception`: `Conversion` | `22018` |
| `Exception`: `DivideByZero`; `IntOpError::DivisionByZero` | `22012` |
| `Exception`: `UnknownType` | `42704` |
| `Exception`: `MismatchType`, `IncompatibleType` | `42804` |
| `Exception`: `NotImplemented` | `0A000` |
| `Exception`: `Execution`, `Optimizer`; `FreeError` | `XX000` |
| `IntError::Empty`, `IntError::Invalid` | `22P02` |
| `LikeError` | `22025` |
| `NameError::Exists` / `Missing` / `Invalid` | `42P07` / `42P01` / `42602` |
| `LexError` | `42601` |
| `ResolveError::UnknownTable` / `UnknownColumn` / `Ambiguous` | `42P01` / `42703` / `42702` |
| `SealError` | `XX001` |

## Your freedom

Where the table lives (a `match`, a lookup, a method on each layer's error); whether the code is stored when the error is made or computed when asked; how you find the innermost cause. Whether the "internal" test is on the class or on a list of codes.

## The Rust toolbox

**`downcast_ref` on the root cause.** The root of the chain is a `&dyn Error`; asking it which concrete type it is, one `downcast_ref` per layer, is how a code is chosen without every layer knowing about codes.

**A default arm that is safe.** The last answer, `XX000`, is what a new layer gets before anyone adds it to the table: the failure is reported, as an internal error, and nothing leaks.

**`starts_with` on a code.** Class is the first two characters; no enum is needed to ask "is this a system failure?".

## If this is new

- [S6 Trait objects](/t/s6-trait-objects): `is::<T>()` and `downcast_ref::<T>()`.

## Tests

- Every cause in the table has its code.
- A code is five characters of digits and capitals; context never changes it.
- A client's own mistake is shown with its message: `ERROR 42P07: creating table orders`.
- A system failure shows only `internal error`, whatever its message and context said.

## Hints

### Ask the root, not the top

Write `code()` as: find the root cause, then test which type it is. If a context layer changes your answer, you asked the wrong element.

### The default is a decision

Do not panic on an unknown cause and do not return a made-up class. `XX000` says "the system failed in a way nobody classified" and hides the message.

### One condition for what the client sees

Class 58 and class XX are both "inside". If you find yourself listing codes one by one, the first two characters already tell you.

## Performance

A code is a walk of the chain and a few type checks, on the failure path only. It is cheap enough to compute when asked; storing it in the error buys nothing and costs a field.

**Measure it.** Ask for the code of a 100-layer error a million times. If it is slow, you are formatting something you do not need.

## Experiment

Optional. Predict first, then run.

1. **Read the code from the outermost layer.** Which test fails? What does adding context do to a division by zero?
2. **Show the message for every class.** Which test fails, and what does the log line of the failing test now contain?

## Other designs

- **The code stored in the error:** chosen where the error is made, by the layer that knows best; simple to read, easy to get inconsistent.
- **PostgreSQL's way:** the code is the primary datum (`errcode(ERRCODE_...)`), and the message is written next to it at the site of the failure.
- **Typed classes:** an enum of classes instead of strings, converted to the five characters at the edge.

## In BusTub

Not part of BusTub. SQLSTATE is the standard (SQL:1999) that PostgreSQL, MySQL's `SQLSTATE`, ODBC and JDBC all share.

## The C/C++ way

| C / C++ | Rust |
|---|---|
| `errcode(ERRCODE_UNDEFINED_TABLE)` | `code()` read from the root cause |
| a big `switch` on an exception type | `downcast_ref` per layer, with a safe default |

**Port rule:** what leaves the process is a decision, not an accident of a message.

## Learn more

- [PostgreSQL: error codes](https://www.postgresql.org/docs/current/errcodes-appendix.html) · [PostgreSQL: error reporting](https://www.postgresql.org/docs/current/error-message-reporting.html)
