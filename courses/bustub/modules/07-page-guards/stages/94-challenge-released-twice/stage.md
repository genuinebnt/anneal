A challenge: no walkthrough, no hints, no solution. It is extra practice and does not count towards the course.

## What to fix

`src/common/scoped_pin.rs` has a guard that unpins a page when dropped and can also be released early with `release(self)`, which returns the count after unpinning. It looks right, and after an early release the count is one too low. Find the bug and fix it.

## Why

Explicit early release is common (`drop(guard)` is not always possible because you want the return value), and it is the one place where RAII guards go wrong. The fix is a Rust idiom worth knowing.

> [!CHECK] Once the tests pass: what was the bug, in one sentence, and why did ordinary inputs not show it?
> ||`release` unpinned, and then `self` was dropped at the end of `release`, so `Drop` unpinned a second time. Copy out what is needed and call `std::mem::forget(self)` so `Drop` does not run on that path.||

## The contract

- `ScopedPin::new(&counter)` counts one pin; dropping it uncounts it.
- `release(self)` uncounts the pin now and returns the count after that; the guard is consumed and must not uncount again.

## Invariants

These must hold after every step, whatever the input:

- The counter equals the number of live, unreleased guards.
- Every pin is uncounted exactly once.

## Relations between input and output

How the answer must change when the input changes. The property tests check these on random inputs:

- Releasing a guard and dropping a guard have the same effect on the counter.
- `release` returns the count after the release.

## Examples

Worked cases (the tests include them):

```text
new, new -> 2; release one -> returns 1; drop the other -> 0
```

## What the tests check

- Drop, release, and a mix.
- The value `release` returns.
- A property: the counter equals live guards.

## Done when

All the `s1g_c3` tests pass, and you can say in one sentence what the bug was.
