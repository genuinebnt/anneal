Now the policy. LRU-K judges a frame by its **backward k-distance**: how long ago was its k-th most recent access? A frame that was used twice recently has a small distance for `k = 2`; a frame used once has seen only one access, so it has not *shown* that it will be used again, and its distance is **infinite**. Among infinite-distance frames the oldest first access goes first (that is plain LRU on the first touch); among the rest the largest finite distance goes first. A single scan over a huge table gives every page one access each and so loses to every page that is genuinely hot.

> [!CHECK] Pool of 4 frames, `k = 2`, clock ticks once per access. Accesses in order: 0, 1, 0, 2, 3, 2, 1. Which frame would LRU evict, which would LRU-2 evict, and are they the same? Say what each rule looks at.
> ||Last accesses: frame 0 at time 2, 1 at time 6, 2 at time 5, 3 at time 4. Plain LRU evicts the one with the oldest *last* access: frame 0. For LRU-2 look at the second most recent access: frame 0 → time 0, frame 1 → time 1, frame 2 → time 3, frame 3 has only one access so its distance is infinite. LRU-2 evicts frame 3, although it is newer than frame 0, because it has not shown any repeated use. They differ.||
>
> - What is frame 3's backward 2-distance?
> - Which frames have enough history to be compared by distance?
> - What would the answer be with `k = 1`?

## The task

Same interface as 1d-01; now `evict` must pick the victim by this rule, among evictable frames only:

1. If some evictable frame has **fewer than `k`** recorded accesses, choose among those the one whose **first** access is oldest.
2. Otherwise choose the frame whose **k-th most recent** access is oldest (the largest backward k-distance).

History is the accesses since the frame was last forgotten, and evicting or removing a frame forgets it. Whether a frame is currently evictable does not change its history: a frame marked non-evictable and evictable again keeps its accesses.

The tests compare your replacer with a brute-force model (a map from frame to every access time, the rule applied by scanning) over random sequences for `k` from 1 to 4, and check four consequences of the rule: with `k = 1` it is plain LRU by last access; a frame with `k` accesses is never evicted while an evictable frame with fewer exists (*scan resistance*); when no frame has `k` accesses the order is by first access; and the distance is measured to the k-th most recent access, not the first.

## Your freedom

What you keep per frame, how you compare, whether you scan or keep a sorted structure (the third stage requires speed; this one does not). The test sees only which frame comes out.

## The Rust toolbox

**Compare with a key, not with nested ifs.** `iter().min_by_key(|n| key(n))` returns the element with the smallest key. A tuple key encodes the two cases of the rule: `(0, first_access)` for frames with fewer than `k` accesses and `(1, kth_access)` for the others sorts all of the first kind before the second, and breaks ties by the number. Tuples compare lexicographically, which is why this works.

**`Option` as "infinite".** `None` can stand for "no k-th access yet"; note that `None < Some(_)` in Rust's ordering, which happens to put infinite first when you want it, but a named tuple key says what you mean.

**Chaining iterator adapters.** `self.nodes.values().filter(|n| n.evictable).min_by_key(...).map(|n| n.frame)` reads as the rule. If the compiler complains that a closure borrows something that moves, bind the pieces you need to local variables before the chain.

**A model written beside the code.** `tests/stages_1d.rs` holds the model in 10 lines (`model_victim`). Read it after you finish and compare with your version: where does your design store less, and why is that safe?

**Test by consequence.** Properties that follow from a rule ("with `k = 1` it is LRU", "a scanned frame goes before a hot one") are cheap to state and catch bugs a model of the same rule shares with the code. They are the second line of defence.

## If this is new

- [S3 Vec & slices](/t/s3-vec-slices) and [S1 Option & Result](/t/s1-option-result): sorting by key, `min_by_key`, `Option` ordering.
- The *iterators and closures* concept (optional) shows the adapter chains used here.
- The *LRU-K and scan resistance* concept (optional) has the algorithm with a worked example.
- S6 Iterators: Use; Understand: `filter().min_by_key().map()` chains.
- S8 The core traits: Implement by hand: `Ord` on tuples, deriving the order you need.
- Y5 Testing & verification: Understand it: a brute-force model; properties that follow from a rule (k = 1 is LRU).

## Tests

- A scan of single-access frames is evicted before a frame accessed twice; the oldest first access goes first among them.
- Among frames with `k` accesses, the oldest k-th access goes first; the distance is to the k-th, not the first.
- A frame marked non-evictable and back keeps its history.
- For random sequences and `k` from 1 to 4, victims equal the model's.
- Consequences: `k = 1` is LRU; no `k`-access frame is evicted while a scanned one is evictable; with fewer than `k` accesses everywhere the order is first access first.

## Hints

### Write the two cases on paper

List four frames with different histories for `k = 2` and sort them by hand using the rule. Then write the key that sorts them the same way.

### What to store

For `k = 3` you need the third most recent access time, so you must keep the last three. When a fourth arrives, what happens to the oldest? And when fewer than three have arrived, what do you report?

### Ties

Two frames cannot have the same access time: the clock advances on every access. So when does the model need a tie-break at all? (Only among infinite distances, and the first-access rule settles that.)

## Performance

`evict` by scanning every frame is O(n). Correct, and the next stage shows why it is not good enough. `record_access` costs O(1): a push, maybe a pop.

**Measure it.** Time `evict` at 1 000 and at 100 000 frames. If the second is 100 times slower, you have measured an O(n) algorithm; what would a log-factor look like?

## Experiment

Optional. Predict first, then run.

1. **A scan against LRU.** With 8 hot frames accessed 3 times each and then 100 scan frames accessed once, with a pool of 16, compare which frames each of LRU (your 1c replacer, adapted) and LRU-2 would evict. Count the hot frames lost.
2. **Larger k.** Run the same with `k = 3`, `k = 5`. What happens to a hot frame that is accessed only twice? What is the price of a larger `k`?

## Other designs

- **History deque per frame (ours).** Keeps the last `k` times; the k-th is the oldest kept.
- **A ring of `k` slots.** No allocation per frame beyond a fixed array; same behaviour.
- **Only two numbers per frame.** For `k = 2` you need only the last two times; special-casing `k = 2` saves memory and loses generality.
- **Approximate k-distance.** Real systems (including PostgreSQL's earlier ARC and its 2Q/clock-sweep) trade exactness for lower overhead.

## In BusTub

BusTub's LRU-K replacer follows the rule above: a frame with fewer than `k` recorded accesses has `+inf` backward distance, and ties among those go to the frame with the earliest recorded timestamp. The public `lru_k_replacer_test.cpp` is a sample scenario; it is ported in the boss stage.

## The C/C++ way

| C / C++ | Rust |
|---|---|
| `std::list<size_t> history_` and `history_.size() < k_` | `VecDeque<usize>` and `history.len() < k` |
| a loop that remembers the best so far | `iter().filter(..).min_by_key(..)` |
| `std::numeric_limits<size_t>::max()` for infinity | a tuple key whose first element is 0 or 1 |

**Port rule:** "a sentinel for infinity" becomes a separate component in a sort key.

## Learn more

- [`Iterator::min_by_key`](https://doc.rust-lang.org/std/iter/trait.Iterator.html#method.min_by_key) · [tuple ordering](https://doc.rust-lang.org/std/cmp/trait.Ord.html#impl-Ord-for-(T,)) 
- O'Neil, O'Neil and Weikum, SIGMOD 1993
