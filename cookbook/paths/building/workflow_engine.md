# Building: A workflow / job orchestration engine (FlowForge)

Read at the moment indicated; attempt your own version first.

Use this when you have a real project in front of you. For each stage: **attempt your design first**, then read the listed recipes to compare, steal what is better and note what you missed.

### Step 1: Stage 1-2: parse, count, bad input

**Goal:** Load JSON, report errors without panics.

| # | Recipe | Why here |
|---|---|---|
| 1.1 | [`serde_defaults`](../../examples/encoding_compression/serde_defaults.rs) · simple | Optional fields and defaults. |
| 1.2 | [`serde_custom`](../../examples/encoding_compression/serde_custom.rs) · advanced | Custom (de)serialisation. |
| 1.3 | [`error_enum`](../../examples/errors/error_enum.rs) · simple | Typed errors. |
| 1.4 | [`clap_cli`](../../examples/dev_tools/clap_cli.rs) · intermediate | A CLI with exit codes. |
| 1.5 | [`diagnostics_spans`](../../examples/errors/diagnostics_spans.rs) · intermediate | Pointing at the bad token. |

### Step 2: Stage 3-4: schema, ids, dependencies

**Goal:** Validate before running anything.

| # | Recipe | Why here |
|---|---|---|
| 2.1 | [`newtype_ids`](../../examples/language/newtype_ids.rs) · simple | TaskId newtype. |
| 2.2 | [`collections_tour`](../../examples/language/collections_tour.rs) · simple | HashMap vs BTreeMap (determinism). |
| 2.3 | [`arena_ids`](../../examples/memory/arena_ids.rs) · simple | Index instead of String lookups. |
| 2.4 | [`option_result_toolbox`](../../examples/errors/option_result_toolbox.rs) · simple | Chained validation. |

### Step 3: Stage 5-8: graph order and scale

**Goal:** Attempt the algorithm first; then compare.

| # | Recipe | Why here |
|---|---|---|
| 3.1 | [`index_graph`](../../examples/algorithms/index_graph.rs) · intermediate | Vec-index graph. |
| 3.2 | [`csr_graph`](../../examples/algorithms/csr_graph.rs) · intermediate | Dense and large graphs. |
| 3.3 | [`heap_patterns`](../../examples/data_structures/heap_patterns.rs) · intermediate | Deterministic tie-breaking. |
| 3.4 | [`shrinking`](../../examples/dev_tools/shrinking.rs) · advanced | Shrink failing random DAGs. |
| 3.5 | [`cache_locality`](../../examples/perf/cache_locality.rs) · intermediate | Why layout matters at 1M nodes. |
| 3.6 | [`bench_harness`](../../examples/perf/bench_harness.rs) · intermediate | Measure. |

### Step 4: Executors

**Goal:** Sequential, then concurrent with limit N.

| # | Recipe | Why here |
|---|---|---|
| 4.1 | [`threads_channels`](../../examples/concurrency/threads_channels.rs) · intermediate | Basics. |
| 4.2 | [`joinset_semaphore`](../../examples/concurrency/joinset_semaphore.rs) · intermediate | Limit N. |
| 4.3 | [`thread_pool_work_stealing`](../../examples/concurrency/thread_pool_work_stealing.rs) · advanced | Pool. |
| 4.4 | [`structured_concurrency_cancellation`](../../examples/concurrency/structured_concurrency_cancellation.rs) · advanced | Cancel on failure. |
| 4.5 | [`retry_backoff_jitter`](../../examples/distributed/retry_backoff_jitter.rs) · simple | Task retries. |
| 4.6 | [`graceful_shutdown`](../../examples/async/graceful_shutdown.rs) · intermediate | Drain. |

### Step 5: Persistence and API

**Goal:** Survive restarts; expose HTTP.

| # | Recipe | Why here |
|---|---|---|
| 5.1 | [`wal_append_log`](../../examples/os_and_storage/wal_append_log.rs) · advanced | Durable log. |
| 5.2 | [`event_sourced_ledger`](../../examples/capstones/event_sourced_ledger.rs) · capstone | Event sourcing. |
| 5.3 | [`lease_fencing_sim`](../../examples/capstones/lease_fencing_sim.rs) · capstone | Prevent double-run. |
| 5.4 | [`saga_orchestrator`](../../examples/distributed/saga_orchestrator.rs) · advanced | Compensation. |
| 5.5 | [`axum_router_state`](../../examples/web/axum_router_state.rs) · intermediate | REST. |
| 5.6 | [`axum_idempotency`](../../examples/web/axum_idempotency.rs) · intermediate | Idempotent submit. |
| 5.7 | [`job_service_axum`](../../examples/capstones/job_service_axum.rs) · capstone | Capstone. |
| 5.8 | [`tracing_logs`](../../examples/dev_tools/tracing_logs.rs) · intermediate | Observability. |
