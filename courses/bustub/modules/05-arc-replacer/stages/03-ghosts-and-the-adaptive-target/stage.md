This is the part of ARC that makes it adaptive. When a page is evicted from `mru`, its id is remembered on `mru_ghost`; from `mfu`, on `mfu_ghost`. If a page that is **on a ghost list** is accessed again, ARC has learned something: the side it evicted from was too small. It brings the page back (on `mfu`, since it has now been seen again), and moves a target `p`, the number of frames `mru` should hold, towards the side that lost the page. The next evictions then take from the other side.

> [!CHECK] A workload alternates between a small hot set and a long scan, and the target `p` is 0. A hot page that was evicted from `mfu` returns. In which direction should `p` move, and what is the effect on the next victim? What would go wrong if ghost lists were unbounded?
> ||The page was lost from `mfu`, which means `mfu` was too small, so `mru` should shrink: `p` goes **down**. A lower target means `mru` is allowed fewer frames, so evictions come from `mru` more readily and `mfu` is protected. (If it had come from `mru_ghost`, `p` would go up, protecting `mru`.) Unbounded ghost lists would grow with every distinct page ever seen: the replacer would need memory proportional to the whole database, not to the pool. ARC caps the four lists, so the extra memory is at most about the size of the pool again.||
>
> - Which list was too small, if a page returns from `mfu_ghost`?
> - What does the target `p` control, exactly?
> - How many page ids may the ghost lists hold at most?

## The task

State: the four lists (`mru` and `mfu` hold frames, the ghost lists hold page ids, all oldest first), the target `p` (starts at 0) and `c` = `num_frames`. The full rules, which the model in the tests follows:

**`record_access(frame, page)`**

1. If `frame` is live: a **hit**, as in 1e-02 (to the newest end of `mfu`).
2. Else if `page` is on `mru_ghost`: set `delta = 1` if `|mru_ghost| >= |mfu_ghost|`, else `|mfu_ghost| / |mru_ghost|` (integer division, computed **before** removing the page). Then `p = min(p + delta, c)`. Remove the page from the ghost list; `frame` joins the newest end of `mfu`, not evictable.
3. Else if `page` is on `mfu_ghost`: set `delta = 1` if `|mfu_ghost| >= |mru_ghost|`, else `|mru_ghost| / |mfu_ghost|`. Then `p = max(p - delta, 0)`. Remove the page from the ghost list; `frame` joins the newest end of `mfu`, not evictable.
4. Else a **new page**. First keep the lists within their limits: if `|mru| + |mru_ghost| >= c`, drop the oldest page of `mru_ghost` (if any); otherwise if the total of all four lists `>= 2c`, drop the oldest page of `mfu_ghost` (if any). Then `frame` joins the newest end of `mru`, not evictable.

**`evict()`**: if `|mru| >= p`, try `mru` first and then `mfu`; otherwise try `mfu` first and then `mru`. In each list take the oldest evictable frame. The victim's page goes to the newest end of the ghost list of the side it came from. `None` if nothing is evictable.

**`remove(frame)`** forgets the frame and leaves **no ghost** (the page was deleted, not evicted).

The tests check this against the model, on small pools where every rule is exercised, plus hand-worked scenarios.

## Your freedom

How the ghost lists and their limits are stored. The target `p` is internal; its effect is observable only through victims.

## The Rust toolbox

**Integer division and saturating subtraction.** `a / b` truncates toward zero for positive numbers; `p.saturating_sub(delta)` stops at 0 instead of wrapping around (which would panic in debug builds: "attempt to subtract with overflow"). `(p + delta).min(c)` caps the other end.

**One place to compute each rule.** Write `fn on_ghost_hit(&mut self, ...)` once. If two branches copy-paste the delta arithmetic with the lists swapped, a typo in one of them is exactly the bug a proptest finds three hours later.

**Order of operations on a list.** The rule says "computed before removing". With a handle design, take the lengths into local variables first (`let (a, b) = (self.mru_ghost.len(), self.mfu_ghost.len());`), then remove. A rule that reads state it has just changed is a classic source of off-by-ones.

**Early return as a rule list.** Each of the four cases in `record_access` ends the function: `return;`. The rules are exclusive and ordered, and the code can read like them.

**Invariant checks.** `debug_assert!(self.mru.len() + self.mru_ghost.len() <= c)` after every operation costs nothing in release builds and finds limit bugs on the spot.

## If this is new

- [S1 Option & Result](/t/s1-option-result): `if let Some(x) = map.remove(..)`.
- [L1 Ownership & moves](/t/l1-ownership-moves): why the ghost record is *removed* from its map and then used.
- Concept *adaptive replacement cache* (optional) has a diagram of the four lists and the target.
- [L2 Borrowing](/t/l2-borrowing): Split borrows; Borrow-checker limits: copy out what you need before calling `&mut self` methods.
- Y5 Testing & verification: Understand it: an exact model of a published algorithm.

## Tests

- After two ghost hits on `mru_ghost` the target is 2, and the pool evicts from `mfu` while `mru` is smaller than the target.
- Without ghost hits the target stays at 0 and `mru` always goes first.
- A frame brought back after `remove` does not count as a ghost hit.
- A ghost that has been pushed off its list is forgotten: its page is a new page again.
- For random sequences with returning pages on pools of 2 to 5 frames, and on tiny pools of 3, victims equal the model's.

## Hints

### Walk the scenario by hand

The first test describes a pool of 4 frames. Draw the lists and `p` after each call, and predict the order of the final four evictions before you run it.

### Which list is "too small"?

A ghost hit on `mru_ghost` means `mru` lost a page it should have kept, so `p` (the target for `mru`) grows. Check your signs with the scenario above; swapping them passes simple tests and fails the random ones.

### The limits

Rule 4 has two limits and an order. What happens when both are exceeded? Which list is trimmed? Check the order against the model, not against what seems natural.

## Performance

Every rule is O(1) in list operations; the delta needs a division. The ghost lists hold at most `c` and `2c` entries, so the replacer's memory is bounded by a small multiple of the pool.

**Measure it.** Replay the scan trace of the boss stage with the target fixed at zero and with adaptation on. How much of the difference in hit rate comes from ghost hits?

## Experiment

Optional. Predict first, then run.

1. **Fixed target.** Set `delta = 0` in both ghost branches and run the random test. How fast does it fail, and what is the shortest counterexample? It is a measure of how much each rule matters.
2. **A `delta` of 2.** Use a constant 2 instead of the ratio. Which tests notice? Which behaviour does the model never see?

## Other designs

- **Four `IndexList`s (ours).** O(1) everywhere.
- **A single list of tagged entries.** Entry status says which of the four lists it is on; moves between lists become status changes; eviction needs per-status oldest pointers.
- **A heap per list with version numbers.** O(log n); no handles.

## In BusTub

The rules above are BusTub's ARC specification, including its choice of `delta` from the ratio of the ghost list sizes (the paper's `max(1, |B2|/|B1|)`). The 2025 course tests it with a sample scenario and a performance test; the sample is ported in the boss stage, the performance test too.

## The C/C++ way

| C / C++ | Rust |
|---|---|
| `std::max(1, b2.size() / b1.size())` | `if a >= b { 1 } else { b / a }` or `(b / a).max(1)` |
| `unsigned` subtraction that wraps silently | `saturating_sub` (or `checked_sub`) |
| an `assert` that vanishes in release | `debug_assert!`; use `assert!` where a violation must stop the program |

**Port rule:** `size_t` arithmetic that can go below zero must be saturating or checked; the compiler will not warn in release.

## Learn more

- [`usize::saturating_sub`](https://doc.rust-lang.org/std/primitive.usize.html#method.saturating_sub) · [`debug_assert!`](https://doc.rust-lang.org/std/macro.debug_assert.html)
- Megiddo and Modha, FAST 2003, Figure 4 (the ARC algorithm)
