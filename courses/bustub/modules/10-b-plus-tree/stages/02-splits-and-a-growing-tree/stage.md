A B+ tree stays balanced because it grows **at the top**. When a leaf is full, it splits in two and the parent gets one more child; if the parent is full too, it splits and so on up; if the root splits, a new root is made above the two halves and the tree is one level taller. Every leaf stays at the same depth, which is what makes every lookup cost the same `O(log n)` page reads. This stage is the splitting rules and the bookkeeping that keeps the separators right.

> [!CHECK] A leaf with room for 3 pairs holds the keys 10, 20, 30 and the key 25 arrives. It splits into 10, 20 and 25, 30. Which key goes into the parent as the separator, which side does it belong to, and what is different if the page being split is an internal page rather than a leaf?
> ||The separator is 25, the first key of the right leaf, **copied up**: it stays in the leaf (a leaf holds every key) and also appears in the parent as the router "keys below 25 go left, keys from 25 go right". For an internal page the middle key is **moved up**: an internal page's keys are only routers, so the key that separates the two halves leaves both and lives in the parent only. Copy for leaves, move for internal pages.||
>
> - Why must a leaf keep the separator key but an internal page need not?
> - What does the parent do if it has no room for the new separator?
> - What is created when the root splits, and what goes into the header?

## The task

`insert` now handles a full page. The contract, with `leaf_max_size = L` and `internal_max_size = M`:

- A leaf at rest holds **at most `L - 1` pairs**. The insert that makes a leaf reach `L` pairs **splits** it: the lower `ceil(L / 2)` pairs stay, the rest go to a new leaf **to its right**, the leaf links are updated, and the new leaf's first key is the separator copied up into the parent.
- An internal page holds at most `M` children. When a split adds a child to a full internal page, its `M + 1` children are divided: the lower `ceil((M + 1) / 2)` stay, the rest go to a new internal page, and the key between the two halves moves up.
- The parent insert uses the separator to place the new child just after the old one. If there is no parent (the root split), a **new root** with two children is created and the header page is updated.
- Failed (duplicate) inserts change nothing. Splits are invisible to lookups: every key stays findable.

Observable consequences, which the tests check after **every insert** on random shapes with random key orders: the leaves hold exactly the keys (`leaf_sizes` add up); a leaf holds at most `L - 1` pairs and, once there is more than one leaf, at least `L / 2`; the number of leaves fits the depth for fan-outs between `ceil(M / 2)` and `M` (a tree of depth `h` has between `2 * ceil(M/2)^(h-2)` and `M^(h-1)` leaves); **every lookup latches exactly `depth() + 1` pages** (the header and one page per level), which can only happen if every leaf is at one depth; the depth grows by at most one per insert.

## Your freedom

How you find the parent after a split (remember the path on the way down, or search again), what the new internal and leaf pages contain, where the split bookkeeping lives, and whether you write helpers for the leaf and internal cases.

## The Rust toolbox

**Remember the path.** Descend with write guards and push each onto a `Vec<WritePageGuard<'a>>` as you go; after a split, `pop()` gives the parent (or `None` at the root). The guards are the "stack" of ancestors, and dropping the vector releases them all. This is simpler than a recursive function and avoids holding `&mut` borrows across the recursion.

**Splitting a page's entries.** Collect the entries into a `Vec<(K, PageId)>`, insert the new one with `partition_point`, then split the vector with `entries.split_off(keep)` or `entries[keep..]`: it is easier to reason about than shifting bytes in place, and the copy is small (a page). `Vec::insert(at, x)` places the new entry; `div_ceil` rounds up.

**`partition_point` to find a slot.** `entries.partition_point(|(k, _)| cmp.compare(k, &key).is_le())` is "how many entries are at or below the key": the slot of the child that covers it.

**A loop instead of recursion.** `loop { let Some(parent) = path.pop() else { /* new root */ return }; if parent has room { insert; return } else { split parent; key = middle; continue } }` walks up the tree; every iteration deals with one level.

**Creating a page.** `let id = self.bpm.new_page(); let mut guard = self.bpm.write_page(id);` then format it before using it: `new_page` hands out a zeroed page.

## If this is new

- [S3 Vec & slices](/t/s3-vec-slices): `split_off`, `partition_point`, `insert`.
- [L2 Borrowing](/t/l2-borrowing): the parent and the node are two pages (two guards); do not hold views of both mutably through one variable.
- [S1 Option & Result](/t/s1-option-result): `pop()` returns an `Option`; `let Some(x) = .. else { .. }`.
- The optional *splitting and promoting* concept draws the copy-up and the move-up.
- [D6 Trees & BSTs](/t/d6-trees-bsts): Trees & BSTs: search trees: split, borrow, merge.
- Y5 Testing & verification: Understand it; Build it: a model and a shape checker; threaded tests under a timeout.

## Tests

- A leaf of max size 4 splits when the fourth key arrives into leaves of 2 and 2, under a new root of depth 2.
- The depth rises by at most one per insert and reaches the expected height for 300 keys with tiny pages.
- Ascending and descending key orders keep every rule after every insert.
- A refused duplicate changes nothing; every guard is released after a big build.
- For random shapes (`leaf_max` 2 to 8, `internal_max` 3 to 6) and random key orders, `check_shape` holds after every insert and every key is found.

## Hints

### Do it on paper with `L = 3, M = 3`

Insert 1, 2, 3, 4, 5, 6, 7 and draw the tree after each key. Count levels. When does the root split? Which key is the separator each time?

### Which page is the parent?

After you split a leaf you need the parent's page id, and the parent's write guard. If you released the ancestors on the way down you cannot get them back, so for now keep them all (the next stages let go of the ones that cannot be affected).

### Counting

A leaf that reaches `L` splits, so it never rests at `L`. An internal page may rest at `M` children. Off-by-one here shows up as "leaf holds `L` pairs" or "depth is one too small" in the shape checker.

## Performance

A split reads and writes two pages and possibly the parent: constant work, amortised over the `L / 2` inserts it takes to fill the new leaf again. Sequential inserts leave leaves half full (every split divides evenly and the left half is never touched again); random inserts reach about 69% occupancy.

**Measure it.** Insert 100 000 sequential keys and 100 000 random keys into trees with leaf size 100, and compare the number of leaves with `n / 99`. Predict the ratio of the two.

## Experiment

Optional. Predict first, then run.

1. **A skewed split.** Split sequential inserts unevenly (keep `L - 2` pairs on the left): what is the leaf count now, and what would you change for a workload of sorted inserts? (Real systems do this.)
2. **The wrong separator.** Move the key up from a leaf instead of copying it. Which test finds it, and what does a lookup of that key do?

## Other designs

- **Split on overflow (ours).** A page splits when it would exceed its maximum.
- **Split before descending** (preemptive splitting). Split every full page on the way down so a split never needs to go back up; simpler to latch, splits some pages that did not need it.
- **Redistribute before splitting.** Move a pair to a sibling with room first: higher occupancy, more writes.
- **A bulk-loading build** from sorted input: fill leaves left to right and build the levels above; used for creating indexes.

## In BusTub

BusTub's spec: "a leaf node is split when it reaches `max_size`" and "an internal node is split when it would exceed `max_size`". The separator rules above are the standard ones that its reference tests assume.

## The C/C++ way

| C / C++ | Rust |
|---|---|
| a recursive `InsertIntoParent` | a `loop` over a `Vec` of ancestor guards |
| `std::vector<std::pair<KeyType, page_id_t>>` for the split | `Vec<(K, PageId)>` and `split_off` |
| `std::lower_bound` | `partition_point` |
| raw `page_id_t parent` pointers | the parent's guard on the stack |

**Port rule:** C++ recursion over page ids becomes a loop over guards popped from a stack.

## Learn more

- [`Vec::split_off`](https://doc.rust-lang.org/std/vec/struct.Vec.html#method.split_off) · [`usize::div_ceil`](https://doc.rust-lang.org/std/primitive.usize.html#method.div_ceil)
- CMU 15-445 lecture notes on B+ trees: insertion
