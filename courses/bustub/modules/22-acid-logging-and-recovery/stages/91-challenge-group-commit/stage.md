A challenge: no walkthrough, no hints, no solution. Solve it with what this module and the ones before it taught. It is extra practice and does not count towards the course.

## What to build

`GroupCommit` in `src/recovery/group_commit.rs`: transactions that want to commit hand their commit record's LSN to `submit(lsn, now)`; the log is flushed for a whole **batch** at once. `poll(now)` returns the batch to acknowledge when the queue has reached `max_batch` or the **oldest** waiting commit has waited `max_wait`; `flush_all()` returns everything waiting.

## Why

The expensive part of a commit is the `fsync`, and one `fsync` makes every earlier record durable, whoever wrote it. Batching commits turns a thousand syncs a second into a handful, at the price of a bounded delay for each. The rule is simple and the invariants are what matters: nobody is acknowledged before the flush, nobody is acknowledged twice, nobody waits for ever.

## The contract

- `submit(lsn, now)` queues the commit (LSNs arrive in increasing order). Time is a number of ticks passed in.
- `poll(now)` returns `Some(batch)` (all waiting LSNs, in order) when `queue.len() >= max_batch` or `now - oldest_submit >= max_wait`, and `None` otherwise; the batch leaves the queue.
- `flush_all()` returns whatever waits (possibly nothing) and empties the queue. `pending()` is the queue length.

## Invariants

These must hold after every step, whatever the input:

- Every submitted LSN is returned in exactly one batch, in submission order.
- A batch is never returned while neither condition holds.
- `pending()` is submissions minus acknowledgements.

## Relations between input and output

How the answer must change when the input changes. The property tests check these on random inputs:

- No commit waits longer than `max_wait` once `poll` is called at that time.
- A batch never has fewer than 1 element.
- Polling twice at the same time returns a batch at most once (the second poll finds the queue empty or below the threshold).

## Examples

Worked cases (the tests include them):

```text
max_batch 3, max_wait 10: submit 1@0, 2@1: poll@2 -> None; submit 3@2: poll@2 -> [1,2,3]; submit 4@5: poll@16 -> [4]
```

## What the tests check

- Batch-size trigger, time trigger, neither.
- `flush_all`.
- A property: everything acknowledged once and in order.

## Done when

All the `s4c_c2` tests pass.

## Further reading

In the cookbook. Some of these pages show a complete solution, so they are best read after your own attempt.

- [Write-ahead logging schemes](/cookbook/p/logging-schemes-group-commit): group commit with real threads: one fsync for many commits.
- [Logging](/cookbook/p/logging): group commit among the logging schemes.
