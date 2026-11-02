An **observed-remove set** is a set that can be copied to other machines, edited there, and merged back without a coordinator: a CRDT (conflict-free replicated data type). The model: every `add(elem, uid)` creates a pair `(elem, uid)` with a unique id; a `remove(elem)` kills the pairs for `elem` **that this replica has seen**; an element is present while one of its pairs is alive. Merging two replicas is the union of their add pairs and the union of their removed pairs. That is all, and it is enough: union is commutative (order does not matter), associative (grouping does not matter) and idempotent (merging twice changes nothing), so replicas that have heard the same updates in any order are equal. A remove only killed pairs it had seen, so a concurrent add elsewhere survives: **add wins**.

> [!CHECK] Replica A adds `x` (unique id 1) and then removes `x`. Replica B, at the same time, adds `x` (unique id 2). After the two replicas merge, is `x` in the set? Why is this called an *observed*-remove set? Then: A and B merge, B and C merge, A and C merge, in three different orders on three machines: why must they all end up equal?
> ||Yes. A's remove only marked the pair `(x, 1)`, the one it had observed; B's pair `(x, 2)` was never removed, so `x` has a live pair. A remove removes only the adds it has seen. Merge is a union of sets, and union is commutative, associative and idempotent: whatever the order and however often updates arrive, the replicas that have received the same set of updates hold the same two sets of pairs, hence the same elements.||
>
> - What exactly does `remove(x)` mark as removed?
> - What does merge do with the adds and the removed pairs?
> - When is an element in `elements()`?

## The task

In `src/primer/orset.rs` the public API is fixed (`ORSet::new`, `contains`, `add`, `remove`, `merge`, `elements`, `to_string`; `to_string` sorts the elements and prints `{a, b}`, given; the driver `ORSetDriver`, a network of replicas, is given); the inside is yours.

- `add(elem, uid)`: remember the pair `(elem, uid)`.
- `remove(elem)`: mark every pair for `elem` that is **currently here** as removed.
- `contains(elem)`: is there a pair for `elem` that is not removed?
- `merge(&other)`: take in all of `other`'s adds and all of its removals.
- `elements()`: the distinct elements that have at least one live pair, each once.

The tests: exact scenarios (add and remove on one replica; adding twice with two ids needs one remove; adding back after a remove works; removing an absent element is harmless; other elements are not affected; merge brings in adds and removes; add wins over a concurrent remove; merging twice changes nothing; `elements` lists each element once and `to_string` sorts them; merge order does not matter), and three properties: **merge is commutative, associative and idempotent** for replicas that did arbitrary adds and removes independently (and `elements` agrees with `contains`, each element once); **a remove only removes the adds it has seen** (a concurrent remove loses, a later add brings the element back); and **replicas in a network with random adds, removes, saves, loads and lost messages converge after a full sync** (twice, so nothing is in flight) to the same set, which contains every element that was never removed anywhere.

## Your freedom

The representation: two sets of pairs (the intended design), a map from element to a set of live ids, or a 'dot' per add with a version vector; the laws and the add-wins rule are what is tested.

## The Rust toolbox

**`BTreeSet` of pairs.** `BTreeSet<(T, Uid)>` is ordered, deduplicated and cheap to union with `extend`.

**Union as merge.** `self.adds.extend(other.adds.iter().cloned())` is the whole merge for one of the sets; the laws follow from the laws of set union.

**`dedup` on sorted data.** Pairs ordered by element put equal elements next to each other, so `Vec::dedup` after collecting the elements lists each one once.

**Clone to publish.** The driver saves a replica by cloning it; `#[derive(Clone, Debug, Default)]` on `ORSet` is already there, so your fields must be `Clone`.

**Property tests for algebraic laws.** Generate three replicas and check `(a ∪ b) ∪ c = a ∪ (b ∪ c)`: the cheapest way to find an order dependence.

```rust
self.adds.insert((elem.clone(), uid));
let seen: Vec<(T, Uid)> = self.adds.iter().filter(|pair| pair.0 == *elem).cloned().collect();
self.removed.extend(seen);
self.adds.iter().any(|pair| pair.0 == *elem && !self.removed.contains(pair))
```

```rust
self.adds.extend(other.adds.iter().cloned());
let mut out: Vec<T> = self.adds.iter().filter(|p| !self.removed.contains(p)).map(|p| p.0.clone()).collect();
out.dedup();     // the pairs are ordered by element, so equal elements are adjacent
```

## Design notes

**Why remove copies the pairs.** `remove` must record *which* adds it killed, not "elem is gone": a later add has a new id and is not killed. Removing an element that is not (yet) there kills nothing, so a later add stays visible.

**Collect before extending.** Iterating `adds` while inserting into `removed` is fine (different sets), but collecting first keeps the borrow simple.

**Ordered pairs.** `BTreeSet<(T, Uid)>` sorts by element first, so all pairs of one element are adjacent; stage 7's `elements` uses that.

**Why add wins.** Replica A adds `x` with id 0 and removes it (id 0 is in `removed`). Replica B adds `x` with id 1. After the merge `adds = {(x,0), (x,1)}` and `removed = {(x,0)}`: `(x,1)` is alive. A did not see B's add when it removed, so its remove cannot have meant it.

**State-based replication.** Replicas exchange whole states and merge them. That tolerates lost, repeated and reordered messages, which is why the idempotence matters: a replica that loads the same saved copy twice must be unchanged.

**`elements` and duplicates.** Several live pairs of one element appear in the pair order next to each other, so `dedup` collapses them.

## If this is new

- [S4 Maps & sets](/t/s4-maps-sets): `BTreeSet`, union with `extend`.
- S8 The core traits: why `Ord + Clone` bounds, `Clone` for snapshots.
- Y5 Testing & verification: properties for algebraic laws (commutative, associative, idempotent).
- The optional *CRDTs and observed-remove sets* concept.

## Tests

- Add, remove, re-add, absent removes; merge, add-wins, idempotence, listing, order independence.
- Properties: the three laws; observed-remove; convergence of a network.

## Hints

### Remove looks at `adds`, not at `removed`

You can only kill what you have seen: the pairs currently in `adds`.

### `contains` needs both sets

A pair counts only if it is in `adds` and not in `removed`.

### Merge both sets

Merging only the adds would resurrect elements that the other replica removed.

### Don't remove on merge

A merge never deletes anything; removal is recorded by pairs in `removed`.

## Performance

`contains` and `remove` scan the pairs of the whole set here (`O(n)`); a map from element to its pairs would make them `O(pairs of that element)`. The sets only grow (removed pairs are kept), which real systems compact.

**Measure it.** Add and remove the same element 100 000 times under new ids: the state grows by two pairs per round.

## Experiment

Optional. Predict first, then run.

1. **Remove by element.** Make `remove` kill every pair for the element, including ones merged later. Which test shows add-wins is gone?
2. **Forget the removed set in merge.** Which property fails first?

## Other designs

- **Two grow-only sets of pairs (ours):** simplest, grows forever (removed pairs are never forgotten).
- **Dots and a version vector:** metadata proportional to the number of replicas, not the number of adds.
- **Last-writer-wins set:** timestamps decide; a concurrent add and remove lose data.
- **A coordinator** (consensus): always consistent, not available when partitioned.

## In BusTub

`orset.h`: "`/** @brief The observed remove set datatype. */`", `Add(const T &elem, uid_t uid)`: "`uid: unique token associated with the add operation.`", `Remove(const T &elem)`: "`Removes an element from the set if it exists.`".

## The C/C++ way

| C / C++ | Rust |
|---|---|
| `std::set<std::pair<T, uid_t>>` | `BTreeSet<(T, i64)>` |
| `throw NotImplementedException` in the starter | `todo!()` |

**Port rule:** a set of pairs is a `BTreeSet` of tuples; tuples order lexicographically.

## Learn more

- [OR-Set](https://github.com/pfrazee/crdt_notes#or-set) · [Shapiro et al. 2011](https://inria.hal.science/inria-00555588/document)
- [`BTreeSet::extend`](https://doc.rust-lang.org/std/collections/struct.BTreeSet.html) · [Join-semilattices and CRDTs](https://en.wikipedia.org/wiki/Conflict-free_replicated_data_type#State-based_CRDTs)
