A storage engine reads bytes that someone else wrote, or that a crash left half-written. Bad bytes are not a bug in your program: they are a normal answer, and the code must say **what** is wrong and **where**, without panicking. This stage builds a parser for a small record format in `src/rust_primer/records.rs`, and the error types around it.

A stream of records is, record after record: `key_len` (u8), `value_len` (u16, little-endian), the key's bytes (UTF-8), the value's bytes.

## The task

- `encode_records` writes the stream; a key longer than 255 bytes is `FormatError::KeyTooLong`, a value longer than 65 535 is `ValueTooLong` (use `u8::try_from` / `u16::try_from` and `map_err`).
- `parse_records` reads it back. A stream that stops in the middle of a record is `Truncated { at, needed, have }`: the byte where the record starts, how many bytes it needs in all, and how many are left. A key that is not UTF-8 is `BadKey { at }`. **It never panics, whatever the bytes are.**
- `Display` for both error types (one sentence that names its numbers), `Error::source()` for `LoadError` (the wrapped error), and the two `From` conversions, so that `?` turns an `io::Error` or a `FormatError` into a `LoadError`.
- `load_records(path)` reads a file and parses it. A missing file and a damaged file are different errors of one type.

The tests: exact bytes; round trips; each error with its numbers; the messages; `source()`; and three properties: **records round-trip**, **parsing never panics on any bytes**, and **a stream cut anywhere is either a shorter list of the same records or a `Truncated` that accounts for every byte**.

## Your freedom

The error types are given (they are the contract); how you walk the bytes is yours: an index in a `while`, a slice you keep shortening, a cursor type of your own. The messages are yours as long as they contain their numbers.

## The Rust toolbox

**Errors are enums.** One variant per way to go wrong, each carrying the facts: `Truncated { at, needed, have }` tells the caller (and the person reading the log) more than a string does.

**`Display` and `Error`.** `impl fmt::Display` writes the message (`write!(f, "...{at}...")`); `impl std::error::Error` makes the type usable as a `Box<dyn Error>`; `source()` returns the wrapped error so that a chain of causes can be printed.

**`?` and `From`.** `expr?` means "on `Err(e)`, return `Err(From::from(e))`". With `impl From<io::Error> for LoadError`, a function returning `Result<_, LoadError>` can use `?` on an `io::Result` directly.

```rust
let bytes = std::fs::read(path)?;      // io::Error -> LoadError::Io
Ok(parse_records(&bytes)?)             // FormatError -> LoadError::Format
```

**Fallible conversions.** `u8::try_from(len)` is `Err` when `len > 255`; `map_err(|_| FormatError::KeyTooLong { len })` gives the error your type.

**Checking before indexing.** `rest.len() < 3` first, then `rest[0]`; or `rest.get(..3)`, which is `None` instead of a panic.

## If this is new

- L8 Error design: enums as errors, `Display`, `source`.
- [S1 Option & Result](/t/s1-option-result): `?`, `map_err`.
- [S2 Strings & text](/t/s2-strings-text): `std::str::from_utf8`.

## Tests

- Exact bytes; round trips; the empty stream.
- `Truncated` with its three numbers (a missing last byte; not even a whole header); `BadKey` with the offset of the bad record; the limits 255 and 65 535.
- The messages contain their numbers; `LoadError::source()`; a missing file against a damaged file.
- Properties: round trip; no panic on any bytes; a cut stream.

## Hints

### Decide the order of the checks

`Truncated` before `BadKey`: a key you cannot fully read cannot be checked for UTF-8. Check the lengths first.

### `at` is where the record starts

Not where the missing byte is. A reader of the log wants to find the record that is broken.

### The no-panic property is the real test

Every `bytes[i]` and every `bytes[a..b]` is a possible panic. If the property fails, its shrunk input is the shortest bytes that panic.

## Performance

Parsing is one pass: `O(n)`, and a record's key is copied once into a `String`. Returning `&str` borrowed from the input would avoid the copy and tie the result to the input's lifetime.

**Measure it.** Parse 100 000 records of 10-byte keys and values; the time is dominated by the allocations of the keys and values.

## Experiment

Optional. Predict first, then run.

1. **Panic on purpose.** Replace the `Truncated` check with `bytes[at + total - 1]`. Which property finds it, and what does it shrink to?
2. **A `String` error.** Use `Result<_, String>`. What can the tests no longer ask?

## Other designs

- **`Box<dyn Error>` everywhere:** quick, but the caller cannot tell the failures apart.
- **A crate such as `thiserror`** writes `Display` and `From` for you; writing them once by hand shows what it generates.
- **A `nom`-style parser combinator:** the same parser as small composable pieces.

## In BusTub

BusTub's C++ returns `bool`, `std::optional` or throws `Exception`; its disk manager logs and carries on. Rust has no exceptions: from module 1a on, an I/O failure is an `io::Result`, and from module 4c a damaged log is an error value that recovery reads, never a crash.

## The C/C++ way

| C / C++ | Rust |
|---|---|
| `return -1; errno = EIO;` | `Err(LoadError::Io(e))` |
| `throw std::runtime_error("bad")` | `Err(FormatError::BadKey { at })` |
| `if (len < 3) return false;` before reading | `if rest.len() < 3 { return Err(...) }` |
| `catch (...)` at the top | `match load_records(p) { Err(LoadError::Format(e)) => ... }` |

**Port rule:** an error is a value the caller can match on, with the numbers a person needs to find the fault.

## Learn more

- [`std::error::Error`](https://doc.rust-lang.org/std/error/trait.Error.html) · [`From` and `?`](https://doc.rust-lang.org/book/ch09-02-recoverable-errors-with-result.html#a-shortcut-for-propagating-errors-the--operator) · [`str::from_utf8`](https://doc.rust-lang.org/std/str/fn.from_utf8.html)
