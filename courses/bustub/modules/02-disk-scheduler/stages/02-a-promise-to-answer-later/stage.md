A caller who schedules a disk write wants to carry on and find out later whether it worked. The scheduler needs a way to hand back **one answer, once, from another thread**. That is a *promise* (the end the worker writes) and a *future* (the end the caller reads). Rust's standard library has no such pair for plain threads, so you build it. The buffer pool uses it for every page it reads, and the log manager will use it for every commit.

> [!CHECK] A worker thread is given a `Promise` and panics before it sets it. The caller is blocked in `Future::get`. What does the caller see today, with the simplest implementation you can think of? What would you want it to see instead, and which event in the life of the `Promise` is your chance to arrange it?
> ||With the simplest design the caller sleeps forever: nobody will ever set the value and nobody wakes it. What it should get is an error ("broken promise"). The chance is the promise being **dropped**: when the worker unwinds, the promise's destructor runs, and if no value was set it can record "broken" and wake the waiting future.||
>
> - What happens to a value owned by a thread that panics?
> - Which code runs when a `Promise` goes out of scope?
> - How many different states can the shared slot be in?

## The task

`promise::<T>()` returns a connected pair `(Promise<T>, Future<T>)`. Each end can be moved to a different thread.

- `Promise::set(self, value)` delivers `value`. It takes the promise **by value**, so a promise can be set at most once: the compiler enforces the "one-shot".
- `Future::get(self)` waits until the promise is set and returns `Ok(value)`. If the promise is **dropped without being set**, `get` returns `Err(BrokenPromise)` instead of waiting forever, even if it was already waiting when the drop happened.
- `Future::is_ready(&self)` is `true` once `get` would return without waiting (a value is there, or the promise is broken).
- `T` can be any type: the value is moved through, never copied (no `Clone` or `Debug` bound).

## Your freedom

Everything underneath: the shared slot, its lock, how a waiting future is woken. The tests see only the three functions above. You may build it from a `Mutex<Option<T>>` and a `Condvar`, a three-state enum, an atomic and a thread parker, or something else you design.

## The Rust toolbox

**Two owners, one slot: `Arc`.** The promise and the future must both reach the same slot. `Arc<Shared<T>>` is a reference-counted pointer that is `Send` when `Shared<T>: Send + Sync`; each end holds a clone. The compiler's complaint "`T` cannot be sent between threads safely" or "`Rc<..>` cannot be sent" tells you you reached for the wrong pointer or a missing `T: Send` bound.

**State as an enum.** `enum Slot<T> { Waiting, Ready(T), Broken }` makes impossible states unrepresentable: there is no "ready and broken". Matching on `std::mem::replace(&mut *guard, Slot::Waiting)` (or `mem::take`) lets you move the value *out* of a guard, which plain `match *guard { Slot::Ready(v) => v, .. }` refuses ("cannot move out of dereference").

**`self` by value means once.** `fn set(self, value: T)` consumes the promise; calling `set` twice is a compile error ("use of moved value"). Prefer this to a runtime check wherever "at most once" is the rule.

**`impl Drop`: running code when a value dies.** `impl<T> Drop for Promise<T> { fn drop(&mut self) { ... } }` runs when the promise goes out of scope, including while a thread unwinds from a panic. If `set` took the promise by value and moved its fields out, the `Drop` still runs for the leftover pieces; a common trick is to let `set` store the value and then let the normal drop see that the slot is already `Ready` and do nothing.

**Waiting on a state.** `condvar.wait_while(guard, |slot| matches!(slot, Slot::Waiting))` sleeps until the state changes; `matches!` is a shorthand for a one-arm `match` that returns `bool`.

## If this is new

- [L1 Ownership & moves](/t/l1-ownership-moves): moves, and why `self` by value consumes the promise.
- [S1 Option & Result](/t/s1-option-result): matching on enums, `Result<T, E>` as the answer of `get`.
- [L4 Traits & dispatch](/t/l4-traits-dispatch): a trait is how `Drop` plugs into the language.
- There is no smart-pointer or threads track yet: the optional concepts *smart pointers (Box, Rc, Arc)* and *interior mutability* cover `Arc<Mutex<..>>`, and *ownership of files and RAII* covers `Drop`.
- S7 Smart pointers & interior mutability: Use it; Understand it: `Arc`, why `Rc` cannot cross threads, shared state with two owners.

## Tests

- A value set before `get` is returned, and values that are not `Clone` move through.
- A `get` that is already waiting wakes when the value is set from another thread.
- A promise dropped unset gives `Err(BrokenPromise)`, including to a future that was already waiting.
- A promise that is set and then dropped still gives its value.

## Hints

### What does the future wait on?

The future needs two facts: whether anything has happened yet, and what. Sketch the states the shared slot can be in and who changes it from which. Then ask what each of `set`, `drop` and `get` does for each state.

### The drop after a set

`set(self, value)` ends with `self` going out of scope, and so `Drop::drop` runs even after a successful set. If `drop` marks the slot as broken whenever it runs, a successful value turns into an error. Make `drop` look at the state first.

### Waking the waiter on a drop

The easiest mistake: `drop` records "broken" but forgets to wake the future that is already asleep, and one test hangs for ten seconds and fails with a message about a future that was dropped without waking. Which call wakes it?

## Performance

One promise per request means one `Arc` allocation per request: a heap allocation of tens of bytes and an atomic counter, roughly 50 ns. A request that goes to disk costs ten thousand times more, so the pair is not the bottleneck. The real cost is the **wakeup**: a sleeping thread needs the operating system to reschedule it, often 5 to 50 microseconds.

**Measure it.** Set a promise from another thread and `get` it, a million times, and divide. Now spin on `is_ready()` instead of calling `get`, burning a core. Which is faster, and why would you still not ship the spin?

## Experiment

Optional. Predict first, then run.

1. **No drop.** Remove your `Drop` impl and run the hang test. The test has a timeout; what does the message tell you? Write the same test as a real caller sees it.
2. **A cheaper slot.** Rebuild the slot with `std::sync::OnceLock<T>` plus a `Condvar`. What does `OnceLock` give you for free, and what is still missing?

## Other designs

- **`Arc<(Mutex<Slot<T>>, Condvar)>` (ours).** The classic and the easiest to reason about.
- **`std::sync::mpsc::sync_channel(1)`.** A one-slot channel is a promise with a ready-made drop story (`recv` returns `Err` if the sender is dropped). Your pair could be a thin wrapper.
- **Lock-free: an atomic state plus `thread::park`.** No mutex; the future parks until the promise unparks it. Faster, and the memory ordering has to be right.
- **A real `async` future.** `.await` instead of blocking, polled by an executor. A different style of program; see *async versus threads*.

## In BusTub

```cpp
auto promise = std::promise<bool>();
auto future = promise.get_future();
// ... later, on the worker thread:
r.callback_.set_value(true);
// ... the caller:
future.get();
```

## The C/C++ way

| C / C++ | Rust |
|---|---|
| `std::promise<T>` / `std::future<T>` | the `(Promise<T>, Future<T>)` you build |
| `promise.set_value(v)` | `promise.set(v)` (consumes the promise) |
| `future.get()` throws `std::future_error(broken_promise)` | `future.get()` returns `Err(BrokenPromise)` |
| `std::shared_ptr<State>` | `Arc<Shared<T>>` |
| a destructor that checks "was a value set?" | `impl Drop for Promise<T>` |

**Port rule:** C++ exceptions for "no answer will ever come" become a `Result`; shared state held by two `shared_ptr`s becomes one `Arc` cloned into each end.

## Learn more

- [`Arc`](https://doc.rust-lang.org/std/sync/struct.Arc.html) · [`Drop`](https://doc.rust-lang.org/std/ops/trait.Drop.html) · [`mem::replace`](https://doc.rust-lang.org/std/mem/fn.replace.html)
- Tokio's [oneshot channel](https://github.com/tokio-rs/tokio/blob/master/tokio/src/sync/oneshot.rs), a production promise/future
- [Futures and promises](https://en.wikipedia.org/wiki/Futures_and_promises)
