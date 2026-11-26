Your server answers millions of requests. You want to know how long they take: not only the average, but how slow the slowest 1% are, and how many finished within each of a few limits your team cares about. You cannot keep every measurement.

`Timings::new(bounds)` takes the report's limits in strictly increasing order (panic if they are not). All values are microseconds.

- `record(v)` adds one measurement. It takes `&self`, is safe from many threads with nothing lost, and **allocates nothing**. The memory a `Timings` uses does not grow with the number of measurements.
- `count()`, `sum()` (stays at `u64::MAX` instead of wrapping) and `max()` (0 when empty).
- `quantile(q)`, for `0.0 <= q <= 1.0`: `None` when empty. Otherwise let the exact value be the measurement of rank `max(1, ceil(q * count))` in sorted order. The answer is **at least** the exact value, **at most** the exact value plus one eighth of it (rounded down), and never above `max()`. Any `u64` may be recorded.
- `render(name)` is the report, one line each, ending in a newline:

```
name_bucket{le="B"} C        one per bound, in order
name_bucket{le="+Inf"} N
name_sum S
name_count N
```

`C` is the exact number of measurements **less than or equal to** `B`.
