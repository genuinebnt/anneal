# Building: A database / storage engine

Build in this order; each layer needs the one before.

Use this when you have a real project in front of you. For each stage: **attempt your design first**, then read the listed recipes to compare, steal what is better and note what you missed.

### Step 1: Durable bytes

**Goal:** Pages and a log.

| # | Recipe | Why here |
|---|---|---|
| 1.1 | [`file_disk_io_patterns`](../../examples/os_and_storage/file_disk_io_patterns.rs) · advanced | I/O costs. |
| 1.2 | [`slotted_pages_heap_file`](../../examples/database/slotted_pages_heap_file.rs) · advanced | Pages. |
| 1.3 | [`wal_append_log`](../../examples/os_and_storage/wal_append_log.rs) · advanced | WAL. |
| 1.4 | [`file_locking_atomic_replace`](../../examples/os_and_storage/file_locking_atomic_replace.rs) · intermediate | Atomic file updates. |
| 1.5 | [`buffer_pool_clock`](../../examples/database/buffer_pool_clock.rs) · advanced | Cache pages. |

### Step 2: An index

**Goal:** Find things.

| # | Recipe | Why here |
|---|---|---|
| 2.1 | [`bplus_tree`](../../examples/data_structures/bplus_tree.rs) · advanced | Ordered index. |
| 2.2 | [`lsm_compaction_strategies`](../../examples/database/lsm_compaction_strategies.rs) · advanced | Or an LSM. |
| 2.3 | [`bloom_filter`](../../examples/search_vectors/bloom_filter.rs) · intermediate | Skip SSTables. |
| 2.4 | [`mini_kv_store`](../../examples/capstones/mini_kv_store.rs) · capstone | Capstone. |

### Step 3: Transactions

**Goal:** Concurrency and crashes.

| # | Recipe | Why here |
|---|---|---|
| 3.1 | [`lock_manager_deadlock`](../../examples/database/lock_manager_deadlock.rs) · advanced | Locks. |
| 3.2 | [`mvcc_snapshot_isolation`](../../examples/database/mvcc_snapshot_isolation.rs) · advanced | Snapshots. |
| 3.3 | [`aries_recovery`](../../examples/database/aries_recovery.rs) · advanced | Recovery. |
| 3.4 | [`linearizability_checker`](../../examples/distributed/linearizability_checker.rs) · advanced | Test your claims. |

### Step 4: A query layer

**Goal:** SQL on top.

| # | Recipe | Why here |
|---|---|---|
| 4.1 | [`expression_parsing_three_ways`](../../examples/text_and_parsing/expression_parsing_three_ways.rs) · advanced | Parse. |
| 4.2 | [`volcano_vectorized_push`](../../examples/database/volcano_vectorized_push.rs) · advanced | Execute. |
| 4.3 | [`join_algorithms`](../../examples/database/join_algorithms.rs) · advanced | Join. |
| 4.4 | [`query_optimizer`](../../examples/capstones/query_optimizer.rs) · capstone | Optimise. |
| 4.5 | [`tiny_sql_engine`](../../examples/capstones/tiny_sql_engine.rs) · capstone | End to end. |
