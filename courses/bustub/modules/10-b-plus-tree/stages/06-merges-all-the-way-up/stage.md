A merge at the bottom takes a child away from the parent, and the parent has a minimum size too. When an internal page falls below it, it is repaired the way a leaf is, with one twist: an internal page's keys are routers, so a child that moves between two internal pages takes the **parent's separator** with it, and the sibling's key that it leaves behind goes up into the parent instead. The separator **rotates** through the parent. A merge of two internal pages pulls the separator between them down. And since the parent of a merged page may now be short in its turn, the repair climbs, until a page is no longer short or the root is reached, where a root with one child is replaced by that child. This stage is the repair above the leaves.

> [!CHECK] An internal page N with a minimum of 2 children has one child left, X. Its left sibling L has the children A, B, C with the separators 20 and 40 between them, and the parent's separator between L and N is 50. N borrows from L. Which child moves, which keys end up in N, in L and in the parent, and why can the moved child not simply keep the key it had in L?
> ||The last child of L, C, moves to the front of N. N now has the children C and X, and the key between them is the parent's old separator, 50: every key under X is at least 50, and every key under C is below it. The parent's separator becomes 40, the key that was in L in front of C, because it is now the lower bound of everything in N. L keeps A and B with 20 between them. The key 40 cannot stay in N as C's key: N has no key in front of its first child, and the router between C and X must be 50.||
>
> - Which keys does a merge of two internal pages need that neither page has?
> - When does the repair stop climbing?
> - How many levels can one remove take away?

## The task

`remove` now repairs internal pages too. With `internal_max_size = M`:

- An internal page that is not the root holds at least `ceil(M / 2)` children.
- An internal page below its minimum **borrows** a child from its left sibling if that one has more than its minimum, else from its right sibling: the child moves across, the parent's separator comes down into the page that borrows, and the sibling's key next to the moved child goes up into the parent in its place.
- If neither can spare a child, it **merges** with a sibling: the separator between them comes down from the parent between the two halves, the parent loses that separator and the page that went away (deleted after its guard is dropped), and the parent is checked in turn.
- The repair climbs while pages are short. A root internal page left with one child is replaced by that child, and the tree is one level shorter; it can only shrink at the root, one level at a time.

The tests check after **every operation**, on random shapes (leaf size 2 to 8, internal size 3 to 6), that every rule of the tree holds, including the number of leaves fitting the depth for fan-outs between `ceil(M / 2)` and `M`, which is what an internal page left short breaks.

## Your freedom

Whether leaves and internal pages share the borrow and merge code or have their own, how the climb is written (a loop that turns the parent into the node, or recursion), and which page survives a merge.

## The Rust toolbox

**The same loop one level up.** After a merge the parent is the page that may be short: give its guard the role the leaf had (`node = parent;`) and go round again. The path you pushed on the way down still holds the grandparent, so the next `pop()` is the right page.

**A separator that comes down.** In a merge of internal pages, the right page's first child has no key of its own; the parent's separator is its key in the merged page. Copy it out of the parent before you remove the parent's slot.

**Three keys in a rotation.** Write the borrow as three assignments on paper before you code: the moved child's key in its new page, the parent's new separator, and what the sibling loses. Every one of them reads a key that one of the others overwrites, so the order matters.

**Both arms, always.** If your merge code takes "is this a leaf" as a flag, a `match` or an `if` with both arms written makes the compiler insist you cover both, where a forgotten `else` would silently do nothing.

## If this is new

- [L2 Borrowing](/t/l2-borrowing): moving a guard from one variable to another at the end of a loop round.
- [D6 Trees & BSTs](/t/d6-trees-bsts): rotations in search trees.
- The optional *underflow, borrow and merge* concept draws the internal cases.

## Tests

- Removing 120 keys in ascending, descending and scattered order from a tall tree keeps every rule down to an empty tree.
- The tree gets shorter one level at a time, down to a single root leaf.
- Pages that left a deep tree, internal pages included, are deleted from the pool.
- A sliding window of live keys keeps the tree as shallow as its size allows; random operations on random shapes keep every rule.

## Hints

### Draw the rotation

For an internal page at slot `i` with a left sibling, write the children and keys of the sibling, the page and the parent before and after a borrow. Then do the same with a right sibling. The two are mirror images except for one detail: which slot of the parent changes.

### The pages that disappear

A merge of internal pages frees one page and leaves the parent with one child fewer, which may make it short. Climb with the parent, not with the page you merged into. If the climb reaches the root and the root has one child, the root page is freed too.

### The leaf count does not fit the depth

That message from the shape checker means an internal page with too few children is still in the tree: some level was not repaired. Print the depth and the leaf count after every remove of a small test and find the remove where they stop fitting.

## Performance

The repair climbs a level only when a merge empties a child slot in a parent that was already at its minimum, so most removes stop at the leaf. A remove costs at most a constant number of page writes per level, `O(log n)` in all, and the typical one writes a single leaf.

**Measure it.** Delete 100 000 keys from a tree with leaf and internal sizes of 16, in random order and in ascending order, and count how many removes reach the second level and how many the root. Predict which order climbs more often.

## Experiment

Optional. Write your prediction down first, then run.

1. **Forget the rotation.** Let the moved child keep its key from the sibling and leave the parent's separator alone. Which test fails first, and which lookup goes wrong?
2. **Never shrink the root.** Leave a root with one child in place. Which rule breaks, and what does a tree look like after you remove almost everything?

## Other designs

- **Borrow, then merge, at every level (ours, BusTub's).**
- **Free-at-empty.** Never borrow or merge internal pages; delete a page only when it has no children left. Simpler, lets the tree be taller than its size needs.
- **Merge three into two.** Spread the children of three neighbours over two pages, as B*-trees do: higher occupancy, more pages touched.

## In BusTub

BusTub's `Remove` repairs internal pages with the same minimum (`GetMinSize`, `ceil` of half the maximum) and the root exception, and its delete tests (module 2c's boss) check the result.

## The C/C++ way

| C / C++ | Rust |
|---|---|
| a recursive `CoalesceOrRedistribute` on the parent | a `loop` that turns the parent guard into the node |
| `MoveFirstToEndOf`, `MoveLastToFrontOf` on internal pages (older releases) | your own borrow code, one arm per side |
| `parent->SetKeyAt(index, middle_key)` | the rotation's three assignments |

**Port rule:** recursion up the tree becomes a loop, and the parent pointer becomes the next guard on the path.

## Learn more

- CMU 15-445 lecture notes: B+ tree deletion · Comer, *The Ubiquitous B-Tree*, 1979
- [B+ tree visualisation (USFCA)](https://www.cs.usfca.edu/~galles/visualization/BPlusTree.html): delete with internal merges
