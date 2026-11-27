A challenge: no walkthrough, no hints, no solution. Solve it with what this module and the ones before it taught. It is extra practice and does not count towards the course.

## What to build

`exclusive_times` in `src/execution/analyze_times.rs`: parse the text your `EXPLAIN ANALYZE` prints (one line per operator, children indented by two spaces, each ending in `(rows=R, batches=B, loops=L, time=T.TTTms)` or `(never executed)`) and give, for every operator in order, its depth, its inclusive time and its **exclusive** time: its own, without its children's. `slowest` names the operator with the largest exclusive time.

## Why

The time next to an operator is *inclusive*: a Limit "took" as long as the scan under it, so the root always looks the slowest and is never the problem. The question a person asks is which operator spent the time *itself*. The subtraction is trivial and the parsing is where the care goes; doing it on your own output closes the loop: the tool that measures and the tool that reads the measurement.

## The contract

- Lines that are blank or start with `===` are skipped.
- Depth is the indentation in units of two spaces.
- `inclusive_ms` is the number before `ms`; `(never executed)` is 0.0.
- `exclusive_ms` is `inclusive - sum of the children's inclusive`, but never below 0 (clock jitter).
- `slowest` returns the operator text (the line without its depth indent and its statistics) of the largest exclusive time; the first of equals; `None` for no operators.

## Invariants

These must hold after every step, whatever the input:

- Exclusive times are non-negative.
- The exclusive times of a tree whose children never exceed their parents add up to the root's inclusive time.

## Relations between input and output

How the answer must change when the input changes. The property tests check these on random inputs:

- A leaf's exclusive time is its inclusive time.
- Making one operator slower by `d` raises its exclusive time by `d` and its ancestors' inclusive times, not their exclusive ones.

## Examples

Worked cases (the tests include them):

```text
Limit 0.9ms over Sort 0.8ms over Scan 0.5ms -> exclusive 0.1, 0.3, 0.5; slowest is the Scan
```

## What the tests check

- Parsing indentation and statistics.
- Never executed nodes.
- Exclusive time as a difference.
- A property over random trees.

## Done when

All the `s3i_c5` tests pass.

## Further reading

In the cookbook. Some of these pages show a complete solution, so they are best read after your own attempt.

- [Profiling and observability](/cookbook/p/profiling-and-observability): self time versus inclusive time, and per-operator timing like EXPLAIN ANALYZE.
