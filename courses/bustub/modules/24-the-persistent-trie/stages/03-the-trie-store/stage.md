A persistent trie is the perfect basis for a concurrent store. A **reader** takes the current version (one pointer copy under a lock held for a moment) and reads it without any lock; the version cannot change under it. A **writer** builds the next version from the current one and publishes it by swapping the root pointer. Writers must take turns, or two writes starting from the same version would lose one of the changes.

## The task

In `src/primer/trie_store.rs`, `TrieStore` has two mutexes: `root` (guards the pointer to the current version) and `write_lock` (makes writers take turns).

- `get::<T>(key) -> Option<ValueGuard<T>>`: lock `root` just long enough to clone the trie, then read with `get_shared` and wrap the value in a `ValueGuard` (given: it derefs to the value and keeps it alive).
- `put(key, value)` and `remove(key)`: hold `write_lock` for the whole operation; clone the current trie under `root`; build the new trie **without** holding `root`; take `root` again only to store the new trie.

The tests: exact scenarios (put, get and remove; a guard stays valid after the key is removed; a guard sees the version it was taken from; values need not be clonable; four writers and four readers; readers are not blocked by a writer building a big trie), and a property: **the store used from one thread behaves like a map, and a guard taken before later writes still holds the value it was taken with**.

## Your freedom

How you share the root (two mutexes as in the intended design, a `RwLock<Arc<Trie>>`, an `ArcSwap`), as long as readers never wait for a writer to finish building and writers take turns.

## The Rust toolbox

**Lock, clone, unlock, work.** `let snapshot = self.root.lock().unwrap().clone();` holds the lock for the duration of one pointer clone (the temporary guard drops at the end of the statement); the work happens on the clone.

**A second lock for writers.** `let _writer = self.write_lock.lock().unwrap();` kept alive for the whole function makes writers take turns without blocking readers, who only ever take the first, short lock.

**A guard that owns its value.** `ValueGuard<T>` holds an `Arc<T>`: it keeps the value alive even if the trie version it came from is dropped.

**`Default` for a struct of locks.** `#[derive(Default)]` builds the store with an empty trie and an unlocked mutex; keep your fields `Default`.

```rust
let snapshot = self.root.lock().unwrap().clone();     // a Trie is a pointer: cloning is cheap
let _writer = self.write_lock.lock().unwrap();        // released at the end of the function
*self.root.lock().unwrap() = next;                    // publish
```

## Design notes

**Two locks, two jobs.** `root` protects the pointer and is held for a few instructions; `write_lock` protects the *read-modify-write* of a whole update and may be held for the time it takes to build a version. If building the version held `root`, every reader would wait for the writer: the point of the design would be lost.

**Why the guard owns the value.** `get_shared` returns an `Arc<T>`, and the guard holds it. The trie version the value came from can be dropped (the key removed, the store overwritten) and the value lives on until the guard is dropped. In C++ this is why `ValueGuard` also holds the root.

**No deadlock.** Writers take `write_lock` then `root`; readers take only `root`. A fixed order with no cycle.

## If this is new

- [C1 Threads & shared state](/t/c1-threads-shared-state): `Mutex`, lock scope, a clone under a short lock.
- S7 Smart pointers & interior mutability: `Arc` as a snapshot.
- Y5 Testing & verification: guards that must keep their values after later writes.
- The optional *persistent data structures and path copying* concept.

## Tests

- Put, get, remove; guards after removal and across versions; non-clonable values; writers and readers together; a slow writer does not block readers.
- Property: single-threaded equivalence with a map; guards keep their values.

## Hints

### Clone under the lock, work outside it

Hold `root` only for `clone()` and for the store. Everything between is done on your private copy.

### The write lock covers read and write

`write_lock` must be taken before you read the current trie, not just before you store: otherwise two writers read the same version.

### Check for lost updates with many threads

The four-writer test inserts disjoint keys; if any is missing at the end, two writers raced.

## Performance

A read costs a short critical section and then no synchronisation at all. A write costs the path copy. Readers never wait for writers' work, only for the pointer swap, so read throughput does not drop as writes get slower.

**Measure it.** Run 4 reader threads for a second with and without a writer running `put` in a loop; read counts stay about equal.

## Experiment

Optional. Predict first, then run.

1. **One lock for everything.** Hold the root lock while building. Which test shows readers blocked?
2. **No write lock.** Remove it. Which test loses an update, and how often?

## Other designs

- **Two locks and path copying (ours, BusTub's).**
- **`RwLock<Trie>`:** readers share, a writer blocks them while it builds.
- **An atomically swapped pointer** (`arc-swap`, RCU): readers never take a lock at all.
- **A concurrent map** (sharded locks): no snapshots, no old versions.

## In BusTub

`trie_store.h`: "`// This mutex protects the root. Every time you want to access the trie root or modify it, you will need to take this lock.`" and "`// This mutex sequences all writes operations and allows only one write operation at a time.`". The test `TrieStoreTest.ReadWriteTest` blocks a writer in the middle of constructing its value and checks that readers still run; in Rust a value is moved in, so the port's version of that check is "readers run while a writer builds a big trie".

## The C/C++ way

| C / C++ | Rust |
|---|---|
| `std::lock_guard<std::mutex>` | `Mutex::lock().unwrap()`; the guard unlocks when dropped |
| `ValueGuard(Trie root, const T &value)` | `ValueGuard { value: Arc<T> }` |
| `std::optional<ValueGuard<T>>` | `Option<ValueGuard<T>>` |

**Port rule:** "hold the root alive so the reference stays valid" is an `Arc` clone; no separate root is needed.

## Learn more

- [`Mutex`](https://doc.rust-lang.org/std/sync/struct.Mutex.html) · [Read-copy-update](https://en.wikipedia.org/wiki/Read-copy-update) (the same idea in the Linux kernel)
