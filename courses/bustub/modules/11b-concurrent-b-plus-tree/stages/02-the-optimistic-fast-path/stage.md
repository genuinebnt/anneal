With crabbing, every writer still write-latches the header and the root on its way down, if only for a moment, so all writers queue at the top of the tree. Yet almost every insert lands in a leaf with room, and almost every remove in a leaf above its minimum: the pages above the leaf were never going to change. The **optimistic** path bets on that. It descends with **read** latches, which writers on other leaves can share, latches only the leaf for writing, and does the whole operation there. When the bet is lost (the leaf is full, or would be left short) it lets go of everything and takes the pessimistic path of 2e-01. The bet is won so often that most writes touch the top of the tree with read latches only.

> [!CHECK] A writer has read-latched the parent and the leaf, sees that the leaf has room, and wants the leaf's write latch. Rust's `RwLock` cannot turn a read guard into a write guard. If it drops the leaf's read latch and then takes its write latch, what could another writer do to that leaf in between, and what does the parent's read latch, still held, prevent?
> ||Between the two latches another writer could take the leaf's write latch first and insert into it (the leaf may be full now), or a remove could have merged it away and freed the page, or a split could have moved half of its pairs to a new leaf so that the key now belongs elsewhere. A split, a merge or a borrow all need the parent's **write** latch, and the parent is read-latched by this writer, so none of them can happen: the leaf stays the leaf for this key, in the same place. Another writer can still change the leaf's contents without touching the parent, which is why the writer checks again under the write latch whether the leaf has room, and falls back if not.||
>
> - Which structural changes need the parent's write latch?
> - What may another writer still do to the leaf?
> - What must be checked again under the write latch?

## The task

Behaviour does not change; what a write latches does. With and without tombstones:

- `insert` first descends with **read latches** (the header, then each page, the child before letting go of the parent), re-latches the leaf for **writing** while its parent is still read-latched, then lets go of the parent. If the key is already there (a duplicate, or with tombstones a deleted key that comes back), or the leaf has room for one more pair, the insert is done, having write-latched **exactly one page**. Otherwise it releases the leaf and takes the pessimistic path.
- `remove` likewise: done on the leaf when the key is absent, or when the leaf stays at or above its minimum (a root leaf: keeps at least one pair). With tombstones, a delete that fits in the buffer never changes the leaf's size, so it is done on the leaf too.
- Lookups and scans never take a write latch.

The tests count latches through `tree.bpm`: an insert into a leaf with room write-latches exactly one page and read-latches at least one; a remove from a leaf above its minimum, exactly one; with tombstones a delete that fits and a revival, exactly one each; an insert that splits, at least two. And one test watches the top of the tree: while an insert waits at its leaf (the test holds the leaf), a lookup in another leaf must get through, which it cannot if the waiting insert holds the root's write latch.

## Your freedom

Whether the fast path shares code with the pessimistic one, how you represent "done" and "try again" (an `Option<bool>`, an enum, a `bool`), and whether a full leaf whose dead pairs could be reclaimed (2d-03) counts as having room on the fast path.

## The Rust toolbox

**Upgrade by re-latching.** `RwLock` has no upgrade. The standard trick: keep the **parent** read-latched, drop the leaf's read guard, take its write guard, then drop the parent. The parent's latch is what keeps the leaf from being split, merged or moved in between.

**`Option<bool>` for three answers.** `Some(true)` (inserted), `Some(false)` (a duplicate), `None` (not here: take the slow path). At the call site, `if let Some(done) = answer { return done; }` and the slow path below it read in the order they happen.

**Re-check under the write latch.** The decision "the leaf has room" made under the read latch is stale once the read latch is gone; decide again after taking the write latch.

**Counting latches in your own tests.** `let w = tree.bpm.get_writes(); tree.insert(..); assert_eq!(tree.bpm.get_writes() - w, 1);` is how BusTub's `OptimisticInsertTest` checks the fast path, and how you find out which path an operation took.

## If this is new

- [C1 Threads & shared state](/t/c1-threads-shared-state): reader-writer locks and what they promise.
- [S1 Option & Result](/t/s1-option-result): `Option` as "an answer or not yet".
- The optional *optimistic latching* and *reader-writer latches* concepts.

## Tests

- An insert into a leaf with room, and a remove from a leaf above its minimum, write-latch exactly one page.
- With tombstones, a delete that fits in the buffer and a revival write-latch exactly one page each.
- An insert that splits write-latches at least two pages; lookups and scans write-latch none.
- A lookup in another leaf gets through while an insert waits at its leaf.

## Hints

### Count first, then optimise

Run the latch-count tests against the code of 2e-01: they fail with "more than one write latch". That number is the measure of the fast path; print `get_reads()` and `get_writes()` around one operation to see which path it took.

### The leaf at the root

When the tree is a single leaf, the leaf's parent is the header page. Keep the header read-latched while you re-latch the leaf for writing, and remember that a root leaf has its own rule for a remove: it may go down to one pair, not to none.

### Tombstones change what "safe" means

A logical delete leaves the leaf's size alone unless the buffer is full, in which case one pair goes for real. Work out when that can make the leaf short; only then does the fast path have to give up.

## Performance

On the fast path, a write takes read latches on the header and the internal pages and one write latch on a leaf: writers on different leaves share everything above the leaves, and contention moves to the leaves themselves. The cost of losing the bet is one extra descent, paid only by inserts that split and removes that rebalance, typically a few percent of writes.

**Measure it.** Inserts per second with 1, 2, 4 and 8 threads on random keys, with the fast path and without it. Predict where the curve without it flattens.

## Experiment

Optional. Write your prediction down first, then run.

1. **Write-latch the path.** Take write latches all the way down on the fast path. Which test fails, and how much does throughput drop with 8 threads?
2. **Forget the re-check.** Decide "room" under the read latch only. Run the threaded tests in a loop: what goes wrong, and how often?

## Other designs

- **Optimistic with a pessimistic fallback (ours, BusTub's).**
- **Optimistic lock coupling** (Leis and others): version numbers instead of read latches on the way down, re-validated after the fact; readers write nothing at all.
- **B-link trees:** the right-link lets a writer that found a split leaf move right instead of starting over.
- **Pessimistic only:** crabbing alone, as in 2e-01: simpler, the root's latch is the bottleneck.

## In BusTub

BusTub's project asks for an optimistic insert and remove and checks them with `OptimisticInsertTest` and `OptimisticDeleteTest`, which count `GetReads` and `GetWrites` on the traced pool exactly as here; they run in this module's boss.

## The C/C++ way

| C / C++ | Rust |
|---|---|
| `std::shared_mutex` without an upgrade | `RwLock` without an upgrade |
| re-latch under the parent's shared latch | the same, with guards |
| `TracedBufferPoolManager::GetWrites()` | `tree.bpm.get_writes()` |

**Port rule:** the latch protocol is the same in both languages; in Rust each latch is a guard value, so "what does this thread hold" is visible in the code.

## Learn more

- Leis, Haubenschild, Neumann, [*The ART of Practical Synchronization*](https://db.in.tum.de/~leis/papers/artsync.pdf), 2016: optimistic lock coupling
- [Linux kernel seqlocks](https://docs.kernel.org/locking/seqlock.html): optimistic readers with a version counter
