**Where this fits.** The end of the module: long mixed runs against the model, then BusTub's own LRU-K scenario.

> [!CHECK] Give one short access sequence on which LRU-2 evicts a different frame from plain LRU, and one on which it evicts a different frame from LFU (evict the frame used least often overall). Say what each policy evicts.
> ||Against LRU: frame A accessed at times 0 and 1, frame B once at time 2. LRU evicts A (its last access is the oldest), LRU-2 evicts B (one access, infinite distance): B has not shown repeated use. Against LFU: frame D accessed five times at times 0 to 4, frame E three times at times 5 to 7. LFU evicts E (three uses against five); LRU-2 compares the second most recent access, time 3 for D and time 6 for E, and evicts D, whose popularity is old and has faded.||
>
> - What does LRU-K know that LRU does not?
> - What does it know that LFU does not?
> - Which of the two sequences would the scan-resistance property catch?

## The task

Nothing new to design.

- **Long runs.** For `k` in 1, 2, 3, 5: 15 000 random operations on 150 frames; every eviction equals the model's, and the size equals the model's count at every step.
- **BusTub's test.** `tests/lru_k_replacer_test.rs` is `lru_k_replacer_test.cpp`: six frames, a fixed order of accesses and evictions, `SetEvictable` flips, `Remove`, and the expected victims.

## Your freedom

The same as before.

## The Rust toolbox

**Read a failing run.** The message gives `k`, the step and the two victims. Reproduce on a small scale: write the first dozen operations as a test in your own scratch file and shorten until it still fails.

**Release builds for timing.** `cargo test --release --test stages_1d`.

## If this is new

- Everything is in the earlier stages of this module.

## Tests

- 15 000 random operations on 150 frames for four values of `k` agree with the model at every step.
- BusTub's `lru_k_replacer_test`.

## Hints

### The first differing step

Find the first step where your victim and the model's differ. Then print the full state of your replacer (histories, flags) just before it and compute by hand which frame the rule picks.

### Fixed by the model or by the rule?

If your replacer and the model disagree, check the model against the written rule on that single case. The model is short; if it is wrong, say so, because that is a bug in the course.

## Performance

Run the long test under `--release` and compare with the debug build. If the release run takes much longer than a second, one of your operations is not O(log n).

## Experiment

Optional. Predict first, then run.

1. **Adapt to the pool.** In module 1f the buffer pool will call `record_access` on every page access and `set_evictable(false)` while a page is pinned. Predict how many calls per second the replacer must sustain for a pool doing 1 million page accesses per second.
2. **Compare with LRU.** Replay the same cache simulation as in 1c-04 with LRU-K in place of the pair LRU/CLOCK. On a scan-heavy trace the difference shows; construct one.

## Other designs

None for this stage. The *Other designs* sections of 1d-01 to 1d-03 list the alternatives to compare with yours.

## In BusTub

`lru_k_replacer_test.cpp` is the public sample for CMU's project (their autograder runs more cases). Here nothing is hidden: the stage tests before this boss are the rest, and every one is in your repo. The scenario is the best short introduction to what `Evict`, `SetEvictable` and `Remove` must do together.

## The C/C++ way

| C / C++ | Rust |
|---|---|
| `ASSERT_EQ(std::optional<frame_id_t>(2), lru_replacer.Evict())` | `assert_eq!(Some(f(2)), lru_replacer.evict())` |
| `ASSERT_EQ(5, lru_replacer.Size())` | `assert_eq!(5, lru_replacer.size())` |
| `ASSERT_DEATH` / `ASSERT_THROW` for caller bugs | `std::panic::catch_unwind` in a test |

**Port rule:** a death test becomes a test that expects a panic.

## Learn more

- [`std::panic::catch_unwind`](https://doc.rust-lang.org/std/panic/fn.catch_unwind.html) · [`#[should_panic]`](https://doc.rust-lang.org/reference/attributes/testing.html#the-should_panic-attribute)
