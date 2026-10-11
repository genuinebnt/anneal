**Where this fits.** The end of the plain B+ tree: a long random run against a `BTreeSet`, then BusTub's own insert, delete, scale and concurrent tests, ported.

> [!CHECK] BusTub's `OptimisticInsertTest` finds a leaf with room, inserts into it and asserts that **exactly one** write latch was taken. Which of your design decisions could make that test fail although your tree is correct? Name two.
> ||Taking a write latch on the header or on the parent "just in case" (for instance write-latching down the whole path before checking whether the leaf is safe) makes it 2 or more; counting a failed optimistic attempt followed by the pessimistic retry as two writes when the leaf was in fact safe does too. The test exercises the *fast path*: a leaf that cannot split must be inserted into holding only its own write latch.||
>
> - Which latches does the fast path take?
> - What does the test count?
> - What does it not check?

## The task

Nothing new to design.

- **A long run.** Four tree shapes (leaf and internal sizes (2,3), (3,4), (5,5), (8,3)); 6 000 random inserts, removes and lookups each against a `BTreeSet`; the shape checker every 500 steps and at the end; no page left pinned.
- **BusTub's tests.** `b_plus_tree_insert_test.rs` (basic insert, optimistic insert, insert tests with and without the iterator), `b_plus_tree_delete_test.rs` (delete, optimistic delete, sequential edge mix), `b_plus_tree_sequential_scale_test.rs` (5 000 keys in random order) and `b_plus_tree_concurrent_test.rs` (inserts, deletes and mixes over several threads). The C++ tests read leaf pages directly; here they use the tree's observers.

## Your freedom

The same as before. A failure here may be a latching bug that the smaller tests never provoked.

## The Rust toolbox

**Run a concurrency test many times.** `for i in $(seq 50); do cargo test --test b_plus_tree_concurrent_test || break; done`.

**Release mode for the scale test.** `cargo test --release --test b_plus_tree_sequential_scale_test`.

**Reading a shape failure.** The checker names the rule: "a leaf holds N pairs, fewer than the minimum", "a lookup latched K pages". Reproduce on a small tree of your own by printing `leaf_sizes()` and `depth()` after each step.

## If this is new

- Everything is in the earlier stages of this module.

## Tests

- Long runs on four tree shapes agree with a model.
- BusTub's insert, delete, sequential-scale and concurrent tests.

## Hints

### The optimistic tests fail

Latch counts: print `tree.bpm.get_reads()` and `get_writes()` around one insert. Two writes means the fast path was not taken: is your "safe" check off by one (`size + 1 < max_size`)?

### A concurrent test fails once in a while

A race. Re-run in a loop, look at which key is missing or duplicated, and ask which latch was released before the page was safe.

## Performance

The scale test inserts 5 000 keys with leaf size 2 and internal size 3: a tree of depth 12 or more. Run it in release mode and compare debug and release; that is the cost of bounds checks and debug assertions in your page code.

## Experiment

Optional. Predict first, then run.

1. **Bigger pages.** Run the scale test with leaf size 100: how much faster, and what does the depth become?
2. **Under eviction.** Run a long test with a pool of only as many frames as the depth plus a few. What does the pool's eviction rate say about your guards?

## Explain it aloud

The tests check that your tree works. An interviewer checks that you know **why** it works, and asks without a keyboard in front of you. Answer each question out loud, in about a minute, before you open the model answer. Record yourself once: the places you stall are the places you do not know yet.

> [!CHECK] A database indexes a table with a B+ tree. Why not a binary search tree, and why not a hash table? When would you pick the hash table?
> ||A search tree's cost is the number of *pages* it touches, because reading a page from disk costs microseconds to milliseconds, orders of magnitude more than comparing two keys already in memory. A binary tree over a hundred million keys is about 27 levels deep, so about 27 page reads; a B+ tree with a few hundred keys per page is 4 levels. The leaves are also linked in key order, so a range scan or `ORDER BY` reads consecutive leaves, which a hash table cannot do at all. A hash table wins when every query is an exact-match lookup: one probe instead of a descent, and no ordering to maintain.
>
> - What is the unit of cost, and why?
> - What does the tree give you that the hash table cannot?
> - What does the hash table give you that the tree cannot?

> [!CHECK] A leaf is full and an insert arrives. Say what changes, in order, and what the separator in the parent is.
> ||Allocate a new leaf, move the upper half of the pairs into it, and link the chain: the new leaf takes the old leaf's next pointer, and the old leaf points to the new one. Then add a separator to the parent: for a leaf split it is a *copy* of the new leaf's first key (the key still lives in the leaf, because the leaves hold all the data). If the parent is full too, it splits the same way, except that its middle key is *moved* up, not copied, because an internal page does not keep it. A split of the root makes a new root and changes the root page id in the header page.
>
> - Which page is allocated, and which pointers change?
> - Copied up or moved up, and why the difference?
> - What happens when the root splits?

> [!CHECK] Two threads insert into different leaves, both leaves split, and both need the same parent. Why can the tree not deadlock, and what single mistake would make it deadlock?
> ||Latches are taken top-down and, among siblings, left to right, so a thread holding a page only ever waits for pages below it or to its right. A cycle needs someone to wait for something above or to the left, and nobody does. The mistake is an operation that latches against the order: an iterator that holds a leaf and then latches its left neighbour while a writer holds the neighbour and wants the leaf. That is why the iterator in this course holds no latch between two pairs.
>
> - What is the ordering rule?
> - Why does an ordering rule rule out a cycle?
> - Name an operation that would break it.

> [!CHECK] While writers insert and delete, a scan returns a key twice, or misses one that was never deleted. Is that a bug? What would it cost to forbid it?
> ||It is the price of the design, not a bug: the iterator holds no latch between pairs, so a split or a delete in the leaf it is standing in can shift keys under it. You can forbid it in two ways. Hold a read latch on the leaf for the whole scan, and every writer to those leaves waits for it. Or give each reader a snapshot of the data as of one point in time, which means keeping old versions of the keys: multi-version concurrency control, which a later module of this course builds. Real systems choose between the two; most choose versions.
>
> - Why does a scan that holds nothing see changes?
> - What does holding the latch cost the writers?
> - What does a snapshot cost the storage?

> [!CHECK] Back-of-the-envelope. 100 million keys of 8 bytes, 4 KiB pages, an internal entry of about 12 bytes (key and page id) and a leaf pair of 16 bytes (key and record id). How many levels, and how many page reads does one point lookup cost if the top two levels are always in the buffer pool?
> ||An internal page holds about 4096 / 12, roughly 340 children; a leaf holds 4096 / 16 = 256 pairs. 100 million pairs fill about 400 000 leaves (about 600 000 at the usual two-thirds fill). The level above has about 600 000 / 230 = 2 600 pages, the one above that about 12, and then the root: 4 levels. With the root and the 12 pages below it cached, a lookup reads 2 pages from disk: one internal page and the leaf. Each extra level multiplies the capacity by a few hundred, so a thousand times more data adds only one or two levels, which is why indexes stay fast as tables grow.
>
> - Fan-out of an internal page, and entries in a leaf?
> - Leaves, then each level above, until one page is left.
> - Which levels are cached, and what is left to read?

> [!CHECK] You have 60 seconds with an interviewer who has never seen your code. Explain latch crabbing and the optimistic fast path.
> ||Readers take the child's latch and only then release the parent's, so a writer cannot split the child away between the two steps. Writers must hold the path while a split could reach it, but a node with room (for an insert) or above its minimum (for a remove) is *safe*: once it is latched, everything above can be released. Writers usually find a safe leaf, so the optimistic path takes shared latches down and one exclusive latch on the leaf, and only if the leaf would split does it release everything and redo the operation holding the whole path. The bet is that structural changes are rare.
>
> - What does a reader latch first, the child or the parent, and why?
> - What makes a node safe?
> - What does the optimistic path assume, and what happens when it is wrong?

## Write it up

A one-page design note is the artefact an interviewer can read before the call and a hiring manager can skim. Write `notes/b-plus-tree.md` in your repo, one page, in your own words:

- **The design.** Leaf and internal sizes you chose and the one decision in your tree you would defend in a review.
- **A measurement.** A small table from this stage's experiment: leaf size, depth and time for the scale test (for example leaf sizes 2, 8 and 100). One sentence on what the numbers say.
- **A bug.** One failure you hit, how you found it (the shape checker, the loop of concurrent runs, a latch count) and what the fix was.
- **A limit.** One thing the tree cannot do, such as a consistent scan during writes, and what you would add to do it.

Then read it aloud in three minutes. If a sentence needs the code open to make sense, rewrite it.

## Other designs

None for this stage. The *Other designs* sections of 2c-01 to 2c-05 list the alternatives to compare with yours.

## In BusTub

These are the tests of Project 2 tasks 1 to 4 (checkpoints 1 and 2); once they pass, the index is ready for module 2d (tombstones) and for the catalog's indexes in Project 3.

## The C/C++ way

| C / C++ | Rust |
|---|---|
| `std::thread` + `LaunchParallelTest` | `thread::scope` |
| `std::shuffle` with a default engine | a fixed-seed Fisher-Yates shuffle |
| `tree.GetRootPageId() == INVALID_PAGE_ID` | `tree.get_root_page_id() == PageId::INVALID` |

**Port rule:** a test that reaches into pages becomes a test that asks the tree through an observer.

## Learn more

- BusTub's [project 2 page](https://15445.courses.cs.cmu.edu/) · [`thread::scope`](https://doc.rust-lang.org/std/thread/fn.scope.html)
