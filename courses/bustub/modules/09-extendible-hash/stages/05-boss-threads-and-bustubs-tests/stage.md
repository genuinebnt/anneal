**Where this fits.** The end of the hash table: four threads inserting at once, mixed readers and writers, a long run against the model, and BusTub's serial and concurrent tests.

> [!CHECK] Reader threads look up keys that were inserted before the writers started, while writers insert and remove other keys and cause splits, merges and directory changes. What must be true of the latching for a reader never to miss a stable key, and what happens to a reader that holds the header latch for too long?
> ||A reader holds a latch on the page it is looking at while it acquires the next one down, and only then lets go: header, then directory, then bucket, each acquired before the parent is released. A split or merge changes the directory and buckets under write latches, so a reader either sees the table before the change or after it, never in between. A reader that sits on the header latch blocks every writer that needs a new directory, so hold each parent only until the child is latched.||
>
> - Why must the child be latched before the parent is released?
> - What does a writer hold while it splits?
> - Which operation could deadlock with a reader?

## The task

Nothing new to design.

- **Disjoint keys.** Four threads insert 300 keys each into one table (header depth 2, directory depth 9, buckets of 8); every key is found, the integrity check passes, no page stays pinned.
- **Readers and writers.** Three threads insert and remove their own key ranges against private models while two readers repeatedly look up 200 keys inserted beforehand: the stable keys must always be found.
- **A long run.** 20 000 random single-threaded operations agree with a `HashMap` and the hash-class rule.
- **BusTub's tests.** `extendible_htable_test.rs` (`insert_test_2` and `remove_test_1`, ported) and `extendible_htable_concurrent_test.rs` (insert, delete and mixed tests over several threads).

## Your freedom

The same as before; a fix here may be as small as holding a latch a moment longer or as large as changing the order you latch pages.

## The Rust toolbox

**Scoped threads.** `thread::scope(|s| { s.spawn(|| ...); })` lets threads borrow the table (`&ht`) without `Arc`; the scope waits for all of them, and a panic in one thread panics the scope.

**Finding a race.** Run the test in a loop (`for i in $(seq 50); do cargo test --test stages_2b s2b_05 || break; done`); read the message for the thread and key.

**A hang is a failure.** If a run never finishes, a thread is waiting for a latch another thread holds: print the order in which your code takes latches in `insert` and `remove`.

## If this is new

- Everything is in the earlier stages of this module and of 1g.
- [C1 Threads & shared state](/t/c1-threads-shared-state): Understand it: latch crabbing: take the child, then let go of the parent; lock ordering.
- Y5 Testing & verification: Understand it: an invariant checker run after every operation; scoped threads for a stress test.

## Tests

- Four threads on disjoint keys lose nothing.
- Inserts, removes and lookups at the same time; the stable keys are always found.
- A long single-threaded run agrees with a `HashMap`.
- BusTub's serial and concurrent hash table tests.

## Hints

### A stable key is missing

A reader saw the table in the middle of a split. Check that your insert holds the directory's write latch for the whole split and that the reader takes the directory latch before letting go of the header.

### Everything stops

Compare the latch order of `insert` and `remove`: header, directory, bucket, in that order in both, and the second bucket of a merge only after the first is released.

## Performance

Run `cargo test --release` and count operations per second with 1, 2 and 4 threads. The header latch is taken by every operation, in write mode by inserts that create a directory: write latching the header only when a directory must be created (read otherwise, upgrading on need) is the first optimisation.

## Experiment

Optional. Predict first, then run.

1. **Read latch the header in `insert`.** Take only the read latch of the header when the directory exists. What breaks if two threads then race to create a directory?
2. **Scaling.** Measure inserts per second at 1, 2, 4, 8 threads on the same table and on one table per thread. The difference is your contention.

## Other designs

None for this stage. The *Other designs* sections of 2b-02 to 2b-04 list the alternatives to compare with yours.

## In BusTub

The concurrent test spawns threads inserting disjoint ranges and checks all values; the serial tests cover a small table with a tiny bucket size and the `VerifyIntegrity` call after each step.

## The C/C++ way

| C / C++ | Rust |
|---|---|
| `std::vector<std::thread>` and `join` | `thread::scope` |
| `ASSERT_TRUE(ht.Insert(i, i))` | `assert!(ht.insert(&i, &i))` |
| `ht.VerifyIntegrity()` | `ht.verify_integrity()` |

**Port rule:** C++ tests that join a vector of threads become scoped threads.

## Learn more

- [`thread::scope`](https://doc.rust-lang.org/std/thread/fn.scope.html) · BusTub's [extendible_htable tests](https://github.com/cmu-db/bustub/tree/master/test/container/disk/hash)
