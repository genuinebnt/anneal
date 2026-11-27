A challenge: no walkthrough, no hints, no solution. Solve it with what this module and the ones before it taught. It is extra practice and does not count towards the course.

## What to build

`decide` in `src/concurrency/deadlock_prevention.rs`: when a transaction `requester` asks for a lock held by `holder`, a prevention policy decides at once what happens, using the two start timestamps (smaller = older). **Wait-die**: an older requester **waits**, a younger one **dies** (aborts itself). **Wound-wait**: an older requester **wounds** the holder (the holder is aborted), a younger one **waits**.

## Why

Detection finds deadlocks after they happen; prevention makes them impossible by never letting a younger transaction be waited for by an older one (wait-die) or an older one wait for a younger one (wound-wait). No graph, no detector thread; the cost is some aborts that detection would not have needed. Both rules fit in one match, and the property that they never form a cycle is the point.

## The contract

- `decide(policy, requester_ts, holder_ts) -> Decision`, `Decision` is `Wait`, `AbortRequester` or `AbortHolder`.
- Timestamps are distinct; smaller means older.
- `WaitDie`: older requester waits, younger aborts itself. `WoundWait`: older requester aborts the holder, younger waits.

## Invariants

These must hold after every step, whatever the input:

- Under each policy, a transaction only ever waits for transactions of one age relation (younger for wait-die, older for wound-wait).
- The decision depends only on the order of the two timestamps.

## Relations between input and output

How the answer must change when the input changes. The property tests check these on random inputs:

- A waits-for graph built only from `Wait` decisions never has a cycle.
- Swapping the roles of the two policies mirrors the decisions.
- The oldest transaction is never aborted by the wait-die rule and always wins under wound-wait.

## Examples

Worked cases (the tests include them):

```text
wait-die: requester 5, holder 9 (older asks) -> Wait; requester 9, holder 5 -> AbortRequester
wound-wait: requester 5, holder 9 -> AbortHolder; requester 9, holder 5 -> Wait
```

## What the tests check

- Each policy and each age order.
- A simulation: no cycle in the waits-for graph.
- A property over random timestamps.

## Done when

All the `s4d_c1` tests pass.

## Further reading

In the cookbook. Some of these pages show a complete solution, so they are best read after your own attempt.

- [A lock manager](/cookbook/p/lock-manager-deadlock): wait-die and wound-wait prevention in a lock manager.
- [Two-phase locking](/cookbook/p/two-phase-locking): wait-die and wound-wait, and why waits only go one way.
