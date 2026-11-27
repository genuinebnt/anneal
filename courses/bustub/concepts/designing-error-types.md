---
title: Designing error types: enums, From and the question mark
summary: When to return an error and when to panic, how an error enum and From make ? convert between layers, what context an error needs, and what thiserror and anyhow automate.
minutes: 8
---
The first question for any failure: **is it expected, or a bug?** An unreadable file, a duplicate key, a division by zero in user SQL are *expected*: the caller must be able to handle them, so return a `Result`. An index out of bounds from a corrupted invariant is a *bug*: panic with a message that names it. Absence that is not a failure (a lookup that finds nothing) is an `Option`.

## Shape of an error type

For a library (this course is one), define an **enum** with one variant per kind of failure the caller may want to tell apart:

```rust
pub enum DbError {
    Io(std::io::Error),
    NotFound { table: String },
    Conflict(String),
}
```

Implement `Display` (a message for people) and `std::error::Error` (so it composes with `Box<dyn Error>` and `source()`). The `thiserror` crate derives all of that from attributes; this course writes it by hand so you see what the derive does. `anyhow` is for applications: one opaque error type with `.context(..)`; do not return it from a library.

## `?` and `From`

`expr?` means: on `Ok(v)` give `v`; on `Err(e)` **return `Err(From::from(e))`**. So if `DbError: From<std::io::Error>`, a function returning `Result<_, DbError>` can use `?` on an I/O call and the conversion happens by itself. When `?` complains about mismatched error types, the usual fix is a `From` impl, but ask first whether the two layers should share an error type at all.

## Context

A bare error ("No such file") loses where it happened. Add the operation and the object when you propagate: `map_err(|e| DbError::Io { path, source: e })`, or `.context("reading page 7")` with `anyhow`. The message should answer *what was being done to what*.

## What the course does

BusTub's C++ throws `Exception` with an `ExceptionType`; the Rust port returns `Result<_, Exception>` with the same kinds. Expected SQL errors (overflow, divide by zero, a write-write conflict) are `Err(Exception)` that the engine turns into "statement failed"; invariant violations are `assert!`/`panic!`/`unreachable!`. Two spots to read with this in mind: the transaction code taints the transaction *where it creates the error* so no caller can forget; and the constructors (`Watermark::add_txn`, `RobinHoodHashSet::new`) return errors for bad arguments instead of panicking, since a caller may pass user input.

## C++ comparison

| C / C++ | Rust |
|---|---|
| `throw Exception(...)`, caught somewhere | `return Err(Exception::new(...))`, handled by the caller or propagated with `?` |
| `catch (...)` | `match`/`if let Err(e)`; `catch_unwind` only at thread boundaries |
| exception hierarchy | one enum, or `Box<dyn Error>` |
| `noexcept` | a function that returns no `Result` cannot fail *recoverably* |

**Port rule:** a `throw` that a caller is expected to handle becomes a `Result`; a `throw` for "can't happen" becomes a panic.

## In real code

### Using it: an error enum with From and Display

```rust test
use std::fmt;

#[derive(Debug)]
enum DbError {
    Parse(std::num::ParseIntError),
    NotFound(String),
}

impl fmt::Display for DbError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            DbError::Parse(e) => write!(f, "bad number: {e}"),
            DbError::NotFound(k) => write!(f, "no such key: {k}"),
        }
    }
}

impl std::error::Error for DbError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            DbError::Parse(e) => Some(e),
            DbError::NotFound(_) => None,
        }
    }
}

impl From<std::num::ParseIntError> for DbError {
    fn from(e: std::num::ParseIntError) -> DbError {
        DbError::Parse(e)
    }
}

fn lookup(table: &[(&str, &str)], key: &str) -> Result<i32, DbError> {
    let (_, raw) = table.iter().find(|(k, _)| *k == key).ok_or_else(|| DbError::NotFound(key.to_string()))?;
    Ok(raw.parse::<i32>()?) // ? converts ParseIntError into DbError through From
}

#[test]
fn question_mark_converts_with_from() {
    let t = [("a", "12"), ("b", "x")];
    assert_eq!(lookup(&t, "a").unwrap(), 12);
    assert!(matches!(lookup(&t, "b"), Err(DbError::Parse(_))));
    assert!(matches!(lookup(&t, "zz"), Err(DbError::NotFound(_))));
}

#[test]
fn display_and_source_tell_the_story() {
    let e = lookup(&[("b", "x")], "b").unwrap_err();
    assert!(e.to_string().starts_with("bad number"));
    assert!(std::error::Error::source(&e).is_some());
}
```

### Using it: expected failure versus bug

```rust test
/// Expected: the caller can fix its input.
fn new_capacity(n: usize) -> Result<usize, String> {
    if n == 0 { Err("capacity must be positive".into()) } else { Ok(n) }
}

/// A bug if violated: callers hold the invariant; panic loudly with the reason.
fn bucket(hash: usize, capacity: usize) -> usize {
    assert!(capacity > 0, "capacity was validated at construction");
    hash % capacity
}

#[test]
fn bad_input_is_an_error_not_a_panic() {
    assert!(new_capacity(0).is_err());
    assert_eq!(bucket(10, new_capacity(4).unwrap()), 2);
}

#[test]
fn a_broken_invariant_panics_with_its_reason() {
    let r = std::panic::catch_unwind(|| bucket(1, 0));
    let msg = r.unwrap_err().downcast_ref::<&str>().map(|s| s.to_string()).unwrap_or_default();
    assert!(msg.contains("validated at construction"));
}
```

### In the exercises

- **1a-01:** `DiskManager::new` returns the `io::Error` instead of panicking when a file cannot be opened; `rust_io_errors` covers their kinds.
- **3d-01:** arithmetic errors in SQL (overflow, division by zero) are `Exception`s, distinguished from panics.
- **4a-01:** `Watermark::add_txn` returns an error for an impossible read timestamp.
- **4b-02:** `write_write_conflict` builds the error and taints the transaction in one place.

### Where it is used

- **`thiserror`** (libraries) and **`anyhow`** (applications) are the de-facto pair; `std::io::Error` is the model of a library error with a `kind()` and a `source()`.
- The `rust-skills` error guide asks the same first question, "is this failure expected or a bug?", before choosing `Result`, `Option` or `panic!`.
