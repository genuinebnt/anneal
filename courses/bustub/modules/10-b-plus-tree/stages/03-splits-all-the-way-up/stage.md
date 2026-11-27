A root of a few hundred children fills up too. When a leaf splits under a full parent, the parent has no slot for the new child, so the parent splits as well, and its own parent gets one more child, and so on up the path the insert came down. If the split reaches the root, a new root is made above the two halves and the tree is one level taller. That is the whole reason a B+ tree stays balanced: it never grows at the bottom, only at the top, so every leaf stays at the same depth and every lookup costs the same number of page reads. This stage is the internal split and the climb.

> [!CHECK] An internal page with room for 4 children holds the children A, B, C, D with separators 20, 40, 60 between them. Child C splits, and its new right sibling C' starts at 50. The page now needs 5 children. How do you divide them over two pages, which separator goes up to the parent, and why does that key not stay in either half, when the leaf split in 2c-02 kept its separator?
> ||The five children in order are A, B, C, C', D with the separators 20, 40, 50, 60 between them. Keep the lower `ceil(5 / 2) = 3` children (A, B, C with 20 and 40) on the left page and move C' and D (with 60 between them) to a new right page. The separator between the halves, 50, **moves up** to the parent: it is the lower bound of the right page. An internal page's keys are only routers, and a router between C and C' now lives one level higher, so keeping it in either half would put a key with no child beside it. A leaf keeps its separator because a leaf must hold every key of the tree; an internal page holds none.||
>
> - How many separators do five children need, and how many do two pages of 3 and 2 children need?
> - Which page does the parent learn about, and with which key?
> - When does the tree get taller?

## The task

`insert` now handles a full parent. With `internal_max_size = M`:

- An internal page holds at most `M` children. When a split adds a child to a page that already has `M`, its `M + 1` children are divided: the lower `ceil((M + 1) / 2)` stay, the rest go to a new internal page to its right, and the separator between the two halves **moves up**: it is in neither half.
- The parent of the page that split gets the new page and that separator, and may split in turn. A split that reaches the root makes a new root with two children, and the header names it.
- Lookups, `depth` and the descent work for any number of levels; every leaf is at the same depth.

The tests check after **every insert**, on random shapes (leaf size 2 to 8, internal size 3 to 6) with random key orders, that the leaves hold exactly the keys, every leaf is between `L / 2` and `L - 1` pairs, the number of leaves fits the depth for fan-outs between `ceil(M / 2)` and `M` (a tree of depth `h` has between `2 * ceil(M/2)^(h-2)` and `M^(h-1)` leaves), every lookup latches `depth() + 1` pages, and the depth grows by at most one level per insert.

> [!ASIDE] The steps, if you would rather not work them out
> - After a leaf split you hold three things to give the level above: the left page, the separator and the new right page.
> - Pop the parent from the path. If there is none, make a new root with the two pages as children and name it in the header; done.
> - If the parent has room, add the separator and the right page just after the left page; done.
> - Otherwise list the parent's children with the new one in place, keep the lower half, move the upper half to a new internal page, and take the key between the halves out as the separator. The parent and the new page are now the left page, separator and right page of the level above: repeat.

## Your freedom

How the climb is written (a loop over the guards you kept, or recursion), how an internal page is divided (in place, or through a temporary list), and whether the leaf split and the internal split share code.

## The Rust toolbox

**What one level of the climb carries.** `(left: PageId, separator: K, right: PageId)`: the same three things after a leaf split and after an internal split. A `loop` that rebinds them at the end of each round is the climb.

**A temporary `Vec` for the split.** Copy the page's `(key, child)` entries into a `Vec`, `insert` the new one at its place, then write `entries[..keep]` back and `entries[keep..]` into the new page. It is a page's worth of copying, and much easier to get right than shifting bytes in place. `entries[keep].0` is the separator that moves up.

**`partition_point` for the place.** Over the separators (not the first child's slot), `1 + entries[1..].partition_point(|(k, _)| k <= key)` is where the new child goes when slot 0 has no real key.

**Assigning to `mut` parameters.** A function that takes `mut left: PageId, mut key: K, mut right: PageId` can rebind them like locals at the end of each round, so the loop needs no extra variables.

## If this is new

- [S3 Vec & slices](/t/s3-vec-slices): `insert`, `split_off`, ranges of a slice.
- [D6 Trees & BSTs](/t/d6-trees-bsts): search trees, split and balance.
- Y5 Testing & verification: a model and a shape checker, run after every operation.

## Tests

- The depth rises by at most one per insert, and 300 keys with tiny pages make a tree of at least 5 levels.
- Ascending and descending key orders keep every rule after every insert while internal pages split.
- A refused duplicate in a deep tree changes nothing.
- For random shapes and random key orders, every rule holds after every insert and every key is found.

## Hints

### Do it on paper with `L = 3, M = 3`

Insert 1 to 10 in order and draw the tree after each key. When does the first internal page split, which key leaves it, and when does the root split? The tests' tiny pages split on almost every insert, so a paper trace finds an off-by-one faster than a debugger.

### Which key is the separator?

After you insert the new child into the list, the separator that moves up is the key of the first child of the right half. If your layout stores a key in every slot, that key is now in the right page's slot 0, where nobody reads it: that is what "moves up" means in bytes.

### Counting is where it breaks

A leaf splits when it reaches `L`, so a leaf never rests at `L`; an internal page splits when it would exceed `M`, so it may rest at `M`. Mixing the two shows up in the shape checker as "a leaf holds `L` pairs" or as a tree one level too short or too tall for its leaves.

## Performance

A split costs a constant number of page writes per level, and the climb stops at the first parent with room. Since a parent fills only after about `M / 2` of its children split, the climb reaches the next level on roughly one leaf split in `M / 2`; nearly every insert touches one leaf and nothing else.

**Measure it.** Count the page writes per insert for 100 000 random keys with leaf and internal sizes of 100. Predict the average from the split rates above, then measure it with `bpm.get_writes()`.

## Experiment

Optional. Write your prediction down first, then run.

1. **Keep the separator.** Leave the moved-up key in the right internal page as a real key. Which rule does the shape checker report first, and on which insert?
2. **Split before you descend.** Split every full internal page on the way down instead of on the way up. Does the tree end up taller or shorter for 10 000 ascending keys, and why might a concurrent tree want this?

## Other designs

- **Split on the way up (ours, BusTub's).** Only pages that overflow split.
- **Preemptive splitting.** Split any full page on the way down, so a split never climbs: simpler to latch, splits some pages that did not need it.
- **Prefix and suffix truncation.** A separator only has to divide the two halves, so the shortest key that does can be used instead of the whole first key: smaller internal pages, more children per page.

## In BusTub

BusTub's rule is that "an internal node is split when it would exceed `max_size`" after an insert, and that the root may change on a split, which is why the header page exists.

## The C/C++ way

| C / C++ | Rust |
|---|---|
| a recursive `InsertIntoParent` | a `loop` over a `Vec` of ancestor guards |
| `std::vector<std::pair<KeyType, page_id_t>>` for the split | `Vec<(K, PageId)>` and slices of it |
| `std::lower_bound` / `std::upper_bound` | `partition_point` |

**Port rule:** C++ recursion up the tree becomes a loop over guards popped from a stack.

## Learn more

- [`Vec::insert`](https://doc.rust-lang.org/std/vec/struct.Vec.html#method.insert) · [`Vec::split_off`](https://doc.rust-lang.org/std/vec/struct.Vec.html#method.split_off)
- Comer, *The Ubiquitous B-Tree*, 1979 · CMU 15-445 lecture notes on B+ trees: insertion
