A leaf has room for a few hundred pairs, and then it is full. A B+ tree does not make the leaf bigger and does not hang a second leaf beside it at random: it **splits** the leaf in two, keeps both halves in key order with a link from the left one to the right one, and puts a **parent** above them that says which keys live where. That parent is the second page type of the tree, the **internal page**: a list of child page ids with a **separator key** between each pair of neighbours. This stage is the first split and the page it creates. The internal page is made as wide as the tests like, so it never fills up here; what happens when a parent is full is the next stage.

> [!CHECK] A leaf with room for 3 pairs (it splits when it reaches 4) holds 10, 20, 30, and 25 arrives. The leaf splits into 10, 20 and 25, 30. Which key goes into the new parent, which side does it point to, and does that key stay in a leaf too? Afterwards a lookup for 25 and a lookup for 24 start at the root: which child does each follow?
> ||The separator is 25, the first key of the right leaf, **copied up**: it stays in the right leaf (a leaf holds every key of the tree) and the parent holds it as the router "keys below 25 go left, keys from 25 go right". The parent has two children, the old leaf and the new one, and one separator between them. A lookup for 25 compares 25 with the separator, finds it is not below, and follows the right child; 24 is below 25 and follows the left one.||
>
> - Why must the leaf keep its copy of the separator?
> - How many separators does a page of two children need?
> - Who records the new root, and where?

## The task

`insert` now handles a leaf that fills up. With `leaf_max_size = L`:

- A leaf at rest holds **at most `L - 1` pairs**. The insert that makes a leaf reach `L` pairs **splits** it: the lower `ceil(L / 2)` pairs stay, the rest go to a new leaf **to its right**, the left leaf links to the right one, and the right leaf's first key is the separator **copied up** into the parent.
- If the leaf that split was the root, a **new root** (an internal page with the two halves as children) is created and the header page names it: the tree is one level taller. If the leaf had a parent, the separator and the new leaf go into that parent, just after the old leaf. (In this stage's tests the parent always has room.)
- Lookups and inserts descend from the root, following at each internal page the child whose range holds the key, down to a leaf. `depth` counts the levels; `default_internal_max_size()` is as many children as fit in one of your internal pages, and `new` now panics when `internal_max_size` does not fit.
- A refused duplicate changes nothing, and a split is invisible to lookups: every key stays findable.

The tests check after **every insert**, on random keys in random order under one wide root, that the leaves hold exactly the keys, a leaf holds at most `L - 1` pairs and, once there are two leaves, at least `L / 2`, and **every lookup latches exactly `depth() + 1` pages**: the header and one page per level.

## Your freedom

The layout of the internal page (whether it stores a key for its first child, and how many children fit), how the new root is made, how the tree remembers the leaf's parent after a split (keep the path on the way down, or search again), and where the split's bookkeeping lives.

## The Rust toolbox

**Remember the path.** Descend with write guards and push each one on a `Vec<WritePageGuard<'a>>` as you go: after a split, `pop()` gives the parent, or `None` when the leaf was the root. The guards are the stack of ancestors, and dropping the vector releases them all.

**Two pages at once.** The leaf that splits and the new right leaf are two guards, so two views: `Leaf::new(&mut left_guard[..])` and `Leaf::new(&mut right_guard[..])` borrow different guards and can both be mutable.

**Creating a page.** `let id = self.bpm.new_page(); let mut guard = self.bpm.write_page(id);` then format it before using it: `new_page` hands out a page that is not yet a leaf or an internal page.

**`div_ceil` to round up.** `size.div_ceil(2)` is how many pairs the left half keeps.

**Binary search for the child.** On sorted separators, `partition_point(|k| k <= key)` counts the separators at or below the key, which is the index of the child to follow. Whether the first child has a key slot at all is your layout decision; the count must skip it if it does.

## If this is new

- [S3 Vec & slices](/t/s3-vec-slices): `partition_point`, slices of slices.
- [L2 Borrowing](/t/l2-borrowing): two guards, two mutable views.
- [S1 Option & Result](/t/s1-option-result): `pop()` returns an `Option`; `let Some(x) = .. else { .. }`.
- The optional *splitting and promoting* and *routing with separator keys* concepts draw the split and the descent.

## Tests

- A leaf of max size 4 splits when the fourth key arrives into leaves of 2 and 2 under a new root of depth 2; every key is found.
- The header names the new root; further leaf splits that the root absorbs leave the root where it is.
- An internal page of the default size fits a page and one child more does not.
- Ascending, descending and random keys under one wide root keep every rule after every insert; no guard is kept.

## Hints

### The first child's key

An internal page with `n` children needs `n - 1` separators, but it is often stored as `n` key slots with the first one unused, so that slot `i` holds "the smallest key under child `i`". Decide which you do, and make every search skip the unused slot: a binary search that reads it will route some keys wrongly.

### Do it on paper with `L = 4`

Insert 1 to 9 in order and draw the leaves, their links and the root after each key. Which separators does the root hold, and how full are the leaves? Then do it in reverse order.

### A split is not finished until the parent knows

Count the writes a split makes: the old leaf (fewer pairs, a new link), the new leaf (formatted, filled, linked to the old leaf's old neighbour), the parent (one more child) or a new root and the header. Forgetting the link shows up when stage 2c-04 walks the leaves; forgetting the parent shows up as a key that cannot be found.

## Performance

A split writes three pages (two leaves and the parent) and is paid for by the `L / 2` inserts it takes to fill a half-empty leaf again. Sequential inserts leave every leaf half full, since the left half of each split is never written again; random inserts typically settle near 69% full.

**Measure it.** Insert 100 000 sequential keys and then 100 000 random keys into trees with leaf size 100 and a wide root, and compare the number of leaves with `n / 99`. Predict the two ratios first.

## Experiment

Optional. Write your prediction down first, then run.

1. **Move instead of copy.** Take the separator out of the right leaf when you copy it up. Which test notices, and what does a lookup of that key return?
2. **A skewed split.** Keep `L - 1` pairs on the left when the new key is the largest so far. What happens to the leaf count for ascending inserts, and for descending ones?

## Other designs

- **Split on overflow (ours, BusTub's).** A leaf splits when it reaches its maximum.
- **Redistribute first.** Move a pair to a neighbour with room before splitting: fuller leaves, more writes.
- **Bulk loading.** Build the leaves left to right from sorted input and the levels above afterwards (challenge 2c-c1).

## In BusTub

BusTub's rule is that "a leaf node is split when it reaches `max_size`" after an insert; `BPlusTreeInternalPage` is the second page type, and `GetMaxSize` of an internal page is in children.

## The C/C++ way

| C / C++ | Rust |
|---|---|
| `std::vector<page_id_t>` of parents, or a recursive `InsertIntoParent` | a `Vec` of write guards, popped |
| `reinterpret_cast<InternalPage *>` on a fresh page | format the page, then build the view |
| `std::upper_bound` over separators | `partition_point` |

**Port rule:** a parent pointer kept in a C++ node becomes a guard kept on a stack; the borrow checker then makes sure you still hold the page you change.

## Learn more

- [`slice::partition_point`](https://doc.rust-lang.org/std/primitive.slice.html#method.partition_point) · [`u32::div_ceil`](https://doc.rust-lang.org/std/primitive.u32.html#method.div_ceil)
- CMU 15-445 lecture notes on B+ trees: insertion · [B+ tree visualisation (USFCA)](https://www.cs.usfca.edu/~galles/visualization/BPlusTree.html)
