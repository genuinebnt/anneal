So far the tree is correct for one thread. Many threads share it, and the way to share it without a single big lock is **latch crabbing**: walk down holding a page's latch only until the child's latch is taken, like a crab moving one claw at a time. Readers can let go of the parent as soon as the child is latched. Writers must keep the parents of any child that **may split or merge**, because a split changes the parent; but a child that is **safe** (it has room for one more pair, or can lose one without underflowing) cannot change the parent, so every ancestor above it can be released at once. And since most inserts and deletes touch only a leaf with room to spare, an **optimistic** fast path read-latches down and write-latches only the leaf, falling back to the full pessimistic path only when the leaf is not safe.

> [!CHECK] Two writers insert into different leaves under the same parent. The parent is not full. With latch crabbing, do they wait for each other? And if the parent is full? What does the answer say about why a *safe* page lets you release its ancestors?
> ||With a parent that has room, the child pages are safe for neither writer to cause a split above them? Careful: the writers' leaves are the children. If a leaf is safe (it has room), its split cannot happen, so the parent cannot change and its latch is released as soon as the leaf's is taken; two writers then run in parallel on different leaves. If the parent is full and a leaf is not safe, the leaf may split and push a key into the parent, which may split too, so the writer must keep the parent latched, and the two writers serialise on the parent. Safe means "nothing above me can be affected by this operation", which is why the ancestors can be released.||
>
> - What does "safe" mean for an insert? For a remove?
> - Why must the header page be held until a safe page is found?
> - What can go wrong if a writer releases a parent before it knows the child is safe?

## The task

Behaviour does not change; **cost and concurrency do**:

- **Readers** (`get_value`, scans) take read latches down the tree, taking the child's latch before releasing the parent's. A lookup read-latches the header and one page per level: `depth() + 1` latches in total, never a write latch.
- **Writers, pessimistic path.** Write-latch from the header down. A page is **safe for an insert** if adding one pair (or child) cannot split it: a leaf with `size + 1 < max_size`, an internal page with `size < max_size`. It is **safe for a remove** if taking one away cannot make it underflow: a non-root page with more than its minimum; a root leaf with more than one pair; a root internal page with more than two children. At the first safe page, release every ancestor (and the header) and keep going.
- **Optimistic path.** `insert` first read-latches down to the leaf, re-latches the leaf for **writing** while the parent is still held, then releases the parent; if the key is a duplicate, or the leaf stays below `max_size`, the insert is done having write-latched **exactly one page**. `remove` likewise when the leaf stays at or above its minimum (a root leaf: non-empty). Otherwise it falls back to the pessimistic path.

The tests count latches through `tree.bpm`: an insert into a leaf with room write-latches **exactly one** page (and read-latches at least one); a remove from a leaf above its minimum, exactly one; an insert that splits write-latches at least two; lookups and scans write-latch none. Threaded tests, each under a timeout so a deadlock fails instead of hanging: four threads inserting disjoint ranges lose nothing and leave a valid tree; three writers (insert, remove, lookup against private models) with two readers of stable keys; a scan that runs during writers stays sorted and never loses a key that was not touched.

## Your freedom

How you record what you hold (a `Vec` of guards, an enum of "context"), how you decide a page is safe, and whether the fast path shares code with the slow one. The latch order (header, then top to bottom, left to right among siblings) must be consistent.

## The Rust toolbox

**A context that owns the guards.** `struct Context<'a> { header: Option<WritePageGuard<'a>>, path: Vec<WritePageGuard<'a>> }`: dropping the header or clearing the path releases the latches (`ctx.header = None; ctx.path.clear();`). The borrow checker then forbids using a guard you have let go.

**A closure that says what "safe" means.** `descend(key, |page: &[u8]| -> bool { .. })`: pass `impl Fn(&Self, &[u8], bool) -> bool` so one descent routine serves insert and remove.

**Upgrade by re-latching.** Rust's `RwLock` cannot upgrade a read guard to a write guard atomically. The standard trick: keep the **parent** read-latched, drop the leaf's read guard, take its write latch; the parent's latch stops anybody from splitting or merging the leaf in between.

**Assignment drops the old value.** `guard = self.bpm.read_page(child);` takes the child's latch first (the right-hand side runs first) and *then* drops the parent's guard held in `guard`: crabbing in one line.

**Watchdog tests.** The threaded tests run on a thread and wait with a timeout; a deadlock fails with a message. Use the same pattern for your own tests.

## If this is new

- [C1 Threads & shared state](/t/c1-threads-shared-state), and [L1 Ownership & moves](/t/l1-ownership-moves): guards are values you move and drop.
- L6 Closures & functional Rust: `impl Fn` parameters.
- The optional *latch crabbing and safe nodes* and *optimistic latching* concepts draw the protocol.
- Y5 Testing & verification: Understand it; Build it: a model and a shape checker; threaded tests under a timeout.

## Tests

- An insert into a roomy leaf write-latches one page; an insert that splits write-latches at least two.
- A remove from a leaf above its minimum write-latches one page.
- Lookups and scans take no write latch.
- Four threads inserting disjoint ranges; writers and readers together; a scan during writes: all under a timeout, shape checked at the end, no page left pinned.

## Hints

### Count first, then optimise

Run the single-threaded latch-count tests against your pessimistic-only path: they fail with "more than one write latch". That is the measure for the fast path.

### The header page

The root may change, so a writer that might split the root must hold the header's write latch until it knows it will not. Releasing the header is part of releasing the ancestors of a safe page.

### Hang on the threaded test

Print the order in which each thread takes latches. Two threads taking the same pair in opposite orders is the whole story of tree deadlocks: the rule "top to bottom, left to right" removes it.

## Performance

With crabbing and the fast path, almost every operation write-latches one leaf, so writers on different leaves run in parallel and the header and the root are only read-latched. The remaining bottleneck is the root's read latch counter on many cores.

**Measure it.** Inserts per second with 1, 2, 4 and 8 threads on random keys, with and without the fast path (take it out temporarily): predict the scaling.

## Experiment

Optional. Predict first, then run.

1. **Release too early.** Release the parent before the child's latch is held. Which test notices, and how rarely?
2. **Hold the header.** Keep the header write-latched for every insert. How does throughput scale now, and why?

## Other designs

- **Crabbing with the optimistic fast path (ours).**
- **A single tree-wide latch.** Correct and simple; no scaling.
- **B-link trees** (Lehman and Yao): sibling pointers let readers recover from a concurrent split without holding the parent; the basis of PostgreSQL's nbtree.
- **Latch-free trees** (Bw-tree): delta records and compare-and-swap; very different code.

## In BusTub

BusTub's `Context` holds the header guard and a `write_set`; "optimistic" insert and remove are a project-2 extension and checked by `OptimisticInsertTest` and `OptimisticDeleteTest`, which count `GetReads` and `GetWrites` on the traced pool exactly as here.

## The C/C++ way

| C / C++ | Rust |
|---|---|
| `Context` with a `std::deque<WritePageGuard>` | a struct with a `Vec<WritePageGuard>` |
| `ctx.write_set_.clear()` | `ctx.path.clear()` |
| `guard.Drop()` on the parent | assign over the variable or `drop(guard)` |
| `TracedBufferPoolManager::GetReads()` | `tree.bpm.get_reads()` |

**Port rule:** an explicit `Drop()` of a guard becomes ordinary scoping or `drop(..)`.

## Learn more

- Lehman and Yao, *Efficient locking for concurrent operations on B-trees*, 1981 · Graefe, *A survey of B-tree locking techniques*, 2010
- [`RwLock`](https://doc.rust-lang.org/std/sync/struct.RwLock.html) and why it has no upgrade
