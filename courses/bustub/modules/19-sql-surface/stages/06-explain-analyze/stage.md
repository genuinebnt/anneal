`EXPLAIN` tells you what the optimizer *decided*. Whether it was right is a different question, and only running the query can answer it: the plan says `SeqScan` and a filter; the measured plan says the scan produced a million rows and the filter kept three. This stage turns the engine's executors into something you can see inside. Nothing about any single executor changes: you wrap each one in a **profiling executor** that forwards every call and keeps score, and you print the plan tree with the scores next to each node. The skill it builds is the one a database engineer uses all day: read a measured plan and say where the time and the rows went.

> [!CHECK] `select * from a, b where a.x < b.y` runs as a nested loop join, with `a` (4 rows) on the left and `b` (5 rows) on the right. After `EXPLAIN ANALYZE`, what do you expect `rows` and `loops` to be for the scan of `a` and for the scan of `b`? What does that tell you about how the join works?
> ||`a`: rows 4, loops 1: it is started once and read once. `b`: loops 5 (one `init` per left row, plus possibly one more for the final probe that finds the left side exhausted), rows 20: the right side is restarted for every left tuple and read in full each time. It says the join is quadratic in its inputs and that the inner side's cost is paid `loops` times: the number to look at first when a join is slow.||
>
> - What does a node's `time` include, and how do you get its own time?
> - Why does a scan under a `Limit 5` report far fewer rows than the table has?
> - What should `EXPLAIN ANALYZE delete from t` do?

## The task

- `explain analyze <select>` (and `explain (analyze) <select>`) executes the query and prints, after `=== ANALYZE ===`, one line per plan node: the node's description (as plain `EXPLAIN` prints it) followed by `(rows=R, batches=B, loops=L, time=T.TTTms)`, root first, children indented by two spaces under their parent. The query's own result is not printed.
- `rows` counts tuples produced over the node's life, `batches` the calls of `next` that produced something, `loops` the calls of `init`, `time` the milliseconds spent in the node's `init` and `next` (children included).
- A node that was never executed prints `(never executed)`.
- `EXPLAIN ANALYZE` of a statement that changes data is a `NotImplemented` error and does not run it.

Where to work: `ProfilingExecutor` in `src/execution/executors/profiling_executor.rs`, `render_analysis` in `src/execution/analyze_render.rs`, `create_executor` in `src/execution/executor_factory.rs` (wrap each executor when the context has analysis statistics: the counters themselves, `AnalyzeStats`, are given) and `handle_explain_statement` in `src/common/bustub_instance.rs`.

## Your freedom

How timing is taken (one clock read around each call is the simplest), whether the wrapper holds its counters directly or through the shared `NodeStats` (the given type), and the exact format as long as the four numbers are in the line as described: the tests parse them.

## The Rust toolbox

**A decorator is a struct holding a `Box<dyn Executor>`.** It implements the same trait, so a parent cannot tell it from the real thing.

**`Instant::now()` and `.elapsed()`.** The standard monotonic clock; `as_nanos() as u64` adds into an `AtomicU64`.

**Atomics with `Relaxed` ordering.** The counters are read after the query has finished, from the same thread; nothing needs more than `Ordering::Relaxed`.

## If this is new

- [L4 Traits and dispatch](/t/l4-traits-dispatch): a trait object wrapping another trait object.
- S7 Smart pointers: `Arc` for counters shared between the wrapper and the report.

## Tests

- Every operator reports the rows it produced.
- A limit stops pulling from its child; batches count productive calls.
- The inner side of a nested loop join is started once per outer row.
- Times are inclusive and never negative; the analysis changes neither the result nor the data; data-changing statements are refused.

## Hints

### Count what the parent sees

Count rows and batches from the batch *returned to the caller*, after the inner executor filled it, and only when the call returned `true`. A call that returns `false` produced nothing.

### Wrap at one place

Every executor is created in one function that recurses into its children. Wrap there, once, instead of in each executor's constructor: then a new executor is measured without anyone remembering to.

### The key is the node, not its text

Two scans of the same table are two nodes with the same description. Key the statistics by the plan node's identity.

## Performance

Measuring is not free: two clock reads per call, and the calls are per batch of 20 tuples. For a trivial scan the instrumentation can cost more than the scan; PostgreSQL documents this ("the overhead of repeatedly reading the system clock can slow down the query significantly"). The numbers are right in proportion and a little high in absolute terms.

**Measure it.** The same query with and without `explain analyze` over a million rows. What is the overhead per batch, and how does it change if you time only the root?

## Experiment

Optional. Predict first, then run.

1. **Wrap only the root.** What do you lose? (Per-operator rows, and the inner side's `loops`.)
2. **Count `loops` in `next` instead of `init`.** Which test fails, and what number would you read off a nested loop join?

## Other designs

- **Counters inside every executor:** no wrapper and no indirection, and every executor has to remember to count, and the code is full of bookkeeping.
- **Sampling:** a profiler interrupts at intervals and attributes time to the operator that was running; no per-call overhead, no exact row counts.
- **Per-batch timestamps only at the edges** (start of the first call, end of the last): cheaper, and no per-batch detail; the approach some engines take under heavy load.

## In BusTub

BusTub's `EXPLAIN` prints the binder, planner and optimizer trees only: there is no run-time statistics. Its test harness has the closest thing: the `+ensure:nlj_init_check` option wraps the two sides of a nested loop join in counters (module 3f) to check that the right side is initialised once per left tuple. That check is this stage's `loops`, applied to a single test.

## The C/C++ way

| C / C++ | Rust |
|---|---|
| a subclass `ProfilingExecutor : AbstractExecutor` | a struct implementing `Executor` and holding `Box<dyn Executor>` |
| `std::chrono::steady_clock` | `std::time::Instant` |
| `std::atomic<uint64_t>` in a `shared_ptr<Stats>` | `AtomicU64` in `Arc<NodeStats>` |

**Port rule:** observation must not change behaviour. A wrapper that alters a single return value is a bug in the instrument.

## Learn more

- [PostgreSQL: using EXPLAIN](https://www.postgresql.org/docs/current/using-explain.html) · [`std::time::Instant`](https://doc.rust-lang.org/std/time/struct.Instant.html)
