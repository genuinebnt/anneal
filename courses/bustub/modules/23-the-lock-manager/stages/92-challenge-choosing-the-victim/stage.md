A challenge: no walkthrough, no hints, no solution. Solve it with what this module and the ones before it taught. It is extra practice and does not count towards the course.

## What to build

`choose_victim` in `src/concurrency/victim.rs`: given the transactions on a deadlock cycle, each with its id, its start timestamp, the number of locks it holds and the amount of work it has done, pick the one to abort under a policy: `Youngest` (the latest start), `FewestLocks`, or `LeastWork`. **Ties are broken by the larger id** (the newer transaction).

## Why

Every cycle needs a victim, and the choice is a trade-off: the youngest has probably done the least, the one with the fewest locks disturbs the fewest others, the one with the least work wastes the least. Whatever the policy, the choice must be deterministic and must eventually let an old transaction finish (a victim that restarts with a *new* timestamp can starve; keeping the original start prevents it).

## The contract

- `choose_victim(policy, cycle)` returns the index into `cycle` of the victim; `None` for an empty cycle.
- `Youngest`: the largest `start_ts`. `FewestLocks`: the smallest `locks`. `LeastWork`: the smallest `work`. Ties go to the transaction with the larger `id`.

## Invariants

These must hold after every step, whatever the input:

- The victim is a member of the cycle.
- The choice depends only on the compared attribute and the ids.

## Relations between input and output

How the answer must change when the input changes. The property tests check these on random inputs:

- Permuting the cycle does not change which transaction is chosen.
- Adding a transaction with a strictly worse attribute (older, more locks, more work) never changes the victim.
- A one-element cycle's victim is that element.

## Examples

Worked cases (the tests include them):

```text
cycle [(id 1, start 10), (id 2, start 30), (id 3, start 20)] Youngest -> id 2
```

## What the tests check

- Each policy.
- Tie-breaks.
- A property: permutation invariance.

## Done when

All the `s4d_c3` tests pass.

## Further reading

In the cookbook. Some of these pages show a complete solution, so they are best read after your own attempt.

- [A lock manager](/cookbook/p/lock-manager-deadlock): deadlock detection and victims: youngest or fewest locks.
- [Two-phase locking](/cookbook/p/two-phase-locking): victim heuristics: age, progress, locks held, past restarts.
