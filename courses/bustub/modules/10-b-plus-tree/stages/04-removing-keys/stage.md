Deleting from a B+ tree is insertion's mirror image and the harder half. A leaf that loses a pair may fall below its **minimum size**; if so it first tries to **borrow** one pair from a sibling that has one to spare; if neither sibling can spare, it **merges** with a sibling and the parent loses a child, which may make the parent underflow in turn. Merges can reach the root; a root with a single child is replaced by that child and the tree is one level shorter. After every delete the tree must satisfy exactly the rules it satisfied after every insert.

> [!CHECK] A leaf with a minimum size of 2 holds 2 pairs and loses one. Its left sibling has 2 pairs (the minimum) and its right sibling 3. In what order do you try the options, which keys change in the parent, and which page is freed if you have to merge instead?
> ||Borrow from a sibling that has more than the minimum: here the right sibling (3 > 2). Its first pair moves to the end of the underflowing leaf and the parent's separator for the right sibling becomes the right sibling's new first key. The left sibling has no spare, so it is not an option. If neither sibling had a spare you would merge the leaf into a sibling (the left one by convention, else the right), delete the separator and the page that was merged away from the parent, and free that page in the buffer pool.||
>
> - Why does borrowing change one separator and merging remove one?
> - What if the sibling is under a different parent?
> - When is the root allowed to be small?

## The task

`remove(&key)`: nothing happens if the key is absent (no result is returned).

- Remove the pair from its leaf.
- **Minimum sizes:** a leaf that is not the root holds at least `leaf_max_size / 2` pairs, an internal page that is not the root at least `ceil(internal_max_size / 2)` children. The root is exempt: a root leaf may hold as few as one pair, a root internal page as few as two children.
- A page below its minimum **borrows** a pair (or child) from its **left** sibling if that one has more than its minimum, else from its **right** sibling; else it **merges** with a sibling (the left if there is one): all entries go into one page, the other is freed (`bpm.delete_page`, after its guard is dropped), and the parent loses the separator. For leaves the leaf links are repaired; for internal pages the parent's separator comes down between the halves.
- If a parent is now below its minimum, repeat for it. If the **root** is an internal page with one child left, that child becomes the root (header page updated, the old root freed). If the root leaf becomes empty, the tree is empty again: the header names no root and the leaf is freed.

The tests check after **every operation**, on random shapes with random operation sequences, that the leaves hold exactly the live keys, leaf sizes are within `[L/2, L-1]` once there is more than one leaf, the depth fits the leaf count, every lookup latches `depth() + 1` pages, and a scan equals the sorted model. Specific tests: removing a missing key (from an empty tree too) changes nothing; the last key empties the tree and it can be used again; ascending, descending and scattered removal of 120 keys; the tree gets shorter; **pages no longer in the tree are deleted from the pool**.

## Your freedom

Which sibling you prefer when both could lend, how you repair the parent, whether you borrow one pair or rebalance several, and how you represent "the same page, emptied".

## The Rust toolbox

**Several guards at once.** Rebalancing needs write guards on the parent, the node and a sibling. Take them in a fixed order (parent first, then the siblings left to right) so two threads never wait for each other. `Option<WritePageGuard>` for "a sibling that may not exist".

**Free pages after you let go.** `delete_page` fails while the page is pinned, so collect the ids to free in a `Vec<PageId>` and delete them once the guards are dropped, at the end of the operation.

**Swapping entries between two pages.** You cannot hold `&mut` into two guards through one variable, but two separate `Leaf::new(&mut a[..])` and `Leaf::new(&mut b[..])` over two guards are fine. Copy the pair out of one (`entry_at`), remove it, insert it into the other.

**Moving a separator.** In the parent, `set_key_at(i, &key)` after a borrow, `remove_at(i)` after a merge; the exact index depends on whether the sibling is on the left or the right, which is the commonest bug: write both cases as comments before coding.

**`loop` with `continue` for the climb.** After a merge, "the parent is now the node that may be short" is the same situation one level up.

**`Vec::pop` on the ancestors.** The path you pushed on the way down gives the parent; popping it, rebalancing and putting it back as the node in the next round mirrors the insert loop.

## If this is new

- [L2 Borrowing](/t/l2-borrowing): why three guards need three variables; scoped blocks.
- [S1 Option & Result](/t/s1-option-result): `Option` for a missing sibling.
- The optional *underflow, borrow and merge* concept has the diagrams.
- [D6 Trees & BSTs](/t/d6-trees-bsts): Trees & BSTs: search trees: split, borrow, merge.
- [S3 Vec & slices](/t/s3-vec-slices): Understand it: `split_off`, `partition_point`, `insert`, `remove`.
- Y5 Testing & verification: Understand it; Build it: a model and a shape checker; threaded tests under a timeout.

## Tests

- Removing an absent key, or from an empty tree, changes nothing.
- The last key empties the tree (root invalid, depth 0) and it works again.
- Ascending, descending and scattered removals keep every rule down to empty.
- After removing most keys the tree is a single root leaf again.
- Pages that left the tree are deleted from the pool.
- Random inserts and removes on random shapes: after every operation the rules hold and the scan equals the model.

## Hints

### The two siblings

For a node at slot `i` of its parent: the left sibling is slot `i - 1`, the right `i + 1`, if they exist. The separator between node and left sibling is at slot `i`; between node and right sibling at slot `i + 1` (if keys are stored with their right child). Draw it for `i = 0`, a middle slot, and the last.

### Merge direction

If you always merge into the left page, a node with no left sibling merges its right sibling into itself instead: the page that disappears is then the *sibling*. Which page does the parent lose a child for in each case?

### Why did my tree get too tall?

If a merge removes a separator but the parent falls below its minimum and you never repair it, the tree stays taller than needed; the depth bound in the shape checker catches that.

## Performance

A delete without underflow costs one leaf write; a borrow three pages; a merge three plus a page free. Merges propagate rarely (once per `L / 2` deletes at the leaf level, once per `M / 2` of those one level up).

**Measure it.** Build a tree of 100 000 keys with leaf size 100 and delete them in random order; count merges and borrows. Predict which is more common when the tree is half full.

## Experiment

Optional. Predict first, then run.

1. **Merge, never borrow.** Skip borrowing. Which tests fail? (The shape checker should stay quiet; what does the tree cost?)
2. **Lazy delete.** Delete from the leaf and rebalance only when it is empty. Which rule breaks, and what does the tree look like after a thousand deletes? This is the idea tombstones take further in module 2d.

## Other designs

- **Borrow, then merge (ours).** The textbook rule with the minimum sizes above.
- **Lazy deletion.** Leaves may shrink to empty before they are merged or freed; used by some production trees (PostgreSQL's nbtree removes empty pages lazily).
- **Rebalance several pairs at once** between siblings to reach an even split.
- **Never merge**; rebuild periodically.

## In BusTub

BusTub's `Remove` follows the same rules (minimum sizes, left sibling first); its optimistic latching and tombstones (module 2d) are extensions of it.

## The C/C++ way

| C / C++ | Rust |
|---|---|
| `bpm_->DeletePage(page_id)` inside the merge | collect ids, delete after the guards drop |
| a recursive `CoalesceOrRedistribute` | a loop that climbs while the parent is short |
| `GetMinSize()` | `leaf_max_size / 2` and `internal_max_size.div_ceil(2)` |

**Port rule:** recursion up the tree becomes a loop; deleting a page while it is latched becomes a deferred delete.

## Learn more

- CMU 15-445 lecture notes: B+ tree deletion · [`Vec::remove`](https://doc.rust-lang.org/std/vec/struct.Vec.html#method.remove)
