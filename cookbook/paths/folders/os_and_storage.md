# Chapter `os_and_storage`: suggested reading order

Ordered simple -> intermediate -> advanced (then alphabetical). Skim `Use in:` and `COMMON USES` in each header first. Cross-chapter order is in [the tracks](../tracks/).

| # | Recipe | Level | What it is |
|---|---|---|---|
| 1 | [`civil_dates_time`](../../examples/os_and_storage/civil_dates_time.rs) | intermediate | civil dates and time from scratch: days<->civil, RFC 3339, month arithmetic, clocks |
| 2 | [`disk_scheduling_raid`](../../examples/os_and_storage/disk_scheduling_raid.rs) | intermediate | disk-head scheduling (FCFS/SSTF/SCAN/C-SCAN/LOOK/C-LOOK), RAID 0/1/5, and checksum scrubbing. |
| 3 | [`file_locking_atomic_replace`](../../examples/os_and_storage/file_locking_atomic_replace.rs) | intermediate | crash-safe file replacement (temp + fsync + rename + dir fsync) and file locking (flock, PID lock, O_EXCL) |
| 4 | [`mmap_files`](../../examples/os_and_storage/mmap_files.rs) | intermediate | memory-mapped files with `memmap2`: zero-copy reads, durable writes, growth, and an mmap-backed record store |
| 5 | [`process_pipes_signals`](../../examples/os_and_storage/process_pipes_signals.rs) | intermediate | child processes, pipes, exit status vs signals, timeouts, and async-signal-safe signal handlers |
| 6 | [`syscalls_portable`](../../examples/os_and_storage/syscalls_portable.rs) | intermediate | syscalls: what they are, how Rust reaches the kernel, fd ownership, cost, and counting them. |
| 7 | [`zero_copy_sendfile_splice`](../../examples/os_and_storage/zero_copy_sendfile_splice.rs) | intermediate | zero-copy IO: read/write loop vs sendfile, io::copy, clonefile/copy_file_range, mmap, splice, bytes::Bytes. |
| 8 | [`buffered_io_internals`](../../examples/os_and_storage/buffered_io_internals.rs) | advanced | buffered IO internals: build BufReader, BufWriter, LineWriter and a prefetching reader from scratch. |
| 9 | [`file_disk_io_patterns`](../../examples/os_and_storage/file_disk_io_patterns.rs) | advanced | file and disk IO patterns: pread/pwrite, vectored IO, page cache, durability, sparse files, links. |
| 10 | [`io_uring_model`](../../examples/os_and_storage/io_uring_model.rs) | advanced | the io_uring DESIGN modelled in portable Rust: shared SQ/CQ rings, batching, linked SQEs, fixed buffers, async integration. |
| 11 | [`scheduler_policies`](../../examples/os_and_storage/scheduler_policies.rs) | advanced | CPU scheduling policies: FCFS, SJF/SRTF, round-robin, priority+aging, MLFQ, DRR, stride |
| 12 | [`timing_wheel`](../../examples/os_and_storage/timing_wheel.rs) | advanced | hierarchical timing wheel with O(1) insert/cancel, cascading and an overflow list |
| 13 | [`virtual_memory_sim`](../../examples/os_and_storage/virtual_memory_sim.rs) | advanced | virtual memory: a 4-level page-table + TLB + demand-paging simulator, and the real thing via mmap/mprotect. |
| 14 | [`wal_append_log`](../../examples/os_and_storage/wal_append_log.rs) | advanced | a write-ahead log: length-prefixed + checksummed records, torn-write detection, and crash recovery. |
