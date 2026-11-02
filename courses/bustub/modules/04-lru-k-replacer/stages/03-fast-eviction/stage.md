`evict` scanned every frame, which is fine for a few hundred and hopeless for a pool of a million. Evictions are on the critical path of every miss, so this stage makes `evict` **O(log n)**: keep the evictable frames in a structure that always knows the best victim. The policy does not change, so every earlier test must still pass; only the cost does.

> [!CHECK] You keep the evictable frames in a sorted set, ordered by "victim first". An access to a frame changes its position in the order. How do you update the set when that happens, and why must you remove the old entry *before* you change the frame's history?
> ||Sets find an element by its current key. If you change the history first, the key you compute afterwards is the new one and the old entry, stored under the old key, can no longer be found: it stays in the set as a ghost and the same frame appears twice. So: compute the old key, remove it, change the state, compute the new key, insert it. The same order applies to every operation that changes a frame's key or its evictability.||
>
> - Which operations change a frame's key?
> - Which change whether it is in the set at all?
> - What invariant relates the set to the map of frames?

## The task

The behaviour of `LruKReplacer` is exactly that of 1d-02. The new requirement is cost: `record_access`, `set_evictable`, `evict` and `remove` must each be at most O(log n) in the number of tracked frames. The test builds a replacer of 100 000 evictable frames and performs 100 000 evictions, each followed by an access and a change of evictability for the frame that came back, under a limit of ten seconds (the intended design needs milliseconds).

All the earlier tests stay and must keep passing; they are the proof that the faster structure has the same policy.

## Your freedom

Which ordered structure you use (`BTreeSet`, `BTreeMap`, a binary heap with lazy deletion, a skip list from module 0b, your own tree), and what the key contains.

## The Rust toolbox

**`BTreeSet<(A, B, C)>` as a priority queue you can delete from.** It keeps its elements sorted; `first()` is the smallest, `insert` and `remove` are O(log n), and unlike `BinaryHeap` it can remove an arbitrary element given its value. A tuple key sorts lexicographically.

**A key that includes the frame id.** Two frames could in principle share the other components of the key; adding the `FrameId` makes every key unique so that a set does not drop one of them.

**Derive the order you need.** `FrameId` already implements `Ord`; tuples of `Ord` types are `Ord`; you do not have to write a comparison.

**One function that computes the key.** `fn key(node: &Node) -> (u8, usize, FrameId)` called at every place that inserts or removes. Duplicated key computations drift; a single function cannot.

**A checker for the invariant.** In a `#[cfg(debug_assertions)]` block (or a `fn check(&self)` called from tests you write), assert that `order.len() == curr_size` and that every element of `order` names an evictable tracked frame. Run it after every operation while you develop. It is how you find the ghost entry of the check-yourself question.

**`BinaryHeap` is not enough on its own.** It cannot delete from the middle; a "lazy deletion" design pushes a new entry on every change and discards stale entries when they surface. Workable, but you need a way to recognise a stale entry (a version number per frame).

## If this is new

- [S4 Maps & sets](/t/s4-maps-sets): ordered versus hashed collections.
- [S3 Vec & slices](/t/s3-vec-slices) for tuples and sorting by key.
- The *ordered sets as priority queues* concept (optional) walks through this exact design and the lazy-deletion alternative.
- S5 Queues & heaps: Understand: a priority queue you can delete from; lazy deletion.
- S8 The core traits: Implement by hand: `Ord` on tuples, deriving the order you need.

## Tests

- 100 000 evictions among 100 000 frames (each followed by a re-access and an evictability change) complete within the limit.
- All the earlier properties still hold (they run again as part of this stage's regression).

## Hints

### Which operations touch the set?

`record_access` on an evictable frame changes its key. `set_evictable` adds or removes the frame. `evict` and `remove` take it out. A non-evictable frame is not in the set at all, so an access to one must not insert it. Write the four cases down before you code.

### Remove first, then change

For every operation that changes a frame's key, the order is: take the old entry out, change, put the new entry in. Which function can you write once so that no operation forgets?

### The test is slow, why?

Run the test in release mode and print the time. Count how many times your `evict` touches each frame: an O(n) scan hides inside `min_by_key`, in a `contains`, in a `Vec::remove(0)` or in building a fresh collection per call.

## Performance

`BTreeSet` operations are O(log n) with a small constant and good cache behaviour, since it stores several keys per node. At 100 000 frames an operation touches around 3 to 5 nodes. The hash map lookup that finds a frame's node is O(1). In total an eviction costs a few hundred nanoseconds.

**Measure it.** Time 1 million evictions at 1 000, 100 000 and 1 000 000 frames. Does the time per eviction grow? By how much, and does that match "log n"?

## Experiment

Optional. Predict first, then run.

1. **Lazy deletion.** Replace the set by a `BinaryHeap` with a version number per frame and drop stale entries in `evict`. How big can the heap get under a workload of mostly accesses? Is `evict` still O(log n) in the worst case?
2. **Vec<Option<Node>>.** Store nodes in a vector indexed by frame id instead of a `HashMap`. How much faster is a lookup? What did you give up?

## Other designs

- **`BTreeSet` of keys (ours).** Deletion by value, `first()` for the victim.
- **`BinaryHeap` with versions.** Cheaper pushes, but stale entries accumulate.
- **Two ordered sets.** One for infinite-distance frames (by first access) and one for the rest (by k-th access). Smaller keys; the move between sets happens when a frame reaches `k` accesses.
- **A skip list or your own balanced tree.** Possible; module 0b builds a skip list.

## In BusTub

BusTub's own tests are small enough for a linear scan to pass them. The O(log n) version is how a production system would do it, and one of the places this course goes past the project.

## The C/C++ way

| C / C++ | Rust |
|---|---|
| `std::set<std::tuple<...>>` with `erase(key)` | `BTreeSet<(..)>` with `remove(&key)` |
| `std::priority_queue` (no deletion) | `BinaryHeap` (no deletion), same limits |
| `*set.begin()` | `set.first()` returns an `Option<&K>` |

**Port rule:** `std::set` is an ordered tree, so its Rust equivalent is `BTreeSet`; `std::unordered_set` is `HashSet`.

## Learn more

- [`BTreeSet`](https://doc.rust-lang.org/std/collections/struct.BTreeSet.html) · [`BinaryHeap`](https://doc.rust-lang.org/std/collections/struct.BinaryHeap.html)
