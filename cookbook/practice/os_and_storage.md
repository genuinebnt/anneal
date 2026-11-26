# Practice: os_and_storage

**How to use this page.** Run the recipe, then answer "Prove you got it" using real files, processes and (where allowed) `strace`/`dtruss`/`fs_usage`; the OS is the oracle, not the docs.
Use a temp directory guard (as the recipes do) so experiments never leave files behind; crash tests should kill a CHILD process, never your test runner.
Disk numbers are machine-specific and cached: print them, assert behaviour instead. FlowForge persistence stage: attempt your stage first; these tasks use other domains on purpose.

## buffered_io_internals

**Prove you got it**
1. Predict how many underlying `read` calls `MyBufReader` (8 KiB) makes to read 1 MiB byte by byte, then for a single 1 MiB `read` into a big buffer (the bypass rule). Check with the `Counting` wrapper.
2. Predict what a `FlakyReader` returning 1 byte at a time and `Interrupted` errors does to a naive `read_exact` loop versus yours.

**Try next**
- [S] Add `read_until(b'\n')` to `MyBufReader`; assert it equals `std::io::BufRead::read_until` on 1000 seeded random inputs with random chunking.
- [M] Make `MyBufWriter` handle a failed flush properly (error mid-way, bytes partially written); test with `FlakyWriter` that no byte is written twice and none is lost after the retry.
- [A] Implement a `BufReader` with `seek` support that keeps useful buffered data when the seek target falls inside the buffer; assert read-count savings against std on a seek-heavy workload.

**Use it in**: parsers and log writers. Real code: std's [buffered IO source](https://github.com/rust-lang/rust/tree/master/library/std/src/io) and tokio's [io utilities](https://github.com/tokio-rs/tokio/tree/master/tokio/src/io).

**Pitfall**: a `BufWriter` dropped during a panic or error path can lose the tail of output and swallow the flush error.

## civil_dates_time

**Prove you got it**
1. Predict `days_from_civil(1970,1,1)`, the weekday of 2000-02-29, and the result of adding 1 month to 2024-01-31. Check, then explain why months are not a fixed duration.
2. Predict what the RFC 3339 parser does with `2024-02-30T00:00:00Z`, `24:00:00`, and a leap second `23:59:60Z`; decide which should be errors and compare with your implementation.

**Try next**
- [S] Property test: for every day in 1600..=2400, `civil_from_days(days_from_civil(d)) == d` and weekday advances by 1 mod 7.
- [M] Add time zone offsets with a fixed-offset type and assert conversion round trips; add parsing of `+05:30` and rejection of `+24:00`.
- [A] Add a small IANA-tzdata-free DST rule model (for example US rules: second Sunday of March to first Sunday of November) and tests around the gap and the overlap hour; read how [jiff](https://github.com/BurntSushi/jiff) models ambiguous times.

**Use it in**: log parsing and schedulers. Real code: [jiff](https://github.com/BurntSushi/jiff) and [chrono](https://github.com/chronotope/chrono).

**Pitfall**: using wall-clock time for durations; clocks jump (NTP, DST). Use a monotonic `Instant` for elapsed time.

## disk_scheduling_raid

**Prove you got it**
1. Predict the total seek distance ranking of FCFS, SSTF, SCAN, C-SCAN, LOOK on a seeded random workload, and which one starves a request in the SSTF adversarial workload.
2. Predict what a RAID 5 read does when one disk fails (how many disk reads to rebuild one block of N disks) and what the write hole is; produce it by killing a write after the data block but before the parity block.

**Try next**
- [S] Add the average and the maximum waiting time per policy and show SSTF's maximum is the worst.
- [M] Add RAID 6-style second parity or RAID 10; test that any two disk failures are tolerated (or document which are not) across all failure pairs.
- [A] Add an scrubber that repairs silent corruption on a checksummed RAID 5 stripe (find which disk is wrong using the checksum, not the parity alone); inject 1000 seeded bit flips and assert the data is restored in each case.

**Use it in**: storage servers and OS course labs. Real code: Linux `md` and ZFS design documents; compare your scrub logic with ZFS's checksum-in-parent-pointer idea (search for the ZFS design papers).

**Pitfall**: parity can tell you that a stripe is inconsistent, not which disk is wrong; only a checksum tells you that.

## file_disk_io_patterns

**Prove you got it**
1. Predict the sparse file's `st_blocks` after writing one byte at offset 1 GiB versus the apparent length; check with `metadata().len()` and `MetadataExt::blocks()`.
2. Predict the speed ratio of `write` (no fsync) versus `write` + `sync_data` per 4 KiB block; measure. If the ratio is under 10x you may be on a drive that lies about flush or on macOS where plain `fsync` does not flush the drive cache (look up `F_FULLFSYNC`).

**Try next**
- [S] Use `read_at` from 8 threads on one `&File` and assert that each thread reads the right block (use `fill_block`/`check_block` patterns).
- [M] Add `O_DIRECT` (Linux) or `F_NOCACHE` (macOS) and compare cold versus cached read speed; assert contents equal.
- [A] Write a mini benchmark tool (`fio`-style) with sequential, random, read, write, queue-depth options producing a table; compare with real `fio` on one case.

**Use it in**: database storage layers (compose with `buffer_pool_clock` in the database chapter). Real code: [rustix](https://github.com/bytecodealliance/rustix) exposes the same syscalls safely.

**Pitfall**: benchmarks of file reads measure the page cache, not the disk, unless you drop caches or use direct IO.

## file_locking_atomic_replace

**Prove you got it**
1. Predict the file contents after a crash at each of the four steps (before write, after write, after fsync, after rename). Simulate by running the replace in a child process and killing it with a fault-injection env var at each step; assert old-or-new, never mixed.
2. Predict what two processes see with `flock` versus a PID file when one is killed with SIGKILL; check which one self-heals.

**Try next**
- [S] Skip the directory fsync and explain in a comment exactly which crash it would break (you cannot easily test; justify from the recipe).
- [M] Add a `locked_increment` stress test with 8 processes x 1000 increments on a shared counter file; assert final value is 8000; then remove the lock and watch lost updates.
- [A] Build a tiny package-cache directory manager: atomic installs (temp dir, rename), a lock for concurrent installers, cleanup of stale temp dirs after crash, and a test that kills the installer at random points 200 times and checks the cache is always consistent.

**Use it in**: config files and lock files in CLIs. Real code: the [tempfile](https://github.com/Stebalien/tempfile) crate's `persist`, and [nix](https://github.com/nix-rust/nix) wrappers for `flock`.

**Pitfall**: `rename` is atomic only within one filesystem; a temp file in `/tmp` and a target elsewhere may fall back to copy.

## io_uring_model

**Prove you got it**
1. Predict the number of `io_uring_enter`-equivalent submissions for 1000 operations submitted in batches of 1, 32 and 256, then check the `KStats`.
2. Predict what happens when the CQ is full and the "kernel" has more completions; which side must apply backpressure?

**Try next**
- [S] Add a `NOP` op and a timeout op; assert completion order for linked entries on failure (the rest of the chain must be cancelled).
- [M] Add registered (fixed) buffers: assert that the number of buffer registrations stays constant over 10k ops while pointer-checks happen once.
- [A] On Linux only, repeat a file-read benchmark with the real [io-uring](https://github.com/tokio-rs/io-uring) crate against `pread` in a thread pool and write up where each wins; macOS users: do the same with a thread pool versus the model, and report that this part is not runnable on your OS.

**Use it in**: high-throughput storage and network servers. Real code: [tokio-uring](https://github.com/tokio-rs/tokio-uring) and [liburing](https://github.com/axboe/liburing).

**Pitfall**: completion-based APIs make buffer ownership tricky: the kernel owns the buffer until the completion arrives, so you cannot drop or move it on cancel.

## mmap_files

**Prove you got it**
1. Predict whether your process's data is lost if it is killed right after writing through `MmapMut` without `flush`; test with a child process and then reopen the file. Then predict what a power loss would do and why you cannot test that.
2. Predict the SIGBUS scenario: map a file, truncate it from another handle, read past the new end. Provoke it in a child process and observe the exit status signal.

**Try next**
- [S] Assert that `count()` and `get(i)` stay correct after `grow()` and remap (stale slices must not be kept).
- [M] Add a CRC per record and a recovery scan that finds the last valid record after a simulated torn write.
- [A] Build a read-only inverted-index file format (term dictionary plus posting lists) that is memory-mapped and queried without deserialising; compose with `search_vectors/trigram_index` and assert query results equal an in-memory oracle on 1000 seeded queries.

**Use it in**: indexes and databases with read-mostly files. Real code: [memmap2](https://github.com/RazrFalcon/memmap2-rs); ripgrep uses mmap selectively (see [ripgrep](https://github.com/BurntSushi/ripgrep) for when it avoids it).

**Pitfall**: mmap hides IO errors as signals (SIGBUS) and makes latency unpredictable (page faults on your hot path).

## process_pipes_signals

**Prove you got it**
1. Predict the deadlock: write 1 MiB to a child's stdin from the same thread that later reads its stdout when the child echoes. Reproduce with `cat`, then fix with a writer thread and predict the result.
2. Predict `status.code()` versus `status.signal()` for a child killed by SIGKILL and for one that exits 3; check.

**Try next**
- [S] Add a test that `run_with_timeout` kills a `sleep 30` child within 1.2 s and reaps it (no zombie: check with `ps` or `waitpid`).
- [M] Kill the whole process group (child plus grandchildren) on timeout; assert via a grandchild that writes a marker file that it is gone.
- [A] Build a mini task runner: run shell commands with stdout/stderr captured separately, per-command timeout, graceful SIGTERM then SIGKILL escalation, and a sandbox-ish resource limit (`setrlimit`); test each exit path.

**Use it in**: CI agents and supervisors. Real code: [tokio::process](https://github.com/tokio-rs/tokio/tree/master/tokio/src/process) for async spawn and reaping; [nix](https://github.com/nix-rust/nix) for signals.

**Pitfall**: signal handlers may only call async-signal-safe functions (no allocation, no locks); set an atomic flag and return.

## scheduler_policies

**Prove you got it**
1. Predict mean waiting time ordering for FCFS, SJF, SRTF, round-robin (quantum 2 and 10) on the seeded workload, and which has the best Jain fairness index.
2. Predict what happens to a long job under SRTF with a steady stream of short jobs (starvation); show with a workload and the response time metric.

**Try next**
- [S] Add the convoy-effect demonstration (one long job first, many short behind) and quantify mean waiting under FCFS versus RR.
- [M] Add aging to the priority policy and assert that with aging every process finishes within a bounded time (state the bound from the aging rate).
- [A] Implement CFS-like virtual-runtime scheduling with a red-black-tree-equivalent (a `BTreeMap<(vruntime, id), _>`) and nice weights; test that CPU share converges to weight proportions within 2 percent over 100k ticks. Read the Linux fair scheduler source ([kernel/sched/fair.c](https://github.com/torvalds/linux/blob/master/kernel/sched/fair.c)) only after your version passes.

**Use it in**: OS kernels and task runtimes (compose with `async/worker_pool` for tenant fairness). Real code: tokio's [schedulers](https://github.com/tokio-rs/tokio/tree/master/tokio/src/runtime/scheduler).

**Pitfall**: unit-time simulations ignore context switch cost; a tiny quantum looks fair and performs badly.

## syscalls_portable

**Prove you got it**
1. Predict the number of `write` syscalls for printing 1000 lines with `println!` to a pipe versus a terminal (line buffering); count with `CountingWrite` or `strace -c`/`dtruss -c`.
2. Predict what `fd_is_open` reports after closing an fd via `OwnedFd` drop, and what happens if you close the same raw fd twice (a bug in another thread could close a reused fd).

**Try next**
- [S] Add an assertion on `errno` for reading from a closed fd (`EBADF`) and for a non-blocking empty pipe (`EAGAIN`).
- [M] Implement a tiny `strace`-like counter: run a child under `ptrace`-free mode by wrapping `libc` calls in a shim you own, and print per-syscall counts for a workload (document why a real tracer needs ptrace or dtrace).
- [A] Write a minimal `cat`/`wc`/`cp` trio using only raw `libc` calls (no std IO), compare syscall counts with the GNU/BSD tools on a 100 MB file, and tune your buffer size to match.

**Use it in**: debugging slow IO. Real code: the [libc](https://github.com/rust-lang/libc) crate, [rustix](https://github.com/bytecodealliance/rustix), and std's Unix PAL in [`sys/pal/unix`](https://github.com/rust-lang/rust/tree/master/library/std/src/sys/pal/unix).

**Pitfall**: raw fds have no ownership; prefer `OwnedFd`/`BorrowedFd` so double-close becomes a type error.

## timing_wheel

**Prove you got it**
1. Predict the level and slot of a timer with delay 70 ticks at `now=0` (64 slots per level), and what happens at tick 64 (cascade). Print `bucket_for`.
2. Predict the speed ratio of wheel versus `BinaryHeap` for 1M schedule+cancel operations, versus for 1M tick advances with few timers. Measure both; the heap can win when timers are few.

**Try next**
- [S] Assert that cancel of an already-fired timer returns false and never fires twice (differential test against `HeapTimers`, 100k random ops).
- [M] Add a "next deadline" query so a runtime can sleep exactly until the next expiry; test it against a brute-force scan.
- [A] Plug the wheel into your own mini executor (compose with `async/waker_executor`) as the `sleep` implementation; test that 10k timers fire in order with at most 1 tick lateness. Compare with [tokio's time driver](https://github.com/tokio-rs/tokio/tree/master/tokio/src/runtime/time).

**Use it in**: TCP retransmit timers and session expiry in proxies.

**Pitfall**: timer resolution trade-offs: a coarse tick makes timers fire late by up to one tick; a fine tick wastes wakeups.

## virtual_memory_sim

**Prove you got it**
1. Predict hit rates of FIFO, LRU, CLOCK and OPT on the seeded page trace and Belady's anomaly for FIFO with 3 versus 4 frames on the classic sequence `1,2,3,4,1,2,5,1,2,3,4,5`. Verify FIFO gets MORE faults with 4 frames.
2. Predict RSS after `mmap` of 1 GiB anonymous memory untouched, then after touching every 4096th byte; measure with `ps` or `getrusage`.

**Try next**
- [S] Add TLB size sweeps (4, 16, 64 entries) and plot hit rate against working-set size.
- [M] Add a second-chance (CLOCK) with a dirty bit preference; assert it never evicts a page that OPT would keep when that page is referenced next in a tiny trace.
- [A] Implement copy-on-write fork semantics in the simulator: two address spaces sharing frames until a write; assert frame counts and independent contents after writes.

**Use it in**: understanding allocators and guard pages (compose with `memory/bump_arena_allocator`). Real practice: the Linux memory management docs; [memmap2](https://github.com/RazrFalcon/memmap2-rs) or raw `libc::mmap` for experiments.

**Pitfall**: RSS is not virtual size; large `Vec::with_capacity` is not resident until touched (and `vec![0; n]` may use zeroed pages lazily).

## wal_append_log

**Prove you got it**
1. Predict what `recover` returns if the file is truncated at every byte offset of the last record (the first 3 bytes of the header, the end of the header, the middle of the payload). Loop over every truncation point and assert the same N-1 records come back.
2. Predict what happens if you flip one bit in the middle of record 5 of 10: does recovery return 4 records, 9, or 10? Which is correct, and why must you stop at the first bad record?

**Try next**
- [S] Add a record sequence number and detect duplicated or reordered records.
- [M] Add group commit: batch N appends per fsync; benchmark append latency distribution versus fsync-per-record and assert no acknowledged record is lost in a kill test.
- [A] Build a key-value store on the WAL plus an in-memory index, with checkpoints (snapshot plus log truncation) and recovery; kill the process 500 times at random points, and assert the recovered state equals some prefix of the acknowledged operations (compose with `database/aries_recovery` ideas).

**Use it in**: any durable service. Real code: [raft-engine](https://github.com/tikv/raft-engine) and the WAL in [TiKV](https://github.com/tikv/tikv).

**Pitfall**: acknowledging a write to the client before `sync_data` returns turns the WAL into decoration.

## zero_copy_sendfile_splice

**Prove you got it**
1. Predict the syscall count and throughput order for `rw_loop` at 64 KiB, 1 MiB buffers, `io::copy`, and `fs::copy`; check which are equal because std already takes a fast path.
2. Predict whether `sendfile` over loopback is faster than the read/write loop for a 64 MiB file and by how much. If the gap is small, explain why (loopback is memory copy anyway).

**Try next**
- [S] Assert byte equality via checksum for every method for sizes 0, 1, 4095, 4096, 4097 and 64 MiB.
- [M] Add `copy_file_range` (Linux) or `clonefile` (macOS APFS) and show a near-instant copy of a large file plus the fall-back path when the filesystem does not support it.
- [A] Build a static file server on your tcp_framed_server style loop (compose with `networking/tcp_framed_server`) using `sendfile` for large bodies and a cache for small ones; measure with a load generator at 100 connections.

**Use it in**: file servers and backup tools. Real code: the [tokio `copy` utilities](https://github.com/tokio-rs/tokio/tree/master/tokio/src/io) and the [bytes](https://github.com/tokio-rs/bytes) crate for zero-copy buffers.

**Pitfall**: zero-copy often means "shared mutable page": truncating a file while it is being sent produces torn data or errors.

## Chapter projects

### Small (about 2 h): crash-safe config store
Combine `file_locking_atomic_replace`, `wal_append_log` (checksum idea), and `errors/error_enum`.
- M1: a `ConfigStore` that saves JSON atomically with a checksum header.
- M2: a lock file preventing two writers.
- M3: a fault-injection env var that kills at step N.
- Accept: kill at every step over 100 runs: reload always returns old or new complete config; two concurrent processes never interleave.
- Stretch: keep the last 3 versions as backups with rollback.

### Intermediate (a weekend): an LSM-lite key-value store
Combine `wal_append_log`, `mmap_files`, `byte_order_keys` (perf), `buffered_io_internals`, plus `database/lsm_compaction_strategies` and `search_vectors/bloom_filter`.
- M1: memtable (BTreeMap) plus WAL.
- M2: flush to immutable sorted files with big-endian key encoding and an index block; read via mmap.
- M3: bloom filter per file; merge-compaction of two levels.
- M4: crash-recovery test harness.
- Accept: differential test against an in-memory `BTreeMap` oracle over 100k seeded ops (put/get/delete/range) with 50 injected crashes; bloom filter keeps false negatives at exactly zero; compaction never loses a tombstone before it covers older data.
- Stretch: snapshot reads (compose with `database/mvcc_snapshot_isolation`).

### Advanced (a week): a mini OS-concepts lab, then a database storage engine
Combine `virtual_memory_sim`, `scheduler_policies`, `disk_scheduling_raid`, `timing_wheel`, `io_uring_model`, plus `database/buffer_pool_clock` and `concurrency/spsc_ring_buffer`.
- M1: a simulated block device with latency model, request queue and a pluggable disk scheduler.
- M2: a buffer pool with CLOCK replacement and pinned pages, backed by the device; page faults counted like VM.
- M3: a slotted-page heap file and a B+tree on top (compose with `data_structures/bplus_tree`).
- M4: timer wheel for deadlines on IO requests; ring-based submission/completion queues between app and device threads.
- Accept: B+tree matches a `BTreeMap` oracle on 1M seeded ops; buffer hit rate beats LRU on a scan-plus-hot-set workload (clock with scan resistance); an invariant checker verifies no page is evicted while pinned; throughput report across 4 schedulers.
- Stretch: ARIES-style recovery with a crash injector (database/aries_recovery).
