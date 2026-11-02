A database runs many threads on one set of data, and from module 1 on you will share a buffer pool, a replacer and a log between them. Rust does not stop you from sharing; it makes you say how: `Arc` for owners, a `Mutex` for exclusive access, a `Condvar` for waiting until a condition is true, atomics for single numbers. This stage builds three small shared things in `src/rust_primer/shared.rs`.

## The task

- `BoundedQueue<T>`: at most `capacity` values wait in it. `push` **blocks while it is full**, `pop` **blocks while it is empty**, `try_pop` never blocks, and `close` ends the queue: after it, `push` gives the value back as `Err(value)`, and `pop` returns what is left and then `None`. `close` must wake every thread that is waiting.
- `IdGenerator`: `next(&self)` hands out 0, 1, 2, ... to any number of threads, each number once, with no lock (one atomic `fetch_add`).
- `parallel_sum(values, threads)`: split the slice into chunks and add each in its own **scoped** thread (`thread::scope`), so that the threads borrow `values` instead of copying it. Sums wrap on overflow; `threads == 0` counts as 1.

The tests: ordering; close; a `pop` that waits for a `push`; a `push` that waits for room; `close` waking a waiting `pop` and a waiting `push`; 4 threads taking 8 000 ids, none twice; three producers and two consumers that lose nothing; the sum for many thread counts; and a property: **a parallel sum equals the sequential one for any input and any thread count**.

## Your freedom

The state behind the mutex (a `VecDeque` and a flag, or something else), whether you use one `Condvar` or two, how the chunks are cut. The signatures are fixed.

## The Rust toolbox

**Mutex owns its data.** `Mutex<State>`: you can only reach the state through `lock()`, which gives a guard; the lock is released when the guard is dropped. There is no way to forget to lock.

**Condvar: wait in a loop.** `let guard = cv.wait(guard).unwrap();` releases the lock, sleeps, and takes the lock again before returning the guard. A wake-up does not mean the condition is true (another thread may have taken the value first, and some wake-ups are spurious), so always wait in a loop that re-checks:

```rust
let mut state = self.state.lock().unwrap();
loop {
    if let Some(v) = state.items.pop_front() { self.not_full.notify_one(); return Some(v); }
    if state.closed { return None; }
    state = self.not_empty.wait(state).unwrap();
}
```

**`notify_one` and `notify_all`.** One new value wakes one waiting `pop`; `close` changes the answer for everybody, so it wakes all.

**Atomics.** `AtomicU64::fetch_add(1, Ordering::Relaxed)` reads and increments in one indivisible step; `Relaxed` is enough when the number itself is all you share. A `load` followed by a `store` is a race.

**Scoped threads.** `thread::scope(|s| { s.spawn(|| ...); })` joins every thread before it returns, so the threads may borrow from the stack around them, with no `Arc` and no `'static`.

**`Arc<T>` to share ownership**, clone one per thread: `let q = Arc::clone(&q);`. A type used behind an `Arc` must be `Send + Sync`: a `Mutex<State>` is, whenever `State` is `Send`.

## If this is new

- [C1 Threads & shared state](/t/c1-threads-shared-state): `Arc`, `Mutex`, `Condvar`, scoped threads.
- [C3 Atomics & lock-free](/t/c3-atomics-lock-free): `fetch_add` and orderings.
- S7 Smart pointers & interior mutability: `Arc`.

## Tests

- A queue: order, `try_pop`, close, drain-after-close.
- Blocking: `pop` waits for a push; `push` waits for room (checked with a flag after a pause); `close` wakes a `pop` and a `push`.
- Ids: 8 000 ids from 4 threads, all different.
- Producers and consumers: nothing lost, nothing twice.
- `parallel_sum`: many thread counts; wrapping; the empty slice; a property against the sequential sum.

## Hints

### Lock, check, wait, check again

`wait` hands you the guard back; assign it to the same variable and go round the loop. An `if` instead of a `loop` works until two consumers race for one value.

### Wake the other side

A `pop` makes room: wake a waiting `push`. A `push` adds a value: wake a waiting `pop`. Forget one and a test hangs. (A hang is a missing wake-up; the test harness will stop it after a while.)

### `close` sets the flag under the lock

Set `closed` while holding the lock, then notify. Notifying first lets a waiter check, see nothing, and go back to sleep just before the flag is set.

## Performance

Every `push` and `pop` takes the one lock, so the queue's throughput is bounded by how fast threads can pass that lock around: a coarse design that is correct first. `fetch_add` is a single instruction and scales better, but a counter all threads hit is still a cache line they fight over.

**Measure it.** Run 4 producers and 4 consumers moving 1 000 000 values through queues of capacity 1, 16 and 1024. Capacity 1 forces a thread switch per value; the larger ones batch.

## Experiment

Optional. Predict first, then run.

1. **`if` instead of `loop`** around the wait. Does any test fail on your machine? Does that make it right?
2. **Drop the `notify_all` in `close`** (use `notify_one`). Which test hangs, and why does exactly one thread wake?

## Other designs

- **`std::sync::mpsc::sync_channel`** is this queue, built in; one consumer only.
- **`crossbeam-channel`:** a faster lock-free queue with several consumers.
- **A lock-free ring buffer** with atomics only: much faster, much harder (see the optional *lock-free basics* concept).
- **`rayon`'s `par_iter().sum()`** is `parallel_sum` with a thread pool.

## In BusTub

BusTub's `Channel<T>` (`common/channel.h`) is this queue (a mutex, a condition variable and a `std::queue`), and the disk scheduler of module 1b is a worker thread that blocks on it. Module 1a's `DiskManager` is shared by `&self` and `Arc`, as the page file here is.

## The C/C++ way

| C / C++ | Rust |
|---|---|
| `std::unique_lock lock(mu); cv.wait(lock, [&]{ return !q.empty(); });` | `while items.is_empty() { state = cv.wait(state).unwrap(); }` |
| `std::atomic<uint64_t> next; next.fetch_add(1)` | `AtomicU64::fetch_add(1, Ordering::Relaxed)` |
| a `std::thread` that captures a reference (dangling if it outlives) | `thread::scope`: borrowing is checked |
| `mu` and the data it protects are separate fields | `Mutex<State>`: the data is inside |

**Port rule:** put the data inside the mutex; wait in a loop; notify after changing the condition.

## Learn more

- [`Condvar`](https://doc.rust-lang.org/std/sync/struct.Condvar.html) · [`thread::scope`](https://doc.rust-lang.org/std/thread/fn.scope.html) · [`AtomicU64`](https://doc.rust-lang.org/std/sync/atomic/struct.AtomicU64.html)
