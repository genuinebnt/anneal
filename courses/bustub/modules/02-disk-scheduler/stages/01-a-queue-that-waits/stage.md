The disk scheduler works on a hand-off: callers drop requests into a queue, and one background thread takes them out. The thread has nothing to do most of the time, so it must **sleep** while the queue is empty (a loop that keeps looking would burn a whole CPU) and **wake** the moment something arrives. That queue is `Channel<T>`, and you build it from a `Mutex` and a `Condvar`. The log writer, the lock manager and the checkpointer will wait on the same kind of queue later in the course.

> [!CHECK] Three threads call `get` on an empty channel and go to sleep. Another thread then calls `put` twice. Which threads wake up, and what does each receive? What must a thread do right after it wakes, before it takes anything?
> ||Two of the three wake, and each receives one element, the oldest first; which two is not specified. The third stays asleep. A woken thread must look at the queue again, because another thread may have got there first or the operating system may have woken it for no reason (a *spurious wakeup*); only if an element is really there may it take one.||
>
> - How many threads does one `put` need to wake?
> - What does the woken thread know about the queue, and what does it only hope?
> - Where is the mutex while a thread is asleep?

## The task

`Channel<T>` is an unbounded first-in first-out queue shared by many threads. The struct, `Default` and `is_empty` are given; its contract is:

- `Channel::new()` is empty. `len()` is the number of elements waiting.
- `put(element)` adds an element at the back and **never blocks**.
- `get()` removes and returns the element at the front. If the channel is empty it **sleeps until one arrives**. An element is delivered to exactly one caller.
- `consume(channel, f)`, over a `Channel<Option<T>>`, takes elements in a loop and calls `f` on each `Some` value. It returns when it takes a `None`, and takes that `None` out of the channel. A `None` is a stop signal: the way to tell a worker to finish.

Two properties tie these together, and the tests check them on random sequences and with many threads: a single thread sees a plain FIFO queue, and with many producers and many consumers **every element arrives exactly once**: none lost, none twice.

## Your freedom

What holds the elements and how the lock and the condition variable are arranged are yours, including whether `put` wakes one waiter or all of them, and whether a poisoned mutex (see below) is ignored or reported. Only the behaviour above is tested. Several designs are listed under *Other designs*, and the public signatures leave all of them open.

## The Rust toolbox

**`Mutex<T>` owns its data.** There is no separate "data the mutex protects": the queue lives *inside* it, so you cannot touch it without locking. `let mut q = self.queue.lock().unwrap();` gives a guard that behaves like `&mut VecDeque<T>`, and the lock is released when the guard goes out of scope, which for `self.queue.lock().unwrap().push_back(x);` is the end of that statement.

**Why `.unwrap()` after `lock()`.** `lock()` returns a `Result` because a thread that panicked while holding the lock *poisons* it. For a plain queue of owned values, `lock().unwrap()` (turn the poisoning into a panic here too) is a sound default; the *lock poisoning* concept discusses the alternatives.

**`Condvar::wait_while` sleeps on a condition.** `self.ready.wait_while(guard, |q| q.is_empty()).unwrap()` unlocks the mutex, sleeps, re-locks when woken and checks the closure again; it only returns when the closure says `false`, so a spurious wakeup is handled for you. It takes the guard by value and returns it, so the usual shape is `let mut q = self.ready.wait_while(self.queue.lock().unwrap(), |q| q.is_empty()).unwrap();`.

**`Option<T>` as a stop signal.** A channel of `Option<Job>` carries jobs as `Some(job)` and "no more jobs" as `None`. `while let Some(job) = ...` and `match` read naturally with it, and no special sentinel value is needed.

**Values without `Clone`.** `put(element: T)` takes the value by move; nothing here may require `T: Clone` or `T: Debug`. If the compiler says "the trait bound `T: Clone` is not satisfied", you reached for `.clone()` where a move was enough.

## If this is new

- [L1 Ownership & moves](/t/l1-ownership-moves): what a move is, and why `put(element)` takes its argument by value.
- [L2 Borrowing](/t/l2-borrowing), the first problems: why a lock guard borrows the mutex, and when it ends.
- [S1 Option & Result](/t/s1-option-result): `Option` and `?`/`unwrap`, which the stop signal and `lock()` use.
- [S3 Vec & slices](/t/s3-vec-slices) for the queue's storage (`VecDeque` works like `Vec`).
- There is no threads track yet: the optional concepts *condvars and blocking queues* and *mutex owns its data* carry the concurrency, and *reading compiler errors* covers the messages you will meet.
- [C1 Threads & shared state](/t/c1-threads-shared-state): Understand it; Build it: poisoning, `RwLock`, deadlock and lock order, a bounded blocking queue with `Condvar` (`while`, never `if`).
- [C2 Message passing](/t/c2-message-passing): Use it; Understand it: a worker loop that stops when told, graceful shutdown with no lost jobs, a bounded channel from `Mutex` + `Condvar`.
- S5 Queues & heaps: Use: `VecDeque` as a queue.
- L6 Closures & functional Rust: Closure basics; Fn / FnMut / FnOnce: `move` closures, a callback that mutates state.

## Tests

- A single thread sees a FIFO queue, and `len` is always the number of elements waiting; `put` never blocks.
- `get` on an empty channel waits, and a later `put` wakes it.
- `consume` calls `f` in order, eats the `None`, and leaves what follows it.
- Four producers and four consumers: every element is delivered exactly once.

## Hints

### Where does a sleeping thread wait?

A thread that sleeps inside `get` must not hold the lock while it sleeps, or `put` could never add the element it is waiting for. How does `Condvar::wait` solve that? Read its documentation and write down what it does with the guard you give it.

### What if the thread wakes and the queue is empty?

Picture two getters asleep and one `put` that wakes both (with `notify_all`), or a getter that arrives just after the `put` and takes the element first. Your code has to be right in every interleaving, so the waiting is a *loop on a condition*, not a single wait. Which of the two functions above builds that loop for you?

### Stop signals

`consume` should not need to know anything about the elements. What type does it match on, and what happens if `f` panics halfway: is the channel still usable? (Not tested; worth knowing.)

## Performance

A `put` and a `get` with no waiting are a lock, a deque operation and an unlock: tens of nanoseconds. A `get` that has to sleep costs a **context switch**, microseconds, which is why the scheduler hands over *batches* of requests where it can. Notifying when nobody is waiting costs a system call in some implementations; `notify_one` on a `Condvar` with no waiters is cheap in Rust's standard library, but not free.

**Measure it.** Time 1 million `put`s followed by 1 million `get`s on one thread, then the same with one producer thread and one consumer thread. Predict the ratio first. What did the second one pay for?

## Experiment

Optional. Predict first, then run.

1. **Lost wakeups.** Change `get` to `if q.is_empty() { q = self.ready.wait(q).unwrap(); }` and run the 8-thread test many times (`for i in $(seq 50); do cargo test ... ; done`). Does it fail? Which interleaving makes it fail, and how often?
2. **Wake all.** Switch `notify_one` to `notify_all` and time the 4-producer, 4-consumer test. What changes, and why is it still correct?

## Other designs

- **`Mutex<VecDeque<T>>` + `Condvar` (ours).** The textbook version, the one BusTub uses.
- **A lock-free queue.** Faster under contention, much harder to get right; you still need something to sleep on when empty.
- **`std::sync::mpsc` or `crossbeam-channel`.** A ready-made channel; its receiver cannot be shared between consumers, which is why BusTub (and this course) builds its own. After this stage, read how they do it.
- **A bounded queue.** `put` waits when the queue is full. Adds backpressure; changes the contract (`put` may block).

## In BusTub

```cpp
template <class T>
class Channel {
 public:
  void Put(T element) {
    std::unique_lock<std::mutex> lk(m_);
    q_.push(std::move(element));
    lk.unlock();
    cv_.notify_all();
  }
  auto Get() -> T {
    std::unique_lock<std::mutex> lk(m_);
    cv_.wait(lk, [&]() { return !q_.empty(); });
    T element = std::move(q_.front());
    q_.pop();
    return element;
  }
 private:
  std::mutex m_;
  std::condition_variable cv_;
  std::queue<T> q_;
};
```

## The C/C++ way

| C / C++ | Rust |
|---|---|
| `std::mutex` and a separate `std::queue` it protects | `Mutex<VecDeque<T>>`: the data is inside the lock |
| `std::unique_lock<std::mutex> lk(m_);` | `let q = self.queue.lock().unwrap();` (the guard is the lock) |
| `cv_.wait(lk, pred)` | `cv.wait_while(guard, \|q\| !pred).unwrap()` (the closure says "keep waiting") |
| `lk.unlock(); cv_.notify_all();` | drop the guard (end of scope, or `drop(q)`) then `notify_one()` |
| `std::move(q_.front())` then `pop()` | `pop_front()` returns the owned value |

**Port rule:** a mutex plus the data it guards plus a condition variable becomes one `Mutex<T>` and one `Condvar`; the predicate passed to `wait` is inverted in Rust (`wait_while` waits *while* it is true).

## Learn more

- [`Condvar`](https://doc.rust-lang.org/std/sync/struct.Condvar.html) · [`Mutex` and poisoning](https://doc.rust-lang.org/std/sync/struct.Mutex.html#poisoning) · [`VecDeque`](https://doc.rust-lang.org/std/collections/struct.VecDeque.html)
- *Rust Atomics and Locks*, [Building our own channel](https://marabos.nl/atomics/building-channels.html)
- Jon Gjengset, [Crust of Rust: Channels](https://www.youtube.com/watch?v=b4mS5UPHh20)
