A challenge: no walkthrough, no hints, no solution. Solve it with what this module and the ones before it taught. It is extra practice and does not count towards the course.

## What to build

`Slots` in `src/common/slot_pair.rs`: a table of balances, each behind its own lock. `transfer(from, to, amount)` moves `amount` between two slots **atomically** (no thread ever sees the amount in neither slot or in both), and `total()` reads every balance at one instant. Any number of threads may call them, in any order of slots, and nothing may deadlock.

## Why

A pool needs two pages at once all the time (a split holds a page and its sibling, a merge two leaves). Two threads asking for the same pair in opposite orders is the textbook circular wait, and it only shows up under load. The cure is not a smarter lock but an agreement: every thread takes locks in the same global order.

## The contract

- `new(&balances)` makes one slot per balance; `balance(i)` is `None` for an unknown slot.
- `transfer(from, to, amount)` is `Err(NoSuchSlot(i))` for an unknown slot (checking `from` first), `Err(Insufficient)` if `from` holds less than `amount` (nothing changes), and otherwise moves the amount. `from == to` succeeds and changes nothing.
- `total()` is the sum of all balances at one instant.

## Invariants

These must hold after every step, whatever the input:

- The sum of all balances never changes.
- No balance goes below zero through a transfer.
- Whatever the interleaving, every call returns.

## Relations between input and output

How the answer must change when the input changes. The property tests check these on random inputs:

- A transfer from `a` to `b` followed by one from `b` to `a` of the same amount restores every balance.
- Two threads transferring in opposite directions between the same two slots both finish.
- `total()` called while transfers run always equals the initial total.

## Examples

Worked cases (the tests include them):

```text
[10, 5]: transfer(0, 1, 4) -> [6, 9]; transfer(1, 0, 20) -> Err(Insufficient)
two threads: 5000 times 0->1 and 5000 times 1->0, both return
```

## What the tests check

- Moves, failures and the no-op transfer.
- Opposite-order threads finish (watchdog).
- Many threads on random pairs conserve the total.
- `total()` is a consistent snapshot.

## Done when

All the `s1g_c5` tests pass.

## Further reading

In the cookbook. Some of these pages show a complete solution, so they are best read after your own attempt.

- [Locks deadlocks](/cookbook/p/locks-deadlocks): lock ordering and try_lock as the cures for deadlock.
- [Concurrency interview questions](/cookbook/p/concurrency-interview-questions): dining philosophers and other multi-lock problems solved without deadlock.
