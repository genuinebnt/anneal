**Where this fits.** The end of the module: hammer the pieces together, then run BusTub's own two tests for this part.

> [!CHECK] Every earlier test passed and one stress test here still fails now and then. What kinds of bug pass a test that checks one outcome and fail a test that repeats the same thing from many threads? Name two, and say how you would find out which one you have.
> ||A *race*: the result depends on which thread gets there first (two threads read, then both write, and one update is lost). A *missing wake-up or an ordering bug*: the code is right for the schedule the single-threaded test happened to produce. Run the failing test in a loop until it fails and read the message: what it prints (which thread, which page, what value) says whether data was lost, read in the wrong order, or never delivered. Shrink the case (fewer threads, fewer iterations) and add prints or a log of operations to see the interleaving.||
>
> - What does the failure message tell you about which operation is missing or doubled?
> - Does it fail more with more threads, fewer, or the same?
> - Which of your locks is held across a step that should be atomic?

## The task

Nothing new to design. Check that the whole module holds up.

- **Threads on every side.** Four threads each write and read their own pages through one shared `DiskScheduler`, and every thread must get back the bytes it wrote.
- **A real disk.** The scheduler works over the real `DiskManager` and a file as well as over memory.
- **BusTub's tests.** `tests/disk_scheduler_test.rs` is `disk_scheduler_test.cpp` (a write then a read of one page through the scheduler) and `tests/rwlatch_test.rs` is `rwlatch_test.cpp` (readers and writers on one latch), each ported test for test using only the public API.

If something fails here that passed before, an earlier test missed a case; the message names the thread and the page.

## Your freedom

The same as before. A fix here may be small (hold a lock across both steps instead of each) or structural; either is fine as long as the properties hold. If you used an off-the-shelf piece in an earlier stage, now is a good time to read how it does its job.

## The Rust toolbox

**Making a hang a failure.** A deadlock shows as a test that never ends. Wait for a thread's result on a channel with `recv_timeout`, so a hang fails with a message. The module's tests do this with one small helper, `within`; read it, it is worth copying into your own projects.

**Reading a panic message.** `thread 'main' panicked at ...` then the assertion text. A panic in a *spawned* thread shows when you `join().unwrap()` it; the unwrap re-panics with "called `Result::unwrap()` on an `Err` value: Any { .. }". Run with `RUST_BACKTRACE=1`.

**Running one test many times.** `for i in $(seq 100); do cargo test --test stages_1b -- s1b_07 || break; done` finds races that happen one run in fifty.

**Tools beyond the tests.** `loom` runs a small concurrent program under every thread interleaving; `miri` finds undefined behaviour in `unsafe`. Neither is needed here, but the *testing concurrent code* concept shows both on the same channel.

## If this is new

- [L2 Borrowing](/t/l2-borrowing) and [L1 Ownership & moves](/t/l1-ownership-moves) again: every race the compiler could catch it already did, which leaves the logical ones.
- Everything else is in the earlier stages of this module.
- Y5 Testing & verification: Understand it; Build it: a test double that fails on purpose, timeouts so a hang is a failure, loom.

## Tests

- Four threads, 50 write-then-read rounds each, over one scheduler: every read sees its own write.
- The scheduler over the real file disk.
- BusTub's `disk_scheduler_test` and `rwlatch_test`.

## Hints

### Reproduce first

A stress failure is useless until it repeats. Run it in a loop, note the fraction that fail, and only then change code. If a fix drops the failure rate to zero, run it a thousand times before believing it.

### What is shared?

List every piece of state two threads can touch: the queue, the promise's slot, the disk, the latch's data. For each, name the lock (or the atomic) that protects it, and the invariant that lock protects. A missing or wrong name is the bug.

## Performance

Run the module's tests in release mode (`cargo test --release`) a few times and note how long the stress test takes; compare with the debug build. The difference is why the course's performance experiments say *release* every time.

## Experiment

Optional. Predict first, then run.

1. **Break it on purpose.** Remove a lock or a `notify`, run the stress test in a loop, and see how many runs it takes to fail. This calibrates how much to trust a test that passed once.
2. **Run under `miri`.** `cargo +nightly miri test --test stages_1b s1b_02` (slow). What does it catch that the tests cannot?

## Other designs

None for this stage: it checks the earlier designs. The *Other designs* sections of 1b-01 to 1b-06 list the alternatives to compare with yours.

## In BusTub

`disk_scheduler_test.cpp` writes the string "A test string." to page 0 through the scheduler, reads it back, and compares; `rwlatch_test.cpp` runs a handful of threads over one latch. Both are short; the tests in this module are stricter, which is the point of the module's property tests.

## The C/C++ way

| C / C++ | Rust |
|---|---|
| `TEST(DiskSchedulerTest, ScheduleWriteReadPageTest)` | `#[test] fn schedule_write_read_page_test()` |
| `std::promise` / `future.get()` in the test | the module's own promise/future |
| `char buf[BUSTUB_PAGE_SIZE]` and `std::strncpy` | `[u8; BUSTUB_PAGE_SIZE]` and `copy_from_slice` |
| `ASSERT_EQ(std::memcmp(a, b, n), 0)` | `assert!(a == b, "message")` |

**Port rule:** C++ tests that share a buffer between the test and the worker become tests that hand the worker an owned buffer and get it back.

## Learn more

- [`loom`](https://docs.rs/loom) · [`miri`](https://github.com/rust-lang/miri) · [`mpsc::Receiver::recv_timeout`](https://doc.rust-lang.org/std/sync/mpsc/struct.Receiver.html#method.recv_timeout)
- BusTub's [disk scheduler test](https://github.com/cmu-db/bustub/blob/master/test/storage/disk_scheduler_test.cpp)
