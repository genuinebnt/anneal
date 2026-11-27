Deleting from a B+ tree is insertion's mirror image and the harder half. A leaf that loses a pair may fall below its **minimum size**, and a tree of half-empty leaves is a tree that wastes pages and reads. So a short leaf first tries to **borrow** a pair from a sibling that has one to spare; if neither sibling can spare one, it **merges** with a sibling into one page, and the parent loses a child. When the parent is the root and only one child is left, that child becomes the root and the tree is one level shorter. This stage is all of that at the bottom level, under a root wide enough that the internal pages never run short; the next stage takes the repair up the tree.

> [!CHECK] A leaf with a minimum size of 2 holds 2 pairs and loses one. Its left sibling has 2 pairs (the minimum) and its right sibling 3. In what order do you try the options, which key changes in the parent, and which page is freed if you have to merge instead?
> ||Borrow from a sibling that has more than its minimum: here the right one (3 > 2). Its first pair moves to the end of the short leaf, and the parent's separator for the right sibling becomes the right sibling's new first key. The left sibling has no spare, so it is not an option. If neither sibling had a spare, the short leaf and a sibling would merge into one page (into the left one by convention): the parent loses the separator and the child that went away, the leaf links skip the page, and that page is deleted from the pool once nobody holds it.||
>
> - Why does borrowing change one separator and merging remove one?
> - Why is the root allowed to be small?
> - When does the tree get shorter?

## The task

`remove(&key)`: nothing happens if the key is absent, including in an empty tree.

- Remove the pair from its leaf.
- **Minimum sizes.** A leaf that is not the root holds at least `leaf_max_size / 2` pairs. The root is exempt: a root leaf may hold a single pair, and a root internal page may hold two children.
- A leaf below its minimum **borrows** a pair from its **left** sibling if that one has more than its minimum, else from its **right** sibling; the parent's separator between them changes. If neither can spare a pair, the leaf **merges** with a sibling (the left one if there is one): all pairs go into one page, the leaf links skip the other page, the parent loses that child and its separator, and the page is deleted from the pool after its guard is dropped.
- If the parent is the root and has one child left, that child becomes the root (the header names it) and the old root is deleted. If the root leaf becomes empty, the tree is empty again: the header names no root, and the leaf is deleted.
- In this stage's tests the root is wide enough that no internal page other than the root is ever short.

The tests check after **every operation**, for random leaf sizes and random inserts and removes, that the leaves hold exactly the live keys, every leaf of a tree with more than one leaf holds between `L / 2` and `L - 1` pairs, every lookup latches `depth() + 1` pages, and a scan equals the sorted model.

> [!ASIDE] The steps, if you would rather not work them out
> - Descend to the leaf keeping the path, as an insert does, and remove the pair. Nothing removed: done.
> - The leaf is the root: if it is empty, name no root in the header and delete the page. Done either way.
> - The leaf is at or above its minimum: done.
> - Find the leaf's slot `i` in its parent. Its left sibling is slot `i - 1`, its right one `i + 1`, if they exist; latch the ones you need.
> - Left sibling above its minimum: move its last pair to the front of the leaf and set the parent's key at `i` to the moved key. Else right sibling above its minimum: move its first pair to the end of the leaf and set the parent's key at `i + 1` to the right sibling's new first key.
> - Else merge: append the right page of the pair (leaf and left sibling, or leaf and right sibling) to the left page, copy the right page's next-leaf link, remove the right page's slot from the parent, and remember the right page for deleting.
> - If the parent is the root and has one child left, name that child the root and remember the old root for deleting. Drop every guard, then delete the pages you remembered.

## Your freedom

Which sibling you prefer when both could lend, whether you borrow one pair or rebalance several, which page survives a merge, and how you keep the list of pages to free.

## The Rust toolbox

**Several guards at once.** Rebalancing needs write guards on the parent, the leaf and a sibling. Take them in a fixed order, parent first, then the siblings. With the parent write-latched no other writer can reach its children, so the order among the siblings cannot deadlock today; a fixed order is still the habit that keeps the code safe when that stops being true in module 2e. `Option<WritePageGuard>` stands for "a sibling that may not exist".

**Free pages after you let go.** `delete_page` refuses a page that is pinned, so collect the ids to free in a `Vec<PageId>` and delete them once every guard is dropped, at the end of the operation. A block `{ .. }` around the work makes the drop point visible.

**Moving pairs between two pages.** You cannot hold two `&mut` views through one variable, but `Leaf::new(&mut a[..])` and `Leaf::new(&mut b[..])` over two different guards are fine. Copy the pair out of one, remove it, insert it into the other.

**Which separator.** For a node at slot `i` of its parent, the separator between it and its left sibling is the key of slot `i`; between it and its right sibling, the key of slot `i + 1` (with keys stored beside their right child). Write both cases as comments before you code: the index is the commonest bug.

## If this is new

- [L2 Borrowing](/t/l2-borrowing): why three guards need three variables; scoped blocks.
- [S1 Option & Result](/t/s1-option-result): `Option` for a sibling that may not exist.
- [D6 Trees & BSTs](/t/d6-trees-bsts): split, borrow, merge.
- The optional *underflow, borrow and merge* concept draws each case.

## Tests

- Removing an absent key, or removing from an empty tree, changes nothing; the last key empties the tree and it can be used again.
- A short leaf borrows from a sibling with a spare pair and merges with one that has none; the tree is then a single leaf again.
- Ascending, descending and scattered removals under one root keep every rule down to an empty tree; pages that left the tree are deleted from the pool.
- Random inserts and removes for random leaf sizes: after every operation the rules hold and the scan equals the model.

## Hints

### The two siblings

Draw a parent with three children and do the borrow and the merge for the leftmost, a middle and the rightmost child. Which sibling exists, which separator changes, and which page goes away in each of the six drawings?

### Merge direction

If you always merge into the left page, a leaf with no left sibling merges its right sibling into itself: the page that disappears is then the *sibling*, and the parent loses the sibling's slot, not the leaf's. Getting this wrong deletes a page the tree still points to; the shape checker then finds a lookup that latched a page that is no longer a leaf.

### The root is the exception, twice

A root leaf may hold one pair, and becomes "no root" at zero. A root internal page may hold two children, and is replaced by its only child at one. Both cases change the header, so both need the header's guard; both are easy to test by removing every key of a small tree.

## Performance

A delete that leaves its leaf at or above the minimum writes one page; a borrow writes three; a merge writes two and frees one. Merges are rare in steady state: after a merge the surviving leaf is close to full, and it takes about `L / 2` deletes to make it short again.

**Measure it.** Build a tree of 100 000 keys with leaf size 100 and delete them in random order, counting borrows and merges. Predict which is more common while the tree is half full, and which when it is nearly empty.

## Experiment

Optional. Write your prediction down first, then run.

1. **Merge, never borrow.** Skip borrowing. Which test fails, and which rule does the shape checker report? (Merge a 1-pair leaf into a 3-pair sibling with a leaf maximum of 4 and count.)
2. **Lazy delete.** Delete from the leaf and rebalance only when it is empty. Which rule breaks, and what does the tree look like after a thousand deletes? Module 2d takes this idea further with tombstones.

## Other designs

- **Borrow, then merge (ours, BusTub's).** The textbook rule with the minimum sizes above.
- **Lazy deletion.** Leaves may shrink to empty before they are merged or freed; PostgreSQL's nbtree removes empty pages lazily.
- **Rebalance several pairs at once** between siblings to reach an even split: fewer later borrows, more writes now.
- **Never merge**; rebuild the index now and then.

## In BusTub

BusTub's `Remove` follows the same rules: minimum sizes from `GetMinSize`, the root exempt, borrow before merge.

## The C/C++ way

| C / C++ | Rust |
|---|---|
| `bpm_->DeletePage(page_id)` inside the merge | collect the ids, delete after the guards drop |
| `CoalesceOrRedistribute` | borrow or merge on the leaf and its sibling |
| `GetMinSize()` | `leaf_max_size / 2` |

**Port rule:** deleting a page while it is latched becomes a deferred delete once the guards are gone.

## Learn more

- CMU 15-445 lecture notes: B+ tree deletion · [`Vec::remove`](https://doc.rust-lang.org/std/vec/struct.Vec.html#method.remove)
- [B+ tree visualisation (USFCA)](https://www.cs.usfca.edu/~galles/visualization/BPlusTree.html): delete, borrow and merge step by step
