# Track: Operating-system and storage internals

About 4 weeks.

**How to work each step:** read the header, predict the asserts, blank the file and rebuild it from memory (`./study.sh NAME`), then mutate it. Details in [HOW_TO_STUDY.md](../../HOW_TO_STUDY.md). Checkpoint = you can explain the step's goal in five lines without looking.

### Step 1: Files and I/O

**Goal:** What read() really does.

| # | Recipe | Why here |
|---|---|---|
| 1.1 | [`file_disk_io_patterns`](../../examples/os_and_storage/file_disk_io_patterns.rs) · advanced | Sequential vs random. |
| 1.2 | [`buffered_io_internals`](../../examples/os_and_storage/buffered_io_internals.rs) · advanced | Buffering. |
| 1.3 | [`mmap_files`](../../examples/os_and_storage/mmap_files.rs) · intermediate | Memory mapping. |
| 1.4 | [`file_locking_atomic_replace`](../../examples/os_and_storage/file_locking_atomic_replace.rs) · intermediate | Crash-safe updates. |
| 1.5 | [`io_uring_model`](../../examples/os_and_storage/io_uring_model.rs) · advanced | Async I/O. |
| 1.6 | [`zero_copy_sendfile_splice`](../../examples/os_and_storage/zero_copy_sendfile_splice.rs) · intermediate | Avoid copies. |

### Step 2: Memory and scheduling

**Goal:** Virtual memory and CPUs.

| # | Recipe | Why here |
|---|---|---|
| 2.1 | [`virtual_memory_sim`](../../examples/os_and_storage/virtual_memory_sim.rs) · advanced | Paging. |
| 2.2 | [`scheduler_policies`](../../examples/os_and_storage/scheduler_policies.rs) · advanced | Scheduling. |
| 2.3 | [`process_pipes_signals`](../../examples/os_and_storage/process_pipes_signals.rs) · intermediate | Processes. |
| 2.4 | [`timing_wheel`](../../examples/os_and_storage/timing_wheel.rs) · advanced | Timers. |
| 2.5 | [`disk_scheduling_raid`](../../examples/os_and_storage/disk_scheduling_raid.rs) · intermediate | Disks. |

### Step 3: Build one

**Goal:** Capstones.

| # | Recipe | Why here |
|---|---|---|
| 3.1 | [`toy_filesystem`](../../examples/capstones/toy_filesystem.rs) · capstone | A filesystem. |
| 3.2 | [`mini_kv_store`](../../examples/capstones/mini_kv_store.rs) · capstone | A storage engine. |
| 3.3 | [`mini_redis`](../../examples/capstones/mini_redis.rs) · capstone | A server and persistence. |
| 3.4 | [`mini_http_server_thread_pool`](../../examples/capstones/mini_http_server_thread_pool.rs) · capstone | A server. |
