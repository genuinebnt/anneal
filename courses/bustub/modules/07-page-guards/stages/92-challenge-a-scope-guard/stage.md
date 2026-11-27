A challenge: no walkthrough, no hints, no solution. Solve it with what this module and the ones before it taught. It is extra practice and does not count towards the course.

## What to build

`Defer` in `src/common/defer.rs`: a guard that runs a closure when it goes out of scope, however it goes out of scope (normal exit, early return, `?`, a panic). `cancel()` disarms it; `run_now()` runs the closure immediately, once.

## Why

A page guard is a `Defer` that unpins: the point of RAII is that the cleanup cannot be forgotten on the path nobody tested. Building the general guard shows exactly what the language promises (drop order, drop on unwinding) and what it does not (a leaked guard does not run).

## The contract

- The closure runs exactly once: on drop, or on `run_now()`, or never if `cancel()` was called first.
- Guards drop in reverse order of creation.
- A panic in the scope still runs the guard while unwinding.

## Invariants

These must hold after every step, whatever the input:

- The closure never runs twice.
- After `cancel()` the closure never runs.
- After `run_now()`, dropping does not run it again.

## Relations between input and output

How the answer must change when the input changes. The property tests check these on random inputs:

- The number of runs equals the number of guards that were neither cancelled nor leaked.
- Declaring guards A then B runs B's closure first.
- Moving a guard moves the obligation: it runs once, when the new owner drops.

## Examples

Worked cases (the tests include them):

```text
{ let _g = Defer::new(|| log(1)); log(0); } -> 0, 1
cancel -> never runs
two guards: runs in reverse order
```

## What the tests check

- Run on drop, cancel, run_now.
- Order of several guards, early return and `?`.
- Run on panic.
- Moving a guard.

## Done when

All the `s1g_c1` tests pass.

## Further reading

In the cookbook. Some of these pages show a complete solution, so they are best read after your own attempt.

- [`Drop` and RAII](/cookbook/p/drop-raii): Drop guards that run on return, ? and panic.
