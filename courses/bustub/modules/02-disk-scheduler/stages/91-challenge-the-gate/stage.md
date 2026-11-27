A challenge: no walkthrough, no hints, no solution. It is extra practice and does not count towards the course.

## What to fix

`src/common/gate.rs` is a complete one-shot gate: threads wait at it until somebody opens it, and once open it stays open. It looks right, and it has one bug. Find it with the tests and fix it.

## Why

A concurrency bug rarely announces itself: the code passes when one thread waits and fails when several do, and the symptom is a program that never finishes, not an error. Learning to read "something is still waiting" and ask *who should have woken it* is the skill.

> [!CHECK] Once the tests pass: what was the bug, in one sentence, and why did ordinary inputs not show it?
> ||Opening the gate woke one waiter (`notify_one`); every other thread waiting at it slept on. A gate that stays open must wake everyone: `notify_all`.||

## The contract

- `wait` returns once the gate is open, for every thread that waits, however many, and for threads that arrive after it was opened.
- `open` may be called more than once.

## Invariants

These must hold after every step, whatever the input:

- Once open, a gate is never closed again.
- No thread is left waiting at an open gate.

## Relations between input and output

How the answer must change when the input changes. The property tests check these on random inputs:

- The number of threads released by `open` equals the number that were waiting, not one.
- Waiting after the gate is open returns at once, for any number of callers.

## Examples

Worked cases (the tests include them):

```text
4 waiters, open -> 4 released
open, then wait -> returns immediately
two gates: opening one releases only its waiters
```

## What the tests check

- A closed gate holds its waiter; an open one lets everybody through, including late arrivals.
- Every waiter goes on, not just one.
- Gates are independent.

## Done when

All the `s1b_c2` tests pass, and you can say in one sentence what the bug was.

## Further reading

In the cookbook. Some of these pages show a complete solution, so they are best read after your own attempt.

- [A channel from a mutex and condition variables](/cookbook/p/mutex-condvar-channel): condition variables from scratch: who gets woken, and rechecking after wake-up.
- [`std::sync` and thread coordination by use case](/cookbook/p/std-sync-primitives-use-cases): when every waiter must see a change, so notify_all is required.
