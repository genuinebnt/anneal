The tree of modules 2c and 2d is correct for many threads already, and slow: every insert and every remove write-latches the header page first and keeps it, with every page on its path, until it is done, so writers run one at a time. The way to share the tree without that is **latch crabbing**: walk down holding a page only until the child is latched, like a crab moving one claw at a time. A reader can let go of the parent as soon as it holds the child; your lookups already do. A writer must keep the parent of any child that **may split or underflow**, because a split or a merge changes the parent. But a child that is **safe** (it has room for one more pair, or can lose one and stay at its minimum) cannot change anything above it, so every ancestor above it, and the header, can be released at once. This module shares the tree between threads, in three steps: this stage makes writers let go early, the next one gives most writes a path that never write-latches above the leaf, and the third makes scans safe against writers.

> [!CHECK] Two writers insert into different leaves under the same parent. The parent is not full. With latch crabbing, how long do they wait for each other? What changes if the first writer's leaf is full, and what does the answer say about why a *safe* page lets you release its ancestors?
> ||Each writer write-latches the header, then the parent. The parent is not full, so it is safe for an insert: the writer releases the header at once. Then it latches its leaf; a leaf with room for one more pair is safe too, so it releases the parent. The second writer waits for the parent only while the first goes from the parent to its leaf, not for the whole insert. If the first writer's leaf is full, the leaf is not safe: it may split and add a child to the parent, so the writer keeps the parent until it is done, and the second writer waits at the parent for the whole insert. (The header was still released, because the parent had room and the split cannot climb past it.) Safe means "this operation cannot change any page above this one", which is exactly when nothing above it needs to stay latched.||
>
> - What does "safe" mean for an insert? For a remove?
> - Why must the header page be held until a safe page is found?
> - What goes wrong if a writer releases a parent before it knows the child is safe?

## The task

Behaviour does not change; what a writer holds, and for how long, does. With and without tombstones:

- A writer write-latches the header, then each page on the path to the leaf, top down, each one before letting go of anything.
- A page is **safe for an insert** if adding one pair, or one child, cannot split it: a leaf with `size + 1 < max_size`, an internal page with `size < max_size`.
- A page is **safe for a remove** if taking one pair or child away cannot make it short or change the root: a page that is not the root with more than its minimum; a root leaf with more than one pair; a root internal page with more than two children.
- When the writer reaches a safe page, it releases the header and every page above that one, and keeps going down.
- Readers keep reading the way they do: read latches top down, the child before letting go of the parent.

The tests watch the header page from outside. A test read-latches every leaf of a two-level tree whose root has room, starts an insert (or a remove) on another thread, and waits until the writer is stuck at its leaf; then it asks the tree whether it is empty, which needs only the header's read latch. With crabbing the writer let go of the header at the root and the question is answered; without it the question waits for the writer, and the test fails after three seconds. Threaded tests check that crabbing loses nothing: four threads inserting disjoint ranges, and three writers with two readers of stable keys, each against its own model, all under a timeout.

> [!ASIDE] The steps, if you would rather not work them out
> - Give the write descent a test for "safe" that it can ask about each page: one for insert, one for remove (which also needs to know whether the page is the root).
> - Right after latching a page on the way down, ask; if it is safe, drop the header's guard and every guard you kept above this page.
> - Keep pushing the page's guard on the path, as before.
> - A split or a merge then finds on the path exactly the ancestors it may change: every page above the last safe one is gone, and so is the header unless the root may change.

## Your freedom

How you record what you hold (a `Vec` of guards and an `Option` for the header, an enum, a struct of your own), how you decide that a page is safe, and whether one descent serves both insert and remove. The order in which latches are taken (header, then top to bottom; among siblings, left before right) must be the same everywhere.

## The Rust toolbox

**A context that owns the guards.** `struct Context<'a> { header: Option<WritePageGuard<'a>>, path: Vec<WritePageGuard<'a>> }`: setting `header = None` and calling `path.clear()` release the latches. The borrow checker then forbids using a guard you have let go.

**A closure for "safe".** A descent that takes `safe: impl Fn(&[u8], bool) -> bool` (the page and whether it is the root) serves insert and remove with one loop: `|page, _| fits_one_more(page)` and `|page, is_root| can_lose_one(page, is_root)`.

**Assignment drops the old value.** `guard = self.bpm.read_page(child);` takes the child's latch first (the right-hand side runs first) and then drops the parent's guard that `guard` held: crabbing in one line.

**Holding a page from a test.** `bpm.read_page(id)` through the plain pool gives a guard the test can keep in a `Vec`: a writer that wants that page waits until the test drops it. The tests in this module use it to stop a writer in the middle and look at what it holds.

**Watchdogs.** A test that could deadlock runs its work on a thread and waits with `recv_timeout`; a hang becomes a failure with a message. Use the same pattern in your own tests.

## If this is new

- [C1 Threads & shared state](/t/c1-threads-shared-state), and [L1 Ownership & moves](/t/l1-ownership-moves): guards are values you move and drop.
- L6 Closures & functional Rust: `impl Fn` parameters.
- The optional *latch crabbing and safe nodes*, *reader-writer latches* and *deadlock and lock ordering* concepts.

## Tests

- An insert waiting at a leaf, below a root with room, does not hold the header.
- A remove waiting at a leaf, below a root with more than two children, does not hold the header, with and without tombstones.
- Four threads inserting disjoint ranges lose nothing and leave a valid tree.
- Writers and readers together agree with their models; no page is left pinned.

## Hints

### Write "safe" as two tables

For an insert and for a remove, write the condition for a leaf and for an internal page, root and not root: eight cells. The off-by-ones live in the leaf rows: a leaf splits when it *reaches* its maximum, so a leaf with `max_size - 1` pairs is not safe for an insert.

### The header page is an ancestor

The root may change on a split that climbs to the top or a merge that empties it, so the header's latch is part of what a writer holds above the root. Releasing "everything above a safe page" includes it; forgetting it is exactly what the first two tests catch.

### A threaded test hangs

Print which thread takes which latch. Two threads taking the same two latches in opposite orders is the whole story of tree deadlocks; the order "header, top to bottom, left to right" removes it, and a rebalance that latches a sibling after the node it is fixing is the place to check first.

## Performance

With crabbing, a writer holds the top of the tree only while it is above its first safe page, and most leaves are safe: in a tree of leaves around 70% full, an insert finds its leaf safe nearly every time. Writers on different leaves still serialise on the root's write latch for a moment each; the next stage removes that.

**Measure it.** Inserts per second with 1, 2, 4 and 8 threads on random keys, before and after this stage (keep the old version in a branch). Predict the scaling first, and explain why it is not linear.

## Experiment

Optional. Write your prediction down first, then run.

1. **Release too early.** Release the parent before the child is latched. Which test notices, and how often? Run it in a loop of 50.
2. **Never release.** Keep every guard, as before this stage. How much slower are the threaded tests, and which of this stage's tests fail?

## Other designs

- **Crabbing with safe pages (ours, BusTub's).** Simple and correct; writers still pass through the root's write latch.
- **A single tree-wide latch.** Correct and simple; no scaling.
- **B-link trees** (Lehman and Yao): a right-link and a high key on every page let readers and writers recover from a concurrent split without holding the parent; the basis of PostgreSQL's nbtree.
- **Latch-free trees** (the Bw-tree): delta records and compare-and-swap; very different code.

## In BusTub

BusTub's `Context` holds the header page guard and a `write_set` of write guards; its project asks for crabbing with "safe" pages exactly as here, and its concurrent tests (this module's boss) run inserts and removes from several threads.

## The C/C++ way

| C / C++ | Rust |
|---|---|
| `Context` with a `std::deque<WritePageGuard>` | a struct with a `Vec<WritePageGuard>` |
| `ctx.write_set_.clear()` | `path.clear()` |
| `ctx.header_page_ = std::nullopt` | `header = None` |
| `guard.Drop()` on the parent | reassign the variable, or `drop(guard)` |

**Port rule:** an explicit `Drop()` of a guard becomes ordinary scoping, reassignment or `drop(..)`.

## Learn more

- Bayer and Schkolnick, *Concurrency of operations on B-trees*, 1977 · Graefe, *A survey of B-tree locking techniques*, 2010
- [`RwLock`](https://doc.rust-lang.org/std/sync/struct.RwLock.html) · [`thread::scope`](https://doc.rust-lang.org/std/thread/fn.scope.html)
