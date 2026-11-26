# Building: A command-line tool



Use this when you have a real project in front of you. For each stage: **attempt your design first**, then read the listed recipes to compare, steal what is better and note what you missed.

### Step 1: Interface

**Goal:** Args and output.

| # | Recipe | Why here |
|---|---|---|
| 1.1 | [`clap_cli`](../../examples/dev_tools/clap_cli.rs) · intermediate | Parsing. |
| 1.2 | [`error_enum`](../../examples/errors/error_enum.rs) · simple | Exit codes. |
| 1.3 | [`anyhow_context`](../../examples/errors/anyhow_context.rs) · simple | Context for users. |
| 1.4 | [`io_files_paths`](../../examples/language/io_files_paths.rs) · intermediate | Files. |

### Step 2: Quality

**Goal:** Reliable.

| # | Recipe | Why here |
|---|---|---|
| 2.1 | [`testing_patterns`](../../examples/dev_tools/testing_patterns.rs) · intermediate | Golden tests. |
| 2.2 | [`tracing_logs`](../../examples/dev_tools/tracing_logs.rs) · intermediate | Logs. |
| 2.3 | [`shrinking`](../../examples/dev_tools/shrinking.rs) · advanced | Property tests. |
| 2.4 | [`library_api_polish`](../../examples/language/library_api_polish.rs) · intermediate | Doc and API. |

### Step 3: Speed

**Goal:** If needed.

| # | Recipe | Why here |
|---|---|---|
| 3.1 | [`bufwriter`](../../examples/perf/bufwriter.rs) · simple | Output. |
| 3.2 | [`rayon_data_parallelism`](../../examples/concurrency/rayon_data_parallelism.rs) · intermediate | Parallel. |
| 3.3 | [`bench_harness`](../../examples/perf/bench_harness.rs) · intermediate | Measure. |
