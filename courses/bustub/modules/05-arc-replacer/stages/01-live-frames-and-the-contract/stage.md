ARC (the Adaptive Replacement Cache, Megiddo and Modha, FAST 2003) is what the buffer pool will use. It improves on LRU and on LRU-K by *learning from its mistakes*: it remembers which pages it evicted, and if one comes back soon it concludes it evicted from the wrong side and shifts its balance. This module builds it in three steps. The first step has no policy at all; it sets up what ARC needs and fixes the contract.

The first difference from LRU-K is in the interface: `record_access(frame, page)` takes the **page** the frame holds, not just the frame, because ARC must recognise a page that was evicted from a *different* frame and has now returned.

> [!CHECK] A page is evicted from frame 3. A moment later the same page is read from disk again into frame 7. What does the replacer see, and why can it not tell this is "the same page coming back" from the frame number alone?
> ||The replacer sees `record_access(frame 7, page P)`: a frame it has no record of (frame 3 was forgotten on eviction), holding a page. The frame numbers are different, so frame ids say nothing about the page's history. Only the **page id** identifies the page across evictions; so the replacer keeps a list of recently evicted *page ids* (ghosts) and checks the incoming page id against it.||
>
> - What is a frame, and what is a page?
> - Which one survives an eviction?
> - What would the replacer have to remember to recognise a returning page?

## The task

`ArcReplacer::new(num_frames)` makes a replacer for a pool of `num_frames` frames (`c` in the paper). The methods:

- `record_access(frame, page)`: if `frame` is **live** (the replacer holds it), this is a **hit** on the page it holds. Otherwise `frame` now holds `page`: it becomes live, **not evictable**. A caller never gives one page to two live frames.
- `set_evictable(frame, bool)`: changes whether a live frame may be evicted. A frame that is not live is ignored. Setting the value it already has changes nothing.
- `size()`: the number of live frames that are evictable.
- `evict()`: removes one evictable frame and returns it, or `None` if none is evictable. The frame is no longer live. (**Which** frame is yours to choose for now.)
- `remove(frame)`: forgets an evictable frame (its page was deleted). Not live: ignored. Live but **not evictable**: panic.

The tests run random sequences on your replacer and a small model of the contract: `size` is the evictable count, a victim is always an evictable live frame and is gone afterwards, `None` only when nothing is evictable, and hits do not disturb the flag or the count.

## Your freedom

How live frames are stored, and how you will keep four lists in the next stages. The hits of the next stage need a way to find a frame in a list in O(1): the `IndexList` and `Handle` from 1c-01 are one answer, and your own structure another.

## The Rust toolbox

**A newtype per kind of id.** `FrameId(usize)` and `PageId(i32)` are different types, so the compiler rejects a call that passes a page where a frame is wanted. When you store both in maps, `HashMap<FrameId, Live>` and `HashMap<PageId, Ghost>` document which is which.

**An enum for "where is this entry?"** `enum Side { Mru, Mfu, MruGhost, MfuGhost }` as a field says which of the four lists an entry belongs to; `match` forces every case to be handled. Derive `Clone, Copy, PartialEq, Eq` so you can compare and copy it freely.

**Reuse what you built.** `IndexList<FrameId>` and a `HashMap<FrameId, Handle>` give you "remove from the middle" in O(1); the module will call it a dozen times. If your 1c-01 list is not complete, finish it first: it is a dependency of this module's *types*, not of its tests (tests only call `ArcReplacer`).

**Testing a replacer without a pool.** The tests call `record_access` with made-up page ids: the replacer does not care that the pages do not exist. A replacer is a pure data structure, which is why it can be tested hard in isolation.

**Panic for caller bugs only.** `remove` on a pinned frame is a bug in the pool, so panic; `set_evictable` on an unknown frame is harmless (the pool may race with an eviction in a design with several threads), so ignore it.

## If this is new

- [S4 Maps & sets](/t/s4-maps-sets): `HashMap` with two key types, `entry`, `remove`.
- [S1 Option & Result](/t/s1-option-result): `let ... else`.
- [L1 Ownership & moves](/t/l1-ownership-moves): why you cannot keep a `&mut` into one list while changing another; store handles, not references.
- Concept *adaptive replacement cache* (optional) has the whole algorithm with pictures.
- S7 Smart pointers & interior mutability: Understand it: handles instead of references between list nodes.
- S8 The core traits: Derive: `Hash`, `Eq`, `Copy` on id newtypes.

## Tests

- A new frame is not evictable; size counts evictable frames once each.
- A hit keeps the frame live and keeps its flag.
- Evicting forgets the frame; `remove` forgets an evictable one, ignores unknown ones and panics for a pinned one.
- For random sequences, the contract holds at every step.

## Hints

### What must a live frame remember?

Which page it holds, whether it is evictable, and (for the next stage) which list it is on and where. Write the struct on paper before you decide how to store it.

### Hit or new?

`record_access` has exactly two cases here. How does the replacer tell them apart, and what do the two paths do to the count?

## Performance

All operations are hash lookups and a list operation: constant time. The only cost to plan is the **O(n) walk** in `evict` to skip frames that are not evictable; the next stages keep it bounded by how many pinned frames sit at the old end of a list.

**Measure it.** 256 000 frames, all live and evictable: how long does it take to access all of them once? That is the work BusTub's performance test (in the boss stage) repeats ten times.

## Experiment

Optional. Predict first, then run.

1. **Different frame for a returning page.** In a test of your own, evict a page, then give it a different frame. Which part of your design would break if the replacer remembered pages by *frame*?
2. **Counts.** Write a `fn check(&self)` that recomputes the evictable count and compares with the counter, and call it after every operation of a random run.

## Other designs

- **Four `IndexList`s and two maps (ours).** Every operation O(1).
- **One `HashMap<PageId, Entry>` with a status field and a global recency counter.** Simplest; eviction is a scan or a priority structure.
- **Intrusive lists in the frame table.** The pool stores list links in each frame; fastest; couples the replacer to the pool.

## In BusTub

```cpp
class ArcReplacer {
 public:
  explicit ArcReplacer(size_t num_frames);
  auto Evict() -> std::optional<frame_id_t>;
  void RecordAccess(frame_id_t frame_id, page_id_t page_id, AccessType access_type = AccessType::Unknown);
  void SetEvictable(frame_id_t frame_id, bool set_evictable);
  void Remove(frame_id_t frame_id);
  auto Size() -> size_t;
 private:
  std::list<frame_id_t> mru_, mfu_;
  std::list<page_id_t> mru_ghost_, mfu_ghost_;
  size_t mru_target_size_{0};
};
```

## The C/C++ way

| C / C++ | Rust |
|---|---|
| `std::list<frame_id_t>` plus a map of iterators | `IndexList<FrameId>` plus a map of `Handle`s |
| `enum class ArcStatus { MRU, MFU, MRU_GHOST, MFU_GHOST }` | `enum` with the same variants; `match` must cover all |
| `std::optional<frame_id_t> Evict()` | `fn evict(&mut self) -> Option<FrameId>` |
| `BUSTUB_ASSERT` for caller bugs | `assert!` with a message |

**Port rule:** four containers and a status enum stay four containers and a status enum; iterators into the containers become handles.

## Learn more

- Megiddo and Modha, *ARC: A Self-Tuning, Low Overhead Replacement Cache*, FAST 2003 · [the paper's summary on Wikipedia](https://en.wikipedia.org/wiki/Adaptive_replacement_cache)
