# OS, Files and Storage

Processes, signals, mmap, virtual memory, syscalls, buffered IO, file locking, disks and RAID, write-ahead logs.

Run any recipe from the `cookbook/` directory: `cargo run --release --example <name>` (the name is the file name without `.rs`). Each file starts with a header: what it shows, `LEVEL`, `COMMON USES` and `Read:` links.

14 examples.

| Example | What it shows | Level | Common uses | Crates |
|---|---|---|---|---|
| [`wal_append_log`](wal_append_log.rs) | a write-ahead log: length-prefixed + checksummed records, torn-write detection, and crash recovery | advanced | databases (WAL), message brokers, event sourcing, filesystems: durable append with crash recovery. | std only |
| [`process_pipes_signals`](process_pipes_signals.rs) | child processes, pipes, exit status vs signals, timeouts, and async-signal-safe signal handlers | intermediate | build systems and CI runners spawning and timing out jobs; process supervisors restarting | libc |
| [`mmap_files`](mmap_files.rs) | memory-mapped files with `memmap2`: zero-copy reads, durable writes, growth, and an mmap-backed record store | intermediate | LMDB-style embedded key-value stores, search-engine index segments, columnar data files, | libc, memmap2 |
| [`file_locking_atomic_replace`](file_locking_atomic_replace.rs) | crash-safe file replacement (temp + fsync + rename + dir fsync) and file locking (flock, PID lock, O_EXCL) | intermediate | editors and package managers saving files atomically (`git` and `apt` use lock files + rename), SQLite and | libc |
| [`timing_wheel`](timing_wheel.rs) | hierarchical timing wheel with O(1) insert/cancel, cascading and an overflow list | advanced | the Linux kernel's timer wheel for timeouts (cascading wheels, tuned for timers that are usually | std only |
| [`scheduler_policies`](scheduler_policies.rs) | CPU scheduling policies: FCFS, SJF/SRTF, round-robin, priority+aging, MLFQ, DRR, stride | advanced | SRTF/SJF reasoning appears in shortest-job-first request scheduling and tail-latency work; round | std only |
| [`civil_dates_time`](civil_dates_time.rs) | civil dates and time from scratch: days<->civil, RFC 3339, month arithmetic, clocks | intermediate | log/trace timestamps, API date fields, token expiry (`exp` claims are Unix seconds), billing periods | std only |
| [`io_uring_model`](io_uring_model.rs) | the io_uring DESIGN modelled in portable Rust: shared SQ/CQ rings, batching, linked SQEs, fixed buffers, async integration | advanced | tokio-uring, glommio, monoio, high-throughput storage engines, and servers on Linux 5.10+ that want fewer syscalls | libc |
| [`virtual_memory_sim`](virtual_memory_sim.rs) | virtual memory: a 4-level page-table + TLB + demand-paging simulator, and the real thing via mmap/mprotect | advanced | allocators (jemalloc/glibc malloc use mmap + madvise to return memory), language runtimes with guard | libc |
| [`syscalls_portable`](syscalls_portable.rs) | syscalls: what they are, how Rust reaches the kernel, fd ownership, cost, and counting them | intermediate | strace/dtruss-driven debugging, sandboxes and seccomp filters (a syscall allow-list), container | libc, memmap2 |
| [`file_disk_io_patterns`](file_disk_io_patterns.rs) | file and disk IO patterns: pread/pwrite, vectored IO, page cache, durability, sparse files, links | advanced | write-ahead logs (append + fdatasync), SSTable/B-tree page reads with pread, atomic config updates, | libc |
| [`buffered_io_internals`](buffered_io_internals.rs) | buffered IO internals: build BufReader, BufWriter, LineWriter and a prefetching reader from scratch | advanced | log/CSV/JSON-lines writers, line-oriented protocol parsers (Redis/SMTP-style), file copy tools, | bytes |
| [`disk_scheduling_raid`](disk_scheduling_raid.rs) | disk-head scheduling (FCFS/SSTF/SCAN/C-SCAN/LOOK/C-LOOK), RAID 0/1/5, and checksum scrubbing | intermediate | OS schedulers (Linux has mq-deadline/BFQ/none; NVMe usually uses none), storage arrays, home NAS, | std only |
| [`zero_copy_sendfile_splice`](zero_copy_sendfile_splice.rs) | zero-copy IO: read/write loop vs sendfile, io::copy, clonefile/copy_file_range, mmap, splice, bytes::Bytes | intermediate | nginx/Apache static files (`sendfile on;`), Kafka brokers serving consumer fetches straight from the page cache, | bytes, libc, memmap2 |
