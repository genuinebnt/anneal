The buffer pool has far fewer frames than the database has pages, so it has to evict. A **replacer** keeps the set of frames that *may* be evicted (the ones nobody is using) and picks which of them to give up. The pool does not care how: it calls the four methods of the `Replacer` trait, and any policy that keeps their promises can sit underneath. You write the policy that most people write first, **least recently used**: the frame that has gone the longest without being unpinned is the one least likely to be needed soon.

> [!CHECK] Give a sequence of operations on which LRU keeps exactly the wrong frames, one that makes a sensible cache perform terribly. What property of the workload breaks the "recently used means likely to be used again" assumption, and what could a replacer remember to resist it?
> ||A one-time sequential scan over more pages than there are frames: every scanned page is touched once, so each is "most recently used" when it arrives, and together they push out the hot pages that were about to be needed again. The assumption breaks when pages are used once. A replacer can remember more than the last use: how many times a page was used (LFU), the time of the second-to-last use (LRU-K), or whether a page returned after being evicted (ARC). Those are the next three modules.||
>
> - What does a large table scan do to a buffer pool managed by LRU?
> - What would you need to record per frame to tell a one-off page from a hot one?
> - Which of the two costs more memory: one timestamp or a list of them?

## The task

The `Replacer` trait (given, `replacer.rs`) says what the buffer pool may rely on. `LruReplacer::new(num_pages)` makes a replacer for a pool of that many frames, and implements the trait:

- `unpin(frame)`: the frame may now be evicted. A frame the replacer does not hold is added as the **most recently used**. A frame it already holds is left where it is; unpinning twice does **not** refresh it.
- `pin(frame)`: the frame is in use and must not be evicted. It leaves the replacer. A frame that is not there: nothing happens.
- `victim()`: removes and returns the **least recently used** frame, or `None` if there is none.
- `size()`: how many frames may be evicted right now.
- Holding more distinct frames than `num_pages` is a bug in the caller: it **panics**.

The tests run random sequences against a four-line model of LRU, check the promises every policy makes (a general contract that you will also meet in CLOCK, LRU-K and ARC), and run 300 000 operations on 100 000 frames under a time limit. The file also runs a deliberately different, quadratic LRU through the same LRU properties, to show that they test behaviour and not one design.

## Your freedom

Everything inside `LruReplacer`. You can use the `IndexList` you built, a `HashMap` of linked nodes, a `BTreeMap` from a counter to a frame, or a `Vec` with lazy deletion. Any structure works if all four operations are fast enough for the time limit.

## The Rust toolbox

**A trait is the contract between components.** `trait Replacer { fn victim(&mut self) -> Option<FrameId>; ... }`. `impl Replacer for LruReplacer { ... }` is your agreement to keep it. The buffer pool and the tests hold a `Box<dyn Replacer>` and never name `LruReplacer`; that is why you can swap policies without touching the pool.

**`&mut self` means exclusive.** The replacer's methods take `&mut self`, so the caller must hold it exclusively, usually behind the pool's mutex. The replacer therefore needs no locks of its own, and it need not be `Sync`.

**Two structures that must agree.** Many designs keep an ordered structure and a map into it. Every method then changes both, and a bug is a place where only one is updated. A small private `fn check(&self)` that asserts `map.len() == list.len()` and calls it under `debug_assert!` in each method catches most of them.

**Use the entry API instead of get-then-insert.** `map.entry(frame).or_insert_with(|| list.push_back(frame))` looks the key up once, and `if let Entry::Vacant(e) = map.entry(frame) { ... }` lets you act only on the missing case. It also avoids the compiler's complaint about borrowing `map` twice.

**`HashMap::remove` returns the old value.** `if let Some(handle) = map.remove(&frame) { list.remove(handle); }` is the whole of `pin`.

**Panic with a message.** `assert!(len < self.capacity, "the replacer is full: more frames than it was made for")`. A bug in the caller should be loud, early, and say what happened.

## If this is new

- [L4 Traits & dispatch](/t/l4-traits-dispatch): defining and implementing a trait; `dyn Trait` behind a `Box`.
- [S4 Maps & sets](/t/s4-maps-sets): `HashMap`, `entry`, `remove`, `contains_key`.
- [S1 Option & Result](/t/s1-option-result): `?` on an `Option`, `if let Some(..)`.
- The *replacement policies* concept (optional) explains LRU, CLOCK and their cousins, and a hit-rate simulator you can reuse.
- S5 Queues & heaps: Use; Understand: `VecDeque` rotation, lazy deletion.
- Y5 Testing & verification: Understand it: model-based tests: a four-line model of the policy.

## Tests

- Frames leave in the order they were unpinned; a second unpin changes nothing; pinning removes, and a later unpin makes the frame the most recent; a replacer over capacity panics.
- For random sequences, victims and sizes agree with the LRU model.
- For random sequences, the general replacer contract holds: `size` is the number of evictable frames, a victim is always one of them, draining returns each exactly once.
- 300 000 operations on 100 000 frames finish under the time limit.

## Hints

### What is the invariant?

State, in one sentence each, what your structure says about a frame that is evictable, one that is pinned, and one that was just chosen as a victim. Check the sentences against `pin` of an unknown frame and `unpin` of a known one.

### Where does "most recent" live?

Whichever end of your ordered structure is the most recent, `unpin` of a new frame goes there and `victim` takes from the other. What does an unpin of a frame already present do to the order? The test named for it fails quickly if you refresh it.

### Making `pin` fast

`pin` has to find the frame wherever it is in the order. With a plain `Vec` that is a search (O(n)); with a list and a map it is a lookup and a unlink. If the timing test fails, which of your operations is doing the search?

## Performance

All four operations should be O(1) or O(log n) in the number of evictable frames. A pool of a million frames receives an operation for every page access, so the replacer's cost is paid on the hottest path in the system. A design with a search per operation is correct at ten frames and unusable at a hundred thousand.

**Measure it.** Replace your structure with a `Vec<FrameId>` and a linear search. At what size does 300 000 operations take a second? Predict the size, then find it. (The test file runs exactly such a design through the correctness properties.)

## Experiment

Optional. Predict first, then run.

1. **A scan.** Feed LRU a hot set of 20 pages accessed repeatedly, then a single pass over 1 000 pages, then the hot set again, with a pool of 32 frames. How many of the hot pages survive the scan? You have just reproduced the weakness the check-yourself question asked about.
2. **Lazy deletion.** Implement `pin` by marking a frame dead in a map and skipping dead entries in `victim`. What is the worst case for `victim`, and how does the size of the queue behave under a pin-heavy workload?

## Other designs

- **List plus map (ours).** O(1) for everything; two structures to keep in step.
- **`BTreeMap<u64, FrameId>` keyed by a counter.** `unpin` takes the next counter value; `victim` takes the smallest key; a second map `FrameId -> u64` finds a frame's key. O(log n), simple, no linked list.
- **`VecDeque` with lazy deletion.** `pin` only marks; `victim` skips the dead. Amortised O(1) but the queue can hold many dead entries.
- **An intrusive list in the frame table.** The pool stores each frame's links itself. Fastest, and it couples the replacer to the pool's layout, which the trait deliberately avoids.

## In BusTub

```cpp
class LRUReplacer : public Replacer {
 public:
  explicit LRUReplacer(size_t num_pages);
  auto Victim(frame_id_t *frame_id) -> bool override;
  void Pin(frame_id_t frame_id) override;
  void Unpin(frame_id_t frame_id) override;
  auto Size() -> size_t override;
 private:
  std::mutex latch_;
  std::list<frame_id_t> lru_list_;
  std::unordered_map<frame_id_t, std::list<frame_id_t>::iterator> map_;
};
```

## The C/C++ way

| C / C++ | Rust |
|---|---|
| `class LRUReplacer : public Replacer` with `override` | `impl Replacer for LruReplacer` |
| `bool Victim(frame_id_t *out)` | `fn victim(&mut self) -> Option<FrameId>` |
| a `std::mutex latch_` inside the class | none: `&mut self` makes the caller hold exclusive access |
| `list` + `unordered_map<_, list::iterator>` | `IndexList` + `HashMap<FrameId, Handle>` (or your own design) |

**Port rule:** an out-parameter plus a `bool` becomes an `Option`; a lock inside an object becomes `&mut self` and a lock in the owner.

## Learn more

- [`HashMap` entry API](https://doc.rust-lang.org/std/collections/hash_map/enum.Entry.html) · [`BTreeMap`](https://doc.rust-lang.org/std/collections/struct.BTreeMap.html) · [`debug_assert!`](https://doc.rust-lang.org/std/macro.debug_assert.html)
- [Page replacement algorithms](https://en.wikipedia.org/wiki/Page_replacement_algorithm) (the survey of LRU, CLOCK, LFU, ARC)
