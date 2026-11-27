Pages are read by many queries and written by few. A plain mutex lets one thread in at a time, readers included, which wastes the parallelism that is the whole point of a buffer pool. A **reader-writer latch** admits any number of readers *or* one writer. Every frame of the buffer pool (1f) sits behind a latch with exactly this contract, the page guards (1g) hand its guards out, and the B+ tree's latch crabbing (2c) is a discipline for taking them in order: this stage is where the contract and its policy questions are met on their own.

> [!CHECK] A page is held by a reader. A writer arrives and waits. Then a second reader arrives. Should the second reader be let in? Argue for each answer: what does each cost, to whom? Which starvation can each policy produce?
> ||Letting the reader in keeps throughput high (readers never wait for each other), but a steady stream of readers can keep the writer waiting forever: **writer starvation**. Making the second reader wait behind the writer is fair to the writer but lowers read concurrency and risks deadlock in code that takes a read latch twice in a row (the second read waits for the writer, the writer waits for the first read). Standard libraries make different choices; the operating system's `RwLock` usually leaves it unspecified, and so should code that depends on it.||
>
> - Who gets hurt by each policy under a read-heavy load?
> - What does your code do when it takes a read latch while it already holds one?
> - Which policy does `std::sync::RwLock` document?

## The task

`ReaderWriterLatch<T>` owns a value of type `T` and hands out guards:

- `new(value)` makes the latch.
- `read()` returns a guard that gives shared access (`*guard` reads). Any number of read guards may exist at once.
- `write()` returns a guard that gives exclusive access (`*guard = ..` writes). While it exists there are no other guards.
- A guard releases the latch when it is dropped.
- A holder that **panics** does not make the latch unusable for others: the data is plain bytes and the next user may carry on. (Rust's locks "poison" by default; a page latch should not.)

The properties the tests check: no update is lost under many writers; a reader never sees a write half done; readers can overlap; a writer keeps readers out until it lets go.

## Your freedom

The signatures return the standard library's guards (`RwLockReadGuard`, `RwLockWriteGuard`), so the latch wraps a `std::sync::RwLock`. What is yours is the poisoning policy and the reason you write down for it. A latch built from scratch, with guards of its own, is the first experiment.

## The Rust toolbox

**The guard is the permission.** `RwLock::read()` returns `RwLockReadGuard<'_, T>`, which implements `Deref<Target = T>`: while it lives you can read through it, and the borrow checker stops it outliving the latch. `write()` returns an `RwLockWriteGuard` with `DerefMut`. No unlock call exists; **dropping the guard unlocks**.

**Data owned by the lock.** `RwLock<T>` puts the value inside, like `Mutex<T>`. That is the reason `ReaderWriterLatch<T>` is generic: it is the lock *and* the data, so there is no way to touch the value without the right guard.

**Ignoring poisoning.** `lock.read().unwrap_or_else(PoisonError::into_inner)` takes the guard out of the error when the lock is poisoned. `PoisonError::into_inner` is a function path used as a closure; `unwrap_or_else` takes anything callable. You will use this pattern wherever the protected data stays valid after a panic.

**Returning a guard from your own method.** The return type `RwLockReadGuard<'_, T>` has an elided lifetime tied to `&self`: "this guard lives no longer than the latch it came from". If the compiler says "lifetime may not live long enough" you returned something that did not borrow from `&self`.

## If this is new

- [L2 Borrowing](/t/l2-borrowing): what a shared and a mutable reference promise, which is exactly what a read and a write guard promise.
- [L3 Lifetimes](/t/l3-lifetimes): the first problems, for the `'_` in the guard's type.
- [L5 Generics & associated types](/t/l5-generics): the first problems, for `ReaderWriterLatch<T>`.
- [C1 Threads & shared state](/t/c1-threads-shared-state): Understand it; Build it: poisoning, `RwLock`, deadlock and lock order, a bounded blocking queue with `Condvar` (`while`, never `if`).

## Tests

- A write changes what the next read sees; many writers never lose an update.
- A reader never sees the two halves of one write at different values.
- Four readers can be inside the latch at the same time; a writer excludes readers until it releases.
- A latch is usable after a holder panicked.

## Hints

### Can I use the standard library?

Yes. The point of this stage is the contract and the policy, not rebuilding a lock; the standard library's `RwLock` is a good implementation. If you want to build one yourself, the optional experiment is the place, and the tests will tell you whether it works.

### Why is poisoning a design question?

Poisoning exists because a thread that panicked in the middle of an update may have left the data broken. For a page of bytes there is no "broken" other than wrong contents, which the layers above can check themselves. Decide what you do, make the test pass, and write the reason in a comment where you decided.

### What does `drop` of a guard have to do?

If you build the latch from a mutex, a condition variable and a reader count, write down what a read guard's `drop` changes, and which waiters it must wake. A writer waits for the count to reach zero; whom does a writer's drop wake?

## Performance

An uncontended read latch is an atomic increment: about 5 to 20 ns. Under contention many readers on many cores each increment the **same** counter, bouncing its cache line between cores, so a read-mostly latch can be slower than it looks. Real systems shard hot latches or use optimistic reads (see *optimistic latching*).

**Measure it.** Eight threads each take a read latch 5 million times: against one `RwLock`, against eight separate `RwLock`s, and against one `Mutex`. Predict which is slowest before you run it.

## Experiment

Optional. Predict first, then run.

1. **Build your own.** In a scratch file, write `MyLatch<T>` from a `Mutex<(usize, bool)>` (reader count, writer flag), a `Condvar` and an `UnsafeCell<T>`, with guard types of your own that implement `Deref` (and `DerefMut` for the writer) and release in `Drop`. Copy the six tests over. Which are the hardest to pass, and why does the type need `unsafe impl Sync`?
2. **Starve a writer.** With the standard `RwLock`, run eight threads that read in a loop and one that writes. Does the writer finish? On your platform? Relate it to the check-yourself question.

## Other designs

- **`std::sync::RwLock` (ours).** Operating-system backed, correct, a documented-as-unspecified fairness.
- **`parking_lot::RwLock`.** No poisoning, smaller, faster in many workloads, and a fair queue.
- **A spin lock built on atomics.** Short critical sections only; wasting a core while waiting is fine for nanoseconds, awful for milliseconds.
- **A seqlock.** Readers never write to shared memory and retry if a writer interfered: excellent for read-mostly small data, impossible for data with pointers.

## In BusTub

```cpp
class ReaderWriterLatch {
 public:
  void WLock() { mutex_.lock(); }
  void WUnlock() { mutex_.unlock(); }
  void RLock() { mutex_.lock_shared(); }
  void RUnlock() { mutex_.unlock_shared(); }
 private:
  std::shared_mutex mutex_;
};
```

## The C/C++ way

| C / C++ | Rust |
|---|---|
| `std::shared_mutex`, separate `lock()`/`unlock()` calls | `RwLock<T>`, a guard whose `Drop` unlocks |
| `RLock()` ... `RUnlock()` and a bug if you forget one | `let g = latch.read();` ... end of scope |
| the data and its latch are two members | `ReaderWriterLatch<T>` holds both |
| no poisoning (an exception skips the unlock, so you use `std::lock_guard`) | poisoning exists; page latches opt out |

**Port rule:** paired lock/unlock calls become a guard value; hold the guard for exactly as long as you hold the latch.

## Learn more

- [`RwLock`](https://doc.rust-lang.org/std/sync/struct.RwLock.html) · [`PoisonError::into_inner`](https://doc.rust-lang.org/std/sync/struct.PoisonError.html#method.into_inner) · [`parking_lot`](https://docs.rs/parking_lot)
- *Rust Atomics and Locks*, [Building our own locks](https://marabos.nl/atomics/building-locks.html)
