A challenge: no walkthrough, no hints, no solution. Solve it with what this module and the ones before it taught. It is extra practice and does not count towards the course.

## What to build

`make_undo` and `apply_undo` in `src/concurrency/undo.rs`: a tuple is `Vec<i64>` or absent (`None`). `make_undo(old, new)` records how to get back from `new` to `old`: for an update, only the columns that **differ** (a mask and their old values); for a delete or an insert, a flag. `apply_undo(tuple, log)` applies one log to the newer version and returns the older one.

## Why

Storing the whole old row for every update wastes space when one column of fifty changed, which is why systems store *deltas*. Reading an old version then means walking back from the newest, applying each delta. Getting the three cases (insert, update, delete) and the absent tuple right is the whole exercise, and the property that ties them together is simple: undoing the change gives back the old version.

## The contract

- `make_undo(old: Option<&[i64]>, new: Option<&[i64]>) -> UndoLog`: `old == None` (the change was an insert) records `Remove`; `new == None` (a delete) records `Restore(whole old row)`; both present records `Columns { mask, values }` with exactly the differing columns, in column order.
- `apply_undo(tuple: Option<&[i64]>, log) -> Option<Vec<i64>>`: `Remove` gives `None`; `Restore(row)` gives that row; `Columns` overwrites the masked columns of the tuple (which must exist).
- Tuples have a fixed number of columns.

## Invariants

These must hold after every step, whatever the input:

- `apply_undo(new, make_undo(old, new)) == old` for every pair.
- A `Columns` log's mask has no bit set for an equal column, and a log is empty (no bits) only for an unchanged row.

## Relations between input and output

How the answer must change when the input changes. The property tests check these on random inputs:

- Undo logs of a chain of updates, applied newest first, reproduce every older version.
- The size of an update log is proportional to the number of changed columns.
- Making a log from equal rows and applying it changes nothing.

## Examples

Worked cases (the tests include them):

```text
old [1,2,3], new [1,9,3] -> columns {1: 2}; apply to [1,9,3] -> [1,2,3]
old None, new [4] -> Remove; old [4], new None -> Restore([4])
```

## What the tests check

- The three kinds of change.
- A chain of updates undone one by one.
- A property for any pair and any chain.

## Done when

All the `s4a_c4` tests pass.

## Further reading

In the cookbook. Some of these pages show a complete solution, so they are best read after your own attempt.

- [Project 4 · MVOCC](/cookbook/p/project-4-mvocc): delta undo logs holding only modified columns, applied newest first.
- [MVCC implementation choices](/cookbook/p/mvcc-version-storage-gc): delta version storage compared with append-only and time-travel.
