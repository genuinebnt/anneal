---
title: Option and Result combinators: ?, let-else and friends
summary: The small set of methods and syntax that replace nested matches on Option and Result: ?, let-else, map, and_then, ok_or_else, filter, zip, unwrap_or_default, and when to reach for each.
minutes: 6
---
`Option<T>` is "a value or nothing"; `Result<T, E>` is "a value or why not". Both are enums you can `match`, but nesting matches quickly buries the logic. A handful of combinators covers nearly every case:

| you want | write |
|---|---|
| return early on `None`/`Err` | `let x = opt?;` (in a function returning `Option`/`Result`) |
| bind or bail out, with a custom bail | `let Some(x) = opt else { return ..; };` (`let-else`) |
| transform the inside | `opt.map(f)`, `res.map(f)`, `res.map_err(f)` |
| chain a step that may fail | `opt.and_then(f)`, `res.and_then(f)` |
| `None` to an error | `opt.ok_or(err)`, `opt.ok_or_else(\|\| err)` |
| keep only if a test holds | `opt.filter(pred)` |
| both must be present | `a.zip(b)` |
| a default | `opt.unwrap_or(d)`, `unwrap_or_else(f)`, `unwrap_or_default()` |
| test and use | `opt.is_some_and(pred)`, `opt.map_or(default, f)` |
| flip | `opt.ok_or(..)` (Option to Result), `res.ok()` (Result to Option) |

Choose `?` when the failure just propagates, `let-else` when you must do something specific before leaving (a `continue`, a custom error), and `map`/`and_then` when a chain reads better than statements. `unwrap()` is for cases that cannot happen; say why with `expect("..")` or prove it with the type.

## Patterns from this course

- `link.filter(|l| l.is_valid())`: an `Option<UndoLink>` that is only meaningful when valid.
- `get_undo_log_optional(link)?`: the chain walk ends the whole function with `None` when a log has been collected.
- `let Some(logs) = collect_undo_logs(..) else { continue };` in the scan loop: skip a tuple that did not exist for this reader.
- `ctx.txn().cloned().zip(ctx.txn_mgr())`: a transaction **and** its manager, or neither.

## Pitfalls

`unwrap_or(expensive())` computes `expensive()` even when not needed: use `unwrap_or_else`. `ok_or(format!(..))` allocates the string every time: use `ok_or_else`. And `map` with a closure that itself returns an `Option` gives `Option<Option<T>>`; that is `and_then`.

## C++ comparison

| C / C++ | Rust |
|---|---|
| `if (!opt) return std::nullopt;` | `let x = opt?;` |
| `opt.value_or(d)` | `opt.unwrap_or(d)` |
| `opt.has_value() && pred(*opt)` | `opt.is_some_and(pred)` |
| `if (it == end) continue;` | `let Some(x) = it else { continue };` |

**Port rule:** every early-return null check on an optional becomes `?` or `let-else`.

## In real code

### Using it: a lookup written three ways

```rust test
use std::collections::HashMap;

fn nested(m: &HashMap<&str, &str>, k: &str) -> Option<i32> {
    match m.get(k) {
        Some(raw) => match raw.parse::<i32>() {
            Ok(n) => Some(n),
            Err(_) => None,
        },
        None => None,
    }
}

fn question_mark(m: &HashMap<&str, &str>, k: &str) -> Option<i32> {
    let raw = m.get(k)?;
    raw.parse().ok()
}

fn combinators(m: &HashMap<&str, &str>, k: &str) -> Option<i32> {
    m.get(k).and_then(|raw| raw.parse().ok())
}

#[test]
fn the_three_forms_agree() {
    let m: HashMap<_, _> = [("a", "12"), ("b", "x")].into_iter().collect();
    for k in ["a", "b", "zz"] {
        assert_eq!(nested(&m, k), question_mark(&m, k));
        assert_eq!(question_mark(&m, k), combinators(&m, k));
    }
    assert_eq!(combinators(&m, "a"), Some(12));
}
```

### Using it: let-else, zip, filter, ok_or_else

```rust test
fn first_valid_pair(xs: &[Option<i32>], ys: &[Option<i32>]) -> Option<(i32, i32)> {
    for (x, y) in xs.iter().zip(ys) {
        let Some((a, b)) = x.zip(*y) else { continue };
        if a != b {
            return Some((a, b));
        }
    }
    None
}

fn positive(n: i32) -> Result<i32, String> {
    Some(n).filter(|n| *n > 0).ok_or_else(|| format!("{n} is not positive"))
}

#[test]
fn let_else_skips_what_is_incomplete() {
    let xs = [Some(1), None, Some(3), Some(4)];
    let ys = [Some(1), Some(2), None, Some(9)];
    assert_eq!(first_valid_pair(&xs, &ys), Some((4, 9)));
}

#[test]
fn filter_and_ok_or_else_turn_a_test_into_an_error() {
    assert_eq!(positive(3), Ok(3));
    assert_eq!(positive(-1).unwrap_err(), "-1 is not positive");
}
```

### In the exercises

- **0a-01:** `get` is a chain of `?` over `Option`.
- **4a-03:** `collect_undo_logs` uses `?`, `filter` and `let-else`.
- **4a-05:** the scan skips invisible tuples with `let-else`.
- **4b-01:** the context's transaction and manager travel as `Option::zip`.

### Where it is used

- Every Rust codebase; the `Try` trait behind `?` also works for `Option` in functions returning `Option`, and `Result<Option<T>, E>` / `Option<Result<T, E>>` flip with `transpose()`.
