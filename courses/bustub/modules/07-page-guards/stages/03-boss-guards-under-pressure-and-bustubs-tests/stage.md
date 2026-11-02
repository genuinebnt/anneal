**Where this fits.** The end of Project 1: guards moved around like ordinary values, many threads incrementing one page, the guard model under your replacers, and BusTub's two guard-era test files.

> [!CHECK] A guard is moved into a `Vec`, then the vector is dropped. In which order are the guards released, and does the order matter for the pool's invariants? What if a guard is overwritten by assignment (`guard = other_guard;`)?
> ||A vector drops its elements in order, first to last; each guard unlatches then unpins on its own, so the order does not matter for the pool as long as each guard is independent. Assignment drops the old value first (after evaluating the right-hand side), so the old guard releases its pin and latch at that point; if the new guard is for the same page and the old one held its write latch, the right-hand side would have waited for it forever: take the new guard after releasing the old one.||
>
> - What does Rust guarantee about when a value is dropped?
> - Which pattern deadlocks a single thread against itself?
> - How does a moved-from guard avoid double release?

## The task

Nothing new to design.

- **Moves.** Guards moved into a `Vec` keep their pages pinned until dropped; assigning a guard drops the one it replaces.
- **Many threads, one page.** Eight threads increment a counter in one page through write guards; no update is lost.
- **Your replacers.** The guard model of 1g-01 (any order of guards, pin counts, data and failures) runs on a pool that uses the ARC and the LRU-K replacers you built.
- **BusTub's tests.** `buffer_pool_manager_test.rs` (the guard-era tests: `VeryBasicTest`, `PagePinEasyTest`, `PagePinMediumTest`, `PageAccessTest`, `ContentionTest`, `DeadlockTest`, `EvictableTest`, and so on) and `page_guard_test.rs` (`DropTest` and `MoveTest`), each ported test for test.

## Your freedom

The same as before.

## The Rust toolbox

**Read BusTub's scenarios as properties.** `PagePinMediumTest` fills the pool, unpins half, and expects specific `None`s: those are the "fails exactly when every frame is pinned" property of 1f-02 written as a story.

**Moves and drops.** `let g2 = g1;` moves; `g1` cannot be used. `std::mem::replace(&mut slot, new)` swaps and returns the old value, which then drops at the end of the statement unless you bind it.

**Seeing a leak.** At the end of a test, `assert_eq!(bpm.get_pin_count(page), Some(0))` for every page you touched. A non-zero count names the leaking page.

## If this is new

- Everything is in the earlier stages of this module.
- Y5 Testing & verification: Build it: loom or a watchdog timeout for a lock bug.

## Tests

- Moves and assignment of guards; eight threads on one page; the guard model under ARC and LRU-K.
- BusTub's `buffer_pool_manager_test` and `page_guard_test`.

## Hints

### A BusTub test fails and none of mine do

Read the test's story and name the property it tests: "pinned pages are never evicted", "a deleted page's frame is reused", "flush does not unpin". Then write that as a three-line test of your own and shrink it.

### `DeadlockTest` hangs

It holds a write guard and has another thread read the page. That thread waits (correctly) until the guard is dropped; if the test hangs, check that your guard is really dropped, and that no pool call is waiting for the lock the main thread holds.

## Performance

Run the contention test under `--release` and note the time. Replace the `Mutex` around the pool's bookkeeping with a `parking_lot::Mutex` (or measure the sharded design from the 1f experiment) and compare.

## Experiment

Optional. Predict first, then run.

1. **A leak finder.** Add a debug-only counter of live guards to the pool. Panic in the pool's `Drop` if it is non-zero. Which of BusTub's tests, if any, trip it?
2. **Fairness.** Start one writer and eight readers on one page and measure how long the writer waits under `std::sync::RwLock`. Is it bounded on your platform?

## Other designs

None for this stage. The *Other designs* sections of 1g-01 and 1g-02 list the alternatives to compare with yours.

## In BusTub

These are the tests of the current project 1. When they pass, your pool, scheduler, replacer and guards work together. Project 2 builds indexes on top, using exactly this interface.

## The C/C++ way

| C / C++ | Rust |
|---|---|
| `auto guard = bpm->WritePage(pid);` | `let mut guard = bpm.write_page(pid);` |
| `guard.AsMut<Page>()->field = x;` | `guard.get_data_mut()[..].copy_from_slice(..)` or a typed view (module 2a) |
| `guard.Drop();` | `guard.release();` or `drop(guard);` |
| `std::move(guard)` | `let g2 = guard;` (a move is the default) |

**Port rule:** `std::move` disappears; the moved-from variable cannot be used again, and the compiler checks it.

## Learn more

- [`std::mem::replace`](https://doc.rust-lang.org/std/mem/fn.replace.html) · [`parking_lot`](https://docs.rs/parking_lot)
