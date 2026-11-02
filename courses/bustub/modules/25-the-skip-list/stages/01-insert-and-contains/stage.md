A skip list is a sorted linked list with express lanes: every node has a random height and takes part in that many levels, so a search can run along the top lane, drop down when it would overshoot and reach any key in about `log n` steps, with no rebalancing. This stage writes the whole structure and its core: the **search** (from the top level go right while the next key is before the sought one, then drop a level), which also returns the **update vector** (the last node visited on every level); and **insert** and **contains** built on it, with the observers the tests use to look inside.

> [!CHECK] A skip list holds 1, 4, 6, 9 with heights 1, 3, 1, 2. Which keys are on levels 0, 1 and 2? You insert 5 with height 2: which nodes' links change on each level, and what is the update vector for 5 before the insert? What does `contains(5)` do before and after?
> ||Level 0: 1, 4, 6, 9; level 1: 4, 9; level 2: 4. The update vector for 5 is the last node before 5 on each level: level 2: 4, level 1: 4, level 0: 4. Inserting 5 with height 2 links it after 4 on levels 0 and 1: 4's links on levels 0 and 1 now point to 5, and 5 points to what 4 pointed to (6 on level 0, 9 on level 1); level 2 is unchanged. `contains(5)` searches down to level 0 and finds the candidate after 4; before the insert it is 6, which is not equivalent to 5; afterwards it is 5.||
>
> - What if the new height is taller than any node so far?
> - What decides that two keys are the same key?
> - What is the `HEADER`?

## The task

In `src/primer/skiplist.rs` the public API is fixed (`SkipList::new`, `with_compare`, `insert`, `contains`, `erase`, `clear`, `size`, `is_empty`, and the two observers `nodes` and `level`); the **inside is yours**: the node type, how nodes are linked and stored (an arena of nodes linked by index is the intended design: no `Rc<RefCell<..>>`), the search, the random height. The generator `Mt19937` (given) is seeded with the `SEED` const generic.

- `with_compare(compare)`: an empty list ordered by `compare(a, b)` = "is `a` before `b`?" (`new()` uses `<`); a header node with `MAX_HEIGHT` empty links; height 1; size 0; `Mt19937::new(SEED)`.
- the **search**: from the top level in use, on each level move right while the next key is before the sought one; remember the node you stop at on each level (the **update vector**); the candidate is the next node on level 0; it is found if the sought key is not before it (equivalence, as in `std::set`).
- `insert(&key)`: write lock; an equivalent key: `false`, nothing changes; draw a height (BusTub's rule: 1, plus one for every draw of the generator divisible by 4, capped at `MAX_HEIGHT`); if it exceeds the list's height raise it (the new levels start at the header); on each level `0..height` link the new node between `update[level]` and that node's next; size + 1; `true`.
- `contains`, `size` (read lock); `is_empty` is given.
- the observers: `nodes()` returns every key with the height of its node, in order (walk level 0); `level(l)` returns the keys linked on level `l`, in order.

The tests: exact scenarios (a new list is empty; insert adds and contains finds; a duplicate is refused and changes nothing; the list stays sorted whatever the insertion order; a comparison function decides the order; node heights come from the seeded generator; strings work too), and a property: **a list ordered by absolute value or in reverse** treats keys that neither precede the other as one key and keeps the order given. (The set-model property needs `erase`: it comes in stage 2.)

## Your freedom

Everything inside: how nodes are stored and linked, how you represent levels, whether you keep the height of the list or recompute it, and how you reuse the slots of erased nodes. The tests see the keys, the heights and the levels, nothing else.

## The Rust toolbox

**An arena instead of pointers.** `Vec<Node>` plus a `usize` index for a link: no `Rc`, no `RefCell`, no `unsafe`; a node that is erased leaves a slot you can reuse (`free: Vec<usize>`).

**Const generics.** `SkipList<K, const MAX_HEIGHT: usize = 14, const SEED: u32 = 15445>`: the cap and the seed are part of the type; `MAX_HEIGHT` is a constant you can use as an array length or a loop bound.

**A comparison function stored in the struct.** `compare: Box<dyn Fn(&K, &K) -> bool + Send + Sync>`: the order is chosen when the list is made; equivalence is `!compare(a, b) && !compare(b, a)`.

**`RwLock` for many readers.** `self.inner.read().unwrap()` for `contains` and `size`; `write()` for `insert`; one lock around the whole structure is the simplest correct design.

**`Option<usize>` links.** `links: Vec<Option<NodeId>>` indexed by level: `links.get(level).copied().flatten()` is 'the next node on this level, if the node is that tall and the link is set'.

```rust
let (update, found) = inner.search(&self.compare, key);
let id = inner.alloc(SkipNode::new(new_height, Some(key.clone())));
let prev = if level < old_height { update[level] } else { HEADER };
let next = inner.nodes[prev].next(level);
inner.nodes[id].set_next(level, next);
inner.nodes[prev].set_next(level, Some(id));
```

## Design notes

**The comparison is "is before".** Like `std::less`, `compare(a, b)` means a precedes b. Two keys are *equivalent* when neither precedes the other; that is the duplicate test, and why a list ordered by `>` works unchanged.

**Why the update vector.** After the search you know, for every level, which node must point to the new one. Without it you would search again for each level.

**New levels.** A node taller than the list has no predecessor recorded above the old height: the header is the predecessor there, and its links on those levels are empty.

**Heights are reproducible.** `random_height` draws from a port of `std::mt19937` seeded with the list's `SEED` (15445). The test in this stage lists the heights BusTub's test expects for 20 keys; if your search or insert draws a random number it should not (for a duplicate, say), the sequence shifts and the heights no longer match.

## If this is new

- [L5 Generics & associated types](/t/l5-generics): const generics, a stored closure.
- [S3 Vec & slices](/t/s3-vec-slices): an arena as a `Vec`, index links, a free list.
- [C1 Threads & shared state](/t/c1-threads-shared-state): `RwLock`.
- [F3 Memory & allocation](/t/f3-memory-allocation): why an index is a better pointer.
- The optional *skip lists* and *arenas* concepts.
- [D5 Linked lists](/t/d5-linked-lists): Linked lists, the Rust way: index-linked nodes in a `Vec`.
- Y5 Testing & verification: Build it: a `BTreeSet` as the oracle; a structural check after every step.

## Tests

- Empty list; insert and contains; duplicates; sorted order; a comparison function; seeded heights; strings.
- Property: custom orders.

## Hints

### Draw the height only for a new key

Check for the duplicate first; drawing a height for a rejected insert consumes a random number and changes every later height.

### The comparison for `found` goes the other way

Candidate is found when `!compare(key, candidate_key)`: you already know `candidate_key` is not before `key` (the search stopped there).

### Raise the height before linking

`update` only has entries below the old height; use the header for the new levels.

## Performance

Expected `O(log n)` per operation: with branching factor 1/4 the list has about `log_4 n` levels and a search takes about `(4/3) × log_4 n` steps per level... in total roughly `1.3 × log_4(n) × 3` comparisons. The worst case is `O(n)` but its probability is negligible.

**Measure it.** In release mode 200 000 inserts of increasing keys take about 27 ms and 200 000 lookups about 23 ms (roughly 115 ns each, mostly cache misses on the arena). Count the comparisons for 1 000, 10 000 and 100 000 keys: each tenfold increase adds a constant, not tenfold.

## Experiment

Optional. Predict first, then run.

1. **Always height 1.** Make every node height 1. Which tests fail, and what does the list degrade to?
2. **A different branching factor.** Use 1 in 2 instead of 1 in 4. Which test pins the seeded heights?

## Other designs

- **A skip list in an arena behind one lock (ours).**
- **Lock-free skip lists** (Java's `ConcurrentSkipListMap`): CAS on links, the standard concurrent ordered map.
- **A B-tree or a `BTreeSet`:** better cache behaviour, more complicated to split and merge.
- **A treap or an AVL tree:** deterministic or randomised balance with rotations.

## In BusTub

`skiplist.h`: "`The skip list is implemented as a linked list of nodes. Each node has a list of forward links. The number of forward links is determined by a geometric distribution. The skip list maintains a header node that is always at the maximum height of the skip list.`" and `RandomHeight`: "`Branching factor (1 in 4 chance), see Pugh's paper.`". The test `IntegrityCheckTest` checks the shape.

## The C/C++ way

| C / C++ | Rust |
|---|---|
| `std::shared_ptr<SkipNode>` links, `links_[level]` | `Option<usize>` links into `nodes: Vec<SkipNode>` |
| `Compare compare_` template parameter | a boxed closure `Fn(&K, &K) -> bool` |
| `std::shared_lock` / `std::unique_lock` on `rwlock_` | `inner.read().unwrap()` / `inner.write().unwrap()` |

**Port rule:** in safe Rust a node does not own its successor; the list owns all nodes and links name them by index.

## Learn more

- [Pugh's paper](https://15721.courses.cs.cmu.edu/spring2018/papers/08-oltpindexes1/pugh-skiplists-cacm1990.pdf) · [`RwLock`](https://doc.rust-lang.org/std/sync/struct.RwLock.html)
