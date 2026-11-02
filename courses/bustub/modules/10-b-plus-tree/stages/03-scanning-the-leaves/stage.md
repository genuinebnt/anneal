The leaves are linked left to right, so a range scan finds its first key with one descent and then walks the chain without touching the internal pages again. The `IndexIterator` is that walk. It must be cheap to hold: a database may keep thousands of iterators alive at once (one per open cursor), so an iterator **does not keep any page pinned or latched** between calls: it remembers a position (a leaf page id and a slot) and re-latches that leaf for just long enough to copy the next pair out.

> [!CHECK] An iterator remembers `(leaf page id, slot)`. Between two calls to `next`, another thread splits that leaf and half of its pairs move to a new leaf. What can the iterator return on the next call, and is that acceptable? What would it cost to forbid it?
> ||The iterator may return a pair that has moved (it is looked up at the remembered slot, which now holds a different pair or is past the end of a shorter leaf and then the walk continues to the next leaf), so a concurrent scan can miss or repeat keys. BusTub's iterator accepts this; a scan over a changing tree is not a snapshot. To forbid it you would have to hold the leaf's read latch between calls, which blocks writers (and the buffer pool's eviction) for as long as the caller keeps the iterator, or use versioned pages. The rule here is the cheap one: no latch kept.||
>
> - What does the iterator remember?
> - What if the slot is past the end of the leaf?
> - What happens to an iterator when the tree is empty?

## The task

- `begin()`: an iterator at the smallest key. `begin_at(&key)`: at the first key that is **not less than** `key`. `end()`: past the last key. An empty tree's `begin()` equals its `end()`.
- `IndexIterator` implements `Iterator<Item = (K, V)>`: `next` returns the current pair and moves on, `None` at the end. `is_end()` says whether there is nothing left; `==` compares positions: two iterators are equal when they are at the same place (any two ends are equal).
- A position past the last slot of a leaf is the first slot of the next leaf, or the end.
- Nothing stays pinned or latched between calls.

Tests: a scan crosses many leaves and returns every key once, in order, with its own value; `begin_at` for present, absent, too small and too large keys; position equality and reaching the end exactly once; **ten live iterators in a pool of five frames** (a pinned page per iterator would exhaust it) while the tree is still modifiable; and a property that scans and `begin_at(k)` equal a `BTreeSet`'s iteration and `range(k..)`.

## Your freedom

What an iterator holds (the pool, a leaf id, a slot, a copy of the current leaf's keys), and how the tree builds one (the constructors are yours: the tree and the iterator are both your code).

## The Rust toolbox

**Implementing `Iterator`.** `impl Iterator for IndexIterator<'_, K, V, TOMBS> { type Item = (K, V); fn next(&mut self) -> Option<(K, V)> { .. } }` gives `for (k, v) in tree.begin()`, `.map`, `.collect`, `.filter` and the rest for free.

**A lifetime parameter for the pool.** `IndexIterator<'a, ..>` holds `&'a BufferPoolManager` so it can latch leaves later; the compiler then guarantees the pool outlives every iterator. `tree.bpm.inner()` is the plain pool with the right lifetime.

**`PartialEq` by hand.** `impl PartialEq for IndexIterator<..> { fn eq(&self, other: &Self) -> bool { .. } }`: compare the positions. Ends compare equal whatever they remember.

**Latch, copy, release.** `let pair = { let guard = pool.read_page(id); Leaf::new(&guard[..]).entry_at(slot) };` the block ends and the guard with it before you do anything else.

**`?` on an `Option` in `next`.** `let page_id = self.page_id?;` returns `None` at the end.

**Why `is_end` as well as `Iterator`.** BusTub's tests are written `while !it.is_end()`; Rust style is `for`/`while let Some(..) = it.next()`; both are provided.

## If this is new

- [L3 Lifetimes](/t/l3-lifetimes): a struct that holds a reference.
- S6 Iterators: implementing the trait, what `for` desugars to.
- [L4 Traits & dispatch](/t/l4-traits-dispatch): `PartialEq` by hand.
- The optional *range scans and the leaf chain* concept explains the walk.

## Tests

- An empty tree begins where it ends.
- A scan visits every key once in order across many leaves, with each key's own value.
- `begin_at` starts at the first key not less than its argument (present, absent, below all, above all).
- Iterators compare by position; the end is reached exactly once.
- An iterator keeps no page latched or pinned: ten live iterators in a five-frame pool, and the tree can still be changed.
- A property: scans and `begin_at(k)` equal the sorted model and its `range(k..)` for random trees.

## Hints

### Where is the first key?

For `begin`: descend always taking the first child. For `begin_at(k)`: descend as a lookup does, then binary search the leaf for the first key not less than `k`; if that is past the leaf's end, the position is the next leaf's slot 0. Who handles that: the tree or the iterator constructor?

### A leaf that is empty

A leaf can be empty in a tombstone tree (module 2d) or momentarily under concurrency. The walk must skip it, not stop there.

### A scan that starts at a leaf you still hold

If `begin` finds the leaf with a read guard and then builds an iterator that reads the same leaf, you take two read latches on one page in one thread. With a writer waiting for the first, the second one queues behind it and nobody moves: a deadlock that shows up one run in twenty. Drop the guard (keep only the page id) before the iterator latches the leaf. The reader-writer latches concept has the whole story.

### The end

Choose a representation for "at the end" (no leaf) and make `==`, `is_end` and `next` all agree on it.

## Performance

A scan reads each leaf once: `n / L` page reads for `n` keys, sequential in the leaf chain (often adjacent pages). Each `next` latches the leaf, copies 16 bytes and unlatches: the latch cost per key is real; a faster iterator copies the whole leaf (or the rest of it) at once and serves the keys from the copy.

**Measure it.** Scan 1 million keys with the latch-per-key design and with a design that copies a leaf's remaining pairs into a `Vec`: predict the ratio from the per-latch cost (about 50 ns) against a 16-byte copy.

## Experiment

Optional. Predict first, then run.

1. **Hold the latch.** Keep the current leaf's read guard inside the iterator. Which test fails first, and why (think about the five-frame pool and about writers)?
2. **A scan during a split.** In a test of your own, start an iterator, then insert enough keys to split the leaf it is on. What does the iterator return next?

## Other designs

- **Position + re-latch per key (ours).** No latch held; per-key cost.
- **Copy the leaf.** An iterator holds a `Vec` of the current leaf's pairs and the next leaf's id; one latch per leaf.
- **Hold the read latch.** A snapshot of the leaf, blocking writers for as long as the cursor lives.
- **Versioned scan** (MVCC, module 4): a scan at a timestamp sees a consistent view without latches.

## In BusTub

```cpp
class IndexIterator {
 public:
  auto IsEnd() -> bool;
  auto operator*() -> std::pair<const KeyType &, const ValueType &>;
  auto operator++() -> IndexIterator &;
  auto operator==(const IndexIterator &itr) const -> bool;
  auto operator!=(const IndexIterator &itr) const -> bool;
};
```

## The C/C++ way

| C / C++ | Rust |
|---|---|
| `operator*`, `operator++` | the `Iterator` trait's `next` |
| `it != tree.End()` | `!it.is_end()` or `it != tree.end()` |
| a `ReadPageGuard` member held by the iterator | no guard held: re-latch per call |
| `std::pair<const K &, const V &>` | an owned `(K, V)` copied out of the page |

**Port rule:** a C++ iterator class with `*`, `++` and `==` becomes a struct implementing `Iterator` and `PartialEq`.

## Learn more

- [`Iterator`](https://doc.rust-lang.org/std/iter/trait.Iterator.html) · [`PartialEq`](https://doc.rust-lang.org/std/cmp/trait.PartialEq.html)
- CMU 15-445 lecture notes: B+ tree range scans
