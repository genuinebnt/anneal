LRU is exact but costs a list and a map, and a hit has to touch the list. Operating systems approximate it with a cheaper idea from the 1960s: arrange the evictable frames on a **ring** with a **hand** pointing at one. Each frame has one bit, set when the frame is used. To find a victim, sweep the hand round: a frame whose bit is set gets a **second chance** (the bit is cleared, the hand moves on); the first frame found with its bit clear is the victim. A frame that is used often keeps getting its bit set and survives. One bit per frame replaces a timestamp or a list position.

> [!CHECK] Every frame on the ring has its bit set and the hand starts a sweep. What happens, and how many frames does the hand pass before it evicts? Is that a problem for the pool, and what does it say about the worst case of `victim`?
> ||The hand clears every bit on its first revolution, then comes back to the first frame, whose bit is now clear, and evicts it: the sweep passes up to *n* frames, so a single `victim` is O(n) in the worst case. Averaged over many calls it is much cheaper, because the bits cleared by one sweep make the next sweeps short, but a pool that cares about the latency of each eviction would notice.||
>
> - What state has the sweep changed besides the victim?
> - Where does the hand point afterwards?
> - What would an LRU replacer do in the same situation?

## The task

`ClockReplacer::new(num_pages)` implements the same `Replacer` trait as LRU, with this policy:

- **The ring and the hand.** Evictable frames sit on a ring in some order; the hand points at one of them.
- `unpin(f)`: if `f` is already on the ring, its bit is set and it keeps its place. Otherwise it joins the ring **just behind the hand** (the last place the hand will reach), with its bit set.
- `pin(f)`: `f` leaves the ring. The hand keeps pointing at the same frame it did; if `f` was the one under the hand, the hand moves to the next frame. A frame not on the ring: nothing happens.
- `victim()`: the hand sweeps as described. The frame it stops on is removed and returned, and the hand then points at the frame after it. `None` if the ring is empty.
- `size()`: the frames on the ring. More than `num_pages` distinct frames is a caller bug: it panics.

The tests check the policy against a model written as a rotating queue, and the general contract that LRU also keeps.

## Your freedom

How the ring and the hand are stored: a `Vec` and an index, a `VecDeque` that you rotate, a linked structure, a fixed array with a "present" flag. What you cannot change is what the policy says about order, so decide how to represent "just behind the hand" before you write code.

## The Rust toolbox

**Positions that must survive removals.** If the ring is a `Vec` and the hand is an index, removing an element *before* the hand shifts everything after it, so the hand must move back by one to keep pointing at the same frame; removing the one *under* the hand needs the hand to stay where it is (it now points at the next) unless that was the last. Write these three cases on paper first.

**`Vec::insert(i, x)` and `Vec::remove(i)`.** Both shift the tail, so they cost O(n); for a ring of a few thousand frames that is fine, and the experiment asks you to do better.

**Rotating instead of indexing.** A `VecDeque` where the front is the frame under the hand turns a sweep into `pop_front` and `push_back`, and "just behind the hand" into `push_back`. `rotate_left(1)` moves the hand without touching positions.

**`iter().position(|x| ...)`** finds an index; `iter_mut().find(|x| ...)` finds a mutable entry. In a closure over a tuple, patterns work in the parameter list: `|&(frame, _)| frame == wanted`.

**`while` or `loop` with an exit.** The sweep ends only when a clear bit is found; `loop { ... return Some(frame); }` states that directly, and the compiler checks that every path returns or continues.

**`x = (x + 1) % n`** advances an index around a ring. If `n` can be zero this divides by zero, so check emptiness first.

## If this is new

- [S3 Vec & slices](/t/s3-vec-slices): `insert`, `remove`, `position`, `iter_mut`, and why indexing out of range panics.
- [S1 Option & Result](/t/s1-option-result): `let Some(x) = .. else { return }` and `?`.
- [L2 Borrowing](/t/l2-borrowing): why you cannot hold `&mut self.ring[i]` while calling another method that borrows `self`.
- The *CLOCK algorithm* concept (optional) walks through the sweep with pictures.
- S5 Queues & heaps: Use; Understand: `VecDeque` rotation, lazy deletion.
- Y5 Testing & verification: Understand it: model-based tests: a four-line model of the policy.

## Tests

- A frame whose bit was set again gets a second chance (victims come out in the clock's order, not LRU's).
- Unpinning a frame already on the ring changes neither its place nor the size.
- Pinning a frame behind the hand leaves the hand on the same frame; pinning the one under the hand moves it to the next.
- For random sequences, victims and sizes agree with the model of the policy, and the general replacer contract holds.

## Hints

### Draw it

Take the sequence `unpin 1, 2, 3; victim; unpin 2; victim; victim` and draw the ring, the bits and the hand after every step. Predict the three victims, then run the second-chance test and compare. If your picture and your code disagree, the picture is cheaper to fix.

### Where is "just behind the hand"?

If the ring is a `Vec` with the hand at index `h`, the frame just behind the hand sits at index `h - 1` (wrapping). Inserting a new frame there, and keeping the hand on the same frame, shifts one index. What about an empty ring?

### The hand after a removal

Both `pin` and `victim` remove a frame. In each, say where the hand ends up in the three cases: the removed frame was before the hand, under it, or after it. Do both functions need the same rule?

## Performance

A `victim` sweep is O(n) in the worst case and cheap on average. The bigger cost in this design is `unpin` and `pin`, which search the ring for the frame: O(n) per call with a `Vec`. LRU's list-and-map design had O(1) for these.

**Measure it.** Run the 100 000-frame timing test from 1c-02 on your CLOCK (copy it). It will be slow. What single structure would make `pin` and `unpin` O(1) without changing the policy? (This is the first experiment.)

## Experiment

Optional. Predict first, then run.

1. **Constant time.** Add a `HashMap<FrameId, usize>` from frame to ring slot, or keep a fixed array with a "present" flag, and make `pin` and `unpin` O(1). What now has to be kept consistent when the ring changes, and what does `victim` cost?
2. **CLOCK against LRU.** In the boss stage you will see that CLOCK behaves exactly as LRU when every use is a pin followed by an unpin. Predict what happens to the victims when frames are unpinned *without* being pinned first, then write a test that shows the difference.

## Other designs

- **`Vec` ring with an index (ours).** Short; `pin` and `unpin` are O(n).
- **`VecDeque` rotated so the front is the hand.** The sweep is `pop_front`/`push_back`; `pin` is a search plus a remove.
- **A fixed array of `(present, bit)` per frame id.** O(1) for everything except finding the next present frame; the hand scans slots.
- **A circular linked list in an arena.** O(1) removal by handle (like 1c-01); more code.

## In BusTub

```cpp
class ClockReplacer : public Replacer {
  auto Victim(frame_id_t *frame_id) -> bool override;   // sweep: clear the bit, or take the frame
  void Pin(frame_id_t frame_id) override;                // remove from the ring
  void Unpin(frame_id_t frame_id) override;              // add with the reference bit set
};
```

## The C/C++ way

| C / C++ | Rust |
|---|---|
| `std::vector<bool>` of reference bits | `Vec<(FrameId, bool)>` (a bit beside each frame) |
| `clock_hand_ = (clock_hand_ + 1) % size` | `self.hand = (self.hand + 1) % self.ring.len()` |
| `bool Victim(frame_id_t *out)` | `fn victim(&mut self) -> Option<FrameId>` |
| a mutex in the class | `&mut self` |

**Port rule:** a `std::vector<bool>` of per-element flags becomes a field in the element, which keeps the flag and the element from drifting apart.

## Learn more

- [`Vec::insert`](https://doc.rust-lang.org/std/vec/struct.Vec.html#method.insert) · [`VecDeque::rotate_left`](https://doc.rust-lang.org/std/collections/struct.VecDeque.html#method.rotate_left)
- [Page replacement algorithms: clock](https://en.wikipedia.org/wiki/Page_replacement_algorithm#Clock)
