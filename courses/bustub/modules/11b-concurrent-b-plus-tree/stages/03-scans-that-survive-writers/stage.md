The iterator of 2c-04 holds no latch between two calls of `next`: it remembers a leaf and a slot, and that is what lets thousands of open scans coexist with writers. The price is that the position can go stale. An insert before it shifts the pairs one slot right, and the scan reads the key it just returned a second time. A split moves the pairs it has not reached yet to a new leaf, and the slot it remembers now points past the end, so it walks on into the new leaf and returns some of its keys again. A borrow can move a key the scan already returned into the leaf it is about to read. The scan never crashes, and never invents a key, but it can go **backwards**. This stage makes it **re-validate its position** so that it never returns a key twice and never out of order, while still holding no latch between calls.

> [!CHECK] A scan has returned 1, 2, 3, 4 from a leaf of six slots holding 1 to 5, and remembers slot 4. An insert of 6 fills the leaf, which splits: 1, 2, 3 stay, 4, 5, 6 move to a new leaf to the right. What does the scan of 2c-04 return next, and why? Which single fact, remembered by the scan, is enough to notice that something moved?
> ||Slot 4 is now past the end of the left leaf (it holds 3 pairs), so the scan moves on to the next leaf, slot 0, and returns 4 again, then 5 and 6. The scan cannot tell from the leaf and the slot alone that the pairs moved. What it can remember is the **last key it returned**: the next key it returns must be greater. Here 4 is not greater than 4, so the scan knows it is behind and skips it (or looks the position up again from that key), and goes on with 5.||
>
> - Why does a remembered slot go stale?
> - What is true of every key a correct scan returns after 4?
> - Is a scan that misses a key that moved during it still correct?

## The task

The contract of a scan while writers work, single-threaded or not:

- A scan never panics, and never returns a key that was never inserted.
- Every key it returns is **greater than the one before**: no key twice, nothing out of order.
- A key that is in the tree for the whole scan is returned **at most once**. Keys may be missed while writers shift or move pairs under the scan: a scan over a changing tree is not a snapshot.
- Nothing stays latched or pinned between two calls of `next`, as before; `==`, `is_end` and `begin_at` keep their meaning.

The tests open an iterator, change the tree under it from the same thread (an insert that splits its leaf, an insert before its position, a borrow that moves a returned key to the next leaf, removes that shrink and merge its leaf away), and check what the rest of the scan returns; and one test runs scans in a loop while two writers split and merge leaves, checking every scan.

## Your freedom

How the scan re-validates: remember the last key and skip anything not greater, re-seek from the tree by the last key when the position looks stale, copy a leaf's remaining pairs when you reach it, or a mix. Where the comparator lives (the iterator may keep a clone of the tree's), and what the iterator remembers besides.

## The Rust toolbox

**A trait object for the order.** The iterator's type has no comparator parameter. A field `cmp: Box<dyn KeyComparator<K> + 'a>` holds one without changing the type; `begin` can build it from `self.cmp.clone()` (`C: Clone + 'a`), and `cmp.compare(a, b).is_le()` is "not greater".

**`Option<K>` for the last key.** `None` until the first pair is returned. `if let (Some(last), Some(cmp)) = (&self.last, &self.cmp)` tests both at once.

**`continue` in a `loop`.** `next` is already a loop that moves past the end of leaves; a pair that fails the check is one more reason to go round again instead of returning.

**Re-seeking.** If you prefer to look the position up again, the tree's descent with `begin_at(last)` gives a fresh position at the first key not less than the last one; skip it if it is equal. It costs a descent, so do it only when the cheap check fails.

## If this is new

- [L4 Traits & dispatch](/t/l4-traits-dispatch): trait objects, `Box<dyn Trait>`.
- S6 Iterators: an iterator whose `next` skips.
- The optional *range scans and the leaf chain* concept: what a scan may see when the leaves change.

## Tests

- A split under an open scan never makes it repeat a key.
- An insert before its position never makes it repeat a key.
- A borrow that moves a returned key ahead of the scan never makes it return that key again.
- A scan survives its leaf shrinking or merging away; scans that run while writers work return real keys, in order, at most once.

## Hints

### Find the three ways to go backwards

Draw a leaf and a position, then do each of these to it: an insert before the position, a split of the leaf, a borrow by the next leaf from this one. In each drawing, what does the remembered slot point at afterwards, and is that key greater than the last one returned?

### Check once, at the right place

The check belongs where a pair is about to be returned, after every move to another slot or leaf: a pair that passes is returned and becomes the new last key; a pair that fails is skipped. A check only at the start of `next` misses the pairs the walk reaches inside it.

### "At most once" is not "exactly once"

A key may be missed when a writer shifts or moves pairs under the scan, and the contract allows it; never missing one would mean holding latches or re-seeking on every call. The tests check order and uniqueness, never completeness, except for a scan that runs after the writers are done.

## Performance

The check costs one key comparison per returned pair, and the iterator keeps one key: nothing compared with the latch and the copy each `next` already pays. A re-seek costs a descent (`depth()` read latches), so a design that re-seeks on every call is several times slower on a long scan; one that re-seeks only when the check fails pays it only when a writer moved something.

**Measure it.** Scan a tree of a million keys with and without the check, single-threaded, and time it. Then scan while four threads insert and remove in the same range, and count how many pairs the check skipped.

## Experiment

Optional. Write your prediction down first, then run.

1. **Without the check.** Run the threaded scan test in a loop of 50 with the check taken out. How often does a scan return a key twice, and which writes cause it?
2. **Copy the leaf.** Copy a leaf's remaining pairs into a `Vec` when the scan reaches it, and serve `next` from the copy. Which of the three ways to go backwards does that remove, and which does it leave?

## Other designs

- **Check against the last key (ours).** Cheap; allows missing a key that moved.
- **Re-seek by the last key on every call.** Never misses a key that stayed in the tree; costs a descent per pair. Re-seeking only when the check fails is cheaper and misses what the check cannot see (a remove that shifted pairs left).
- **Hold the leaf's read latch** between calls: a stable leaf, at the cost of blocking writers (and eviction) for as long as the cursor lives. BusTub's iterator holds a page guard.
- **B-link high keys:** each leaf stores the highest key it may hold, so a scan can tell that the pairs it wants moved right, and follow the right-link to them.
- **Versioned scans** (module 4): a scan at a timestamp sees a snapshot and never needs any of this.

## In BusTub

BusTub's `IndexIterator` keeps a read guard on its leaf between calls, so a leaf cannot change under it; its tests do not run scans against concurrent writers. This course's iterator holds no latch, which is why it needs the check.

## The C/C++ way

| C / C++ | Rust |
|---|---|
| `KeyComparator comparator_` member | `Box<dyn KeyComparator<K> + 'a>` field |
| `std::optional<KeyType> last_` | `Option<K>` |
| `ReadPageGuard` held by the iterator | no guard held; position checked per call |

**Port rule:** a C++ member of a template parameter type becomes, when the struct's type must not change, a boxed trait object.

## Learn more

- Lehman and Yao, *Efficient locking for concurrent operations on B-trees*, 1981: high keys and right-links
- [PostgreSQL nbtree README](https://github.com/postgres/postgres/blob/master/src/backend/access/nbtree/README): how a scan copes with concurrent page splits
