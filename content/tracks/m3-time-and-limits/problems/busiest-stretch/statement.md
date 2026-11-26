An hourly report says traffic was fine, but the service fell over. The burst began just before one hour and ended just after it, so each hour got only half. You need the busiest stretch **wherever it starts**.

`log` holds `(time_ms, size)` pairs, in non-decreasing order of time. Several events may share a time, and a size may be 0.

A stretch of `span_ms` (at least 1) starting at `s` holds the events with `s <= time < s + span_ms`. So two events exactly `span_ms` apart are never in the same stretch.

## What to write

`busiest(log, span_ms)` looks at the stretches that start **at the time of an event**. It returns the one with the largest total size, with the number of events in it. Among equal totals it returns the **earliest** start. An empty log gives `None`.

`first_over(log, span_ms, budget)` returns the index of the first event at which the budget is broken. That is the first `i` where the events among `log[..=i]` that are **less than `span_ms` older** than `log[i]` add up to **more than** `budget`. It returns `None` if that never happens.

Totals can exceed `u64::MAX`. Logs have up to 200,000 events.
