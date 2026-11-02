Half of ARC is a split between **recency** and **frequency**. A page seen once recently belongs on a list called `mru`; a page seen at least twice belongs on `mfu`. When the pool needs a frame, ARC takes it from `mru` first (while the target is zero, as it is until the next stage): the pages seen once are the ones a scan produces, and they are the cheapest to lose. The pages seen twice stay. This alone already makes a scan harmless, and it needs no counters, just two lists.

> [!CHECK] A scan reads 1 000 pages once each through a pool of 100 frames that hold 60 frequently used pages. With the two lists above and the target at zero, which frames are evicted, in what order, and which survive? What would plain LRU have done?
> ||The 60 hot pages sit on `mfu` (each was seen at least twice). The scan's pages go to `mru`. Evictions take the oldest evictable frame of `mru` first, so the scan evicts its own earlier pages and the 60 hot pages survive. LRU puts the scan's pages at the newest end and evicts the oldest *by last use*, which are the hot pages if they were not touched during the scan: the whole hot set is flushed.||
>
> - Which list does a page enter first?
> - What makes it move to the other?
> - Where does a victim come from if `mru` is empty?

## The task

Same interface as 1e-01. Now `evict` follows the policy, and `record_access` keeps two lists, each ordered from the **oldest** entry to the **newest**:

- A frame that is new to the replacer joins `mru` at the **newest** end.
- A **hit** (a frame that is already live) moves it to the **newest** end of `mfu`, from `mru` or from `mfu`.
- `evict()` takes the **oldest evictable** frame of `mru` if there is one, otherwise of `mfu`. Pinned frames are skipped, not removed. `None` if no frame is evictable.
- `remove(frame)` takes the frame off whichever list holds it.
- Remember the page of an evicted frame as a **ghost** at the newest end of a ghost list of the side it came from (`mru_ghost` for `mru`, `mfu_ghost` for `mfu`). Nothing observes ghosts in this stage, because the tests never bring an evicted page back; the next stage uses them. Building them now is optional but saves a rewrite.

Also: a hit must be **constant time**, even in the middle of a list of a hundred thousand frames. A test does 400 000 hits in the middle of a list of 100 000.

The tests compare your victims with a model made of two plain vectors, on random sequences in which no evicted page ever returns.

## Your freedom

The data structure behind each list (any structure with O(1) removal from the middle: an arena, a map plus linked nodes, an index into a `Vec` with tombstones), and how a frame finds its list entry.

## The Rust toolbox

**O(1) "remove from the middle".** Your frame map stores a handle to the frame's node in its list. A hit is then `list.remove(handle)` (O(1)) and `other.push_back(frame)` (O(1)). `Vec::remove(i)` would be O(n) and fail the speed test.

**Update a map entry in place.** `if let Some(live) = self.live.get_mut(&frame) { live.side = Side::Mfu; live.handle = new_handle; }`: take the new handle first, then update, to avoid holding two borrows of `self` at once.

**Why the compiler complains about two borrows.** `let live = self.live.get(&frame)?;` borrows `self.live`; calling `self.mfu.push_back(..)` while `live` is still used is fine (different fields), but calling `self.push_alive(..)` (a method on all of `self`) is not. Copy out what you need (`let (side, handle) = (live.side, live.handle);`) so the first borrow ends before you call the method.

**Skipping with an iterator.** `list.iter().copied().find(|frame| self.live[frame].evictable)` finds the oldest evictable. Indexing a `HashMap` with `[]` panics for a missing key, which is right here because a listed frame must be live.

**`if let ... else` chains versus `match`.** A `match (order[0], order[1])` over the two lists is clearer than nested `if let`s when there are several ordered fallbacks.

## If this is new

- [S3 Vec & slices](/t/s3-vec-slices) and [S4 Maps & sets](/t/s4-maps-sets).
- [L2 Borrowing](/t/l2-borrowing): two borrows of one struct: fields versus methods.
- The *arenas and generational handles* concept (optional) if your 1c-01 list is the base.
- S7 Smart pointers & interior mutability: Understand it: handles instead of references between list nodes.

## Tests

- Frames seen once leave before frames seen twice; oldest first within each.
- A hit moves a frame to the newest end of `mfu`.
- A pinned frame is skipped but not dropped.
- Removing takes a frame off its list.
- For random sequences with no returning pages, victims equal the model's.
- A hit in the middle of a list of 100 000 frames is constant time (400 000 hits within the limit).

## Hints

### Draw two lists and a few operations

Take `access 1, access 2, access 1, access 3` and draw `mru` and `mfu` after each step. Then say which frame `evict` returns and which it returns next.

### What does a hit change?

The frame moves from its list to the newest end of `mfu`. Its `Live` record changes (list, handle). The list node must be removed and created, or moved. Which of these are O(1) in your design?

### The evict loop

Write `evict` as two attempts: first `mru`, then `mfu`. Each attempt is "the oldest evictable of this list, if any". Can one function serve both?

## Performance

A hit is a lookup, a list unlink and a push: tens of nanoseconds. An eviction is a walk past pinned frames at the old end of a list, then an unlink and a ghost push. If many frames are pinned the walk grows; a pool that pins a large fraction of its frames is in trouble anyway.

**Measure it.** The speed test does 400 000 hits at 100 000 frames. Replace your list with a `Vec` and time it at 10 000, 100 000 frames: the ratio shows the O(n).

## Experiment

Optional. Predict first, then run.

1. **A scan, measured.** Replay a trace of 20 hot pages and a one-off scan of 1 000 pages through a pool of 32 frames with this replacer and with your 1c-02 LRU replacer. Count hits on the hot pages after the scan.
2. **Skip cost.** Pin the 50 oldest frames and time 1 000 evictions. What dominates?

## Other designs

- **Two lists with handles (ours).** O(1) for everything except skipping pinned frames.
- **Keep evictable and pinned frames on separate lists.** `evict` never skips; `set_evictable` moves a frame between lists. Costs a move per pin and unpin.
- **A map from frame to a sequence number, with a heap per side.** O(log n); no linked lists.

## In BusTub

BusTub's spec for these two lists: a new page goes to the front of `mru`; a hit on a page already in `mru` or `mfu` moves it to the front of `mfu`; an eviction takes from `mru` if its size is at least the target, and so on. This course writes the oldest at the front and the newest at the back so that eviction takes from the front, a mirror image of the same lists.

## The C/C++ way

| C / C++ | Rust |
|---|---|
| `mfu_.splice(mfu_.begin(), mru_, it)` (move a node between lists) | `mru.remove(handle)` then `mfu.push_back(frame)` and store the new handle |
| `std::unordered_map<frame_id_t, ArcStatus>` | `HashMap<FrameId, Live>` with a `Side` field |
| a loop over a `std::list` with a `break` | `iter().find(..)` |

**Port rule:** `splice` between lists becomes remove-then-push, and a saved iterator becomes a freshly stored handle.

## Learn more

- Wikipedia: [adaptive replacement cache](https://en.wikipedia.org/wiki/Adaptive_replacement_cache) · [`Iterator::find`](https://doc.rust-lang.org/std/iter/trait.Iterator.html#method.find)
