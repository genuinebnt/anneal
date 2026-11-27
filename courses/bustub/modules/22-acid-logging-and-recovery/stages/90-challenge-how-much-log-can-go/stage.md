A challenge: no walkthrough, no hints, no solution. Solve it with what this module and the ones before it taught. It is extra practice and does not count towards the course.

## What to build

`truncation_lsn` in `src/recovery/truncation.rs`: after a checkpoint, the log before some point can be deleted. The point is the **smallest** of three: the checkpoint's own start (analysis begins there), the oldest `rec_lsn` in the dirty page table (redo must start there, for the page that has been dirty longest), and the first log record of the oldest transaction still active (undo may need to walk back to it).

## Why

An append-only log that is never truncated fills the disk; a log truncated too eagerly makes recovery impossible, and the error shows only after a crash. The rule is one `min` over three numbers, and the exercise is knowing that it is three, and what each protects.

## The contract

- `truncation_lsn(checkpoint_begin, dirty_rec_lsns, active_first_lsns)` returns the minimum of `checkpoint_begin`, of every `dirty_rec_lsns` and of every `active_first_lsns`.
- With empty lists the answer is `checkpoint_begin`.

## Invariants

These must hold after every step, whatever the input:

- The result is at most each of the three inputs.
- The result is one of the inputs.

## Relations between input and output

How the answer must change when the input changes. The property tests check these on random inputs:

- Adding a dirty page or an active transaction can only lower (never raise) the result.
- The result is monotone in `checkpoint_begin`.
- Removing the smallest element of a list raises the result to the next smallest bound.

## Examples

Worked cases (the tests include them):

```text
checkpoint 100, dirty [90, 120], active [95] -> 90
checkpoint 100, dirty [], active [] -> 100
```

## What the tests check

- Each bound being the smallest in turn.
- Empty lists.
- A property against a fold of `min`.

## Done when

All the `s4c_c1` tests pass.

## Further reading

In the cookbook. Some of these pages show a complete solution, so they are best read after your own attempt.

- [Write-ahead logging schemes](/cookbook/p/logging-schemes-group-commit): log truncation below min(redo start, first LSN of any active transaction).
- [ARIES crash recovery](/cookbook/p/aries-recovery): fuzzy checkpoints recording the transaction and dirty page tables.
