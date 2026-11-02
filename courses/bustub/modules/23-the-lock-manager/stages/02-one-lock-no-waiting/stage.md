Before a lock can make anybody wait it has to know who holds it. This stage builds that bookkeeping for **one** thing (a table or a row): the transactions that hold it and in which mode, and the rule every later stage rests on: **the holders of a lock are always compatible with each other**. No threads yet, no waiting: a request is granted now or refused now.

## The task

In `src/concurrency/lock_queue.rs`, `LockQueue`:

- `try_acquire(txn, mode)` answers one of three things: `Ok(true)` (it holds `mode` now: a new lock, the same lock again, or an upgrade of the one it had), `Ok(false)` (another holder is in the way; nothing changes), or an error when `mode` is not an upgrade of what it already holds.
- `release(txn)`, `holders()` (ordered by transaction id), `mode_of(txn)` and `is_empty()`.

The tests: exact scenarios (a new lock is empty; compatible requests share; an incompatible one is refused and leaves nothing behind; asking again is fine; an upgrade replaces the mode and is refused while another holder is in the way; a weaker mode is an error; releasing frees the lock), and a property: **for any sequence of requests and releases, every answer equals that of a model written from the textbook table, and the holders are always pairwise compatible**.

## Your freedom

How holders are stored (a `Vec`, a map), and whether you store anything beyond holders. The stage-1 functions decide compatibility; the tests do not look at your fields.

## The Rust toolbox

**Returning a `Result<bool, E>`.** "Granted" and "refused" are both normal answers; only a request that makes no sense is an error. `Ok(false)` is not a failure.

**Updating in place.** `iter_mut().find(..)` gives a mutable reference to one holder, so an upgrade changes the mode where it stands instead of adding a second entry.

## If this is new

- [S3 Vec & slices](/t/s3-vec-slices): `iter().any`, `retain`, `find`.
- [S1 Option & Result](/t/s1-option-result): a result with three outcomes.
- Y5 Testing & verification: Build it: check a concurrent system by the history it leaves.

## Tests

- A new lock; sharing; refusal; asking again; upgrades; a weaker mode; release.
- Property: the queue against a model of who holds what, with the pairwise-compatible invariant checked after every step.

## Hints

### What does a transaction that already holds the lock ask?

Three cases: the same mode (nothing to do), a stronger one (an upgrade, which must still fit with everybody *else*), a weaker or unrelated one (an error). Decide the case first, then look at the other holders.

### The other holders do not include the asker

An upgrade from S to X must not conflict with the asker's own S lock. Compare with the holders other than the one asking.

## Performance

Holders of one lock are few, so a scan is as fast as anything. The cost that matters later is that every request takes the manager's one lock; keep this structure free of locks of its own.

**Measure it.** A million acquire and release pairs on one queue should take milliseconds.

## Experiment

Optional. Predict first, then run.

1. **Compare the asker with itself too.** Which test finds that upgrades can never succeed?
2. **Forget to remove the old mode on upgrade.** Which property catches the duplicate holder?

## Other designs

- **A map from transaction to mode** makes `mode_of` constant time and `holders` need a sort.
- **A count per mode:** enough to answer "can another transaction get S?" without listing holders, not enough to say who to wake.

## In BusTub

BusTub's `LockRequestQueue` holds a list of requests with a `granted_` flag and the id of a transaction that is upgrading. This stage is its granted half; the waiting half comes next.

## The C/C++ way

| C / C++ | Rust |
|---|---|
| `std::list<std::shared_ptr<LockRequest>>` | a `Vec<(TxnId, LockMode)>` is plenty |
| `for (auto &r : queue) if (r->granted_ && !Compatible(...)) return false;` | `holders.iter().any(\|&(t, m)\| t != txn && !compatible(m, mode))` |

**Port rule:** state the invariant ("holders are compatible") and test it after every step, not only at the end.

## Learn more

- [`Vec::retain`](https://doc.rust-lang.org/std/vec/struct.Vec.html#method.retain) · [`Iterator::any`](https://doc.rust-lang.org/std/iter/trait.Iterator.html#method.any)
