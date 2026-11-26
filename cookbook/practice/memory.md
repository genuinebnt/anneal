# Practice: memory

**How to use this page.** Run each recipe in `--release`, then do **Prove you got it** by printing the numbers (offsets, counts, sizes) before you read the code again.
Memory recipes are best learned by writing the invariant checker first: for each [A] task, write `check_invariants()` before the allocator itself.
Finish with a Chapter project. Run unsafe parts under Miri when you can (`cargo +nightly miri run --example <name>`).

---

## arena_ids
**Prove you got it**
1. `main` builds `(2 + 3) * 4` with five `arena.alloc(..)` calls and prints `arena.0.len()`. Predict the node count (5) and the value (20). Then change the build order (allocate `sum` before `two`) and predict whether it compiles (no: `ExprId`s must exist first); read the error.
2. Predict `size_of::<Expr>()` for the arena version (an enum of `f64` or two `u32`-sized ids) versus an equivalent `Box<Expr>` version (add one to the recipe copy), print both and explain the difference.

**Try next**
- [S] Add `Expr::Neg(ExprId)` and a `Sub` node, extend `eval`, and assert `-(2 - 5) * 4 == 12.0` with hand-built nodes.
- [M] Add `fn depth(&self, id: ExprId) -> usize` written iteratively with an explicit `Vec` stack and call it on a chain of 1,000,000 `Add` nodes. Failure mode: write the recursive version first and show it overflowing in a thread spawned with `stack_size(256 * 1024)`.
- [A] Write a tiny parser for `+ - * /` and parentheses that builds nodes in the arena; acceptance: `eval` equals a recursive-descent oracle that returns `Box<Expr>` trees on 10,000 seeded random expression strings, and `perf/counting_allocator` shows the arena version doing O(log n) allocations (Vec growth) against one per node.

**Use it in:** an AST or IR for a DSL compiler. Real: rustc's arenas <https://github.com/rust-lang/rust/tree/master/compiler/rustc_arena> and oxc <https://github.com/oxc-project/oxc>. (If this feeds a FlowForge DSL stage: attempt your stage first.)

**Pitfall:** mixing `ExprId`s from two different arenas; nothing in the types stops it unless you brand the ids.

---

## bump_arena_allocator
**Prove you got it**
1. The checkpoint test asserts that after a `scope` that grows chunks, `(a.chunk_count(), a.used_bytes()) == (1, used0)` and that the same address is reused after `rewind`. Predict `first_addr == again` and what breaks if `rewind` only resets the cursor but not the chunk list.
2. The drop-glue test asserts `Rc::strong_count(&rc) == 2` after `alloc_value` ("the clone leaked") while `alloc_with_drop` runs drops in order `[11, 10, 2, 1, 0]`. Predict the count if you call `reset` after `alloc_value` (still 2) and after `alloc_with_drop`.

**Try next**
- [S] Allocate a `u8` then a `u64` in a `fixed` arena and assert `used_bytes()` equals 16 (7 bytes of padding); then allocate `u8, u16, u32, u64` and assert the exact byte count you computed by hand.
- [M] Add `alloc_str(&self, s: &str) -> Option<&str>` that copies bytes into the arena; failure mode: a fixed arena of 8 bytes returns `None` for a 9-byte string without partially writing (check `used_bytes()` unchanged).
- [A] Use the arena as per-request scratch in a toy line-protocol handler: allocate parsed tokens, `scope` per request, 100k requests; acceptance: `chunk_count()` and `used_bytes()` stay bounded after the loop, and `cargo +nightly miri run --example bump_arena_allocator` stays clean for your variant.

**Use it in:** per-request scratch memory and ASTs. Real: bumpalo <https://github.com/fitzgen/bumpalo>.

**Pitfall:** skipped destructors; anything holding a file, lock or `Rc` inside the arena leaks (the recipe's `Rc` assertion shows it).

---

## garbage_collectors
**Prove you got it**
1. Section (b) asserts the incremental collector frees a still-reachable object C when the write barrier is off and keeps it when on. Predict which object ends up white at sweep time and why the barrier must shade C (read `lost_object_schedule`).
2. Section (e) refcounting: acyclic garbage c, d is freed at once but a cycle plus tail leaks (`live_objects() == 3`) until trial deletion reclaims them. Predict `live_objects()` after dropping the root of a 3-cycle with no tail (3) and after trial deletion (0).

**Try next**
- [S] In section (a), assert `live_objects()` after allocating a 5-element cycle and unrooting it: 5 before collection, 0 after, and check that the next allocation reuses a freed slot (the recipe asserts reuse for `z`).
- [M] Break the Cheney collector on purpose: skip the forwarding-pointer check so a shared object is copied twice; use `check_graph` (as the recipe does) to show the graph no longer matches and the live count exceeds 4.
- [A] Implement a mark-compact collector behind the `Heap` trait and run it through the same differential fuzz against the reachability oracle (3 seeds x 30k ops); it must pass `check_invariants` after every cycle.

**Use it in:** scripting-language runtimes. For Rust, see arena-based GCs such as <https://github.com/kyren/gc-arena>.

**Pitfall:** forgetting the write barrier: the recipe's "barrier OFF" runs are caught by the oracle in most seeds.

---

## size_class_allocator
**Prove you got it**
1. The basic test allocates three blocks, frees them, and asserts `(free_blocks, free_bytes) == (1, total)`: coalescing merged everything. Predict `free_blocks` if you free only the first and the third block (2), then the middle (1).
2. The fragmentation test asserts `alloc(2000)` fails when free memory is 1 KiB holes and `external_frag() > 0.9`. Predict the `largest_free` value versus `free_bytes` in that state and why total free space is not enough.

**Try next**
- [S] Assert that every returned pointer is 16-aligned for 1,000 random sizes (the recipe asserts `p % 16 == 0` in places; add the check to the fuzz loop for `realloc` results too).
- [M] Add `calloc(n)` that zero-fills the payload; assert reuse of a block previously filled with `0xAB` returns zeros. Failure mode: `alloc(usize::MAX)` returns `None` (existing) and `calloc(usize::MAX/2, 4)` must detect multiplication overflow.
- [A] Replace the size-class bins with a single address-ordered free list (first-fit) and compare `external_frag()` and `report()` after the recipe's random workload; write down which has lower fragmentation and verify using the recipe's `check()` after every 1000 operations.

**Use it in:** understanding malloc and embedded heaps. Real: dlmalloc <https://github.com/alexcrichton/dlmalloc-rs>.

**Pitfall:** omitting the footer, so freeing a block cannot coalesce backwards (the recipe asserts header == footer).

---

## slab_generational
**Prove you got it**
1. The first block removes `alice`, inserts `carol` and asserts `a.index == c.index`, `a.ver != c.ver` and `slab.get(a) == None`. Predict `slab.get(a)` if `Handle` held only the index (it would return `carol`: the ABA problem the recipe prints).
2. The connection table adds 100, 200, 300, 400 bytes, then +1 each, and `retain(|_, c| c.bytes > 250)`. Predict `left` (`[("10.0.0.3",301),("10.0.0.4",401)]`) and `conns.contains(hs[0])`.

**Try next**
- [S] Remove and reinsert into the same slot 300 times and print `c.ver`; predict its value, then switch `ver` to `u8` and run 300 reuses to observe wraparound; assert what happens to the oldest stale handle.
- [M] Add `get2_mut(a, b) -> Option<(&mut T, &mut T)>` (needs `split_at_mut`); failure mode: `a == b` returns `None`, stale handles return `None`. Assert with the connection table.
- [A] Extend the recipe's model test (50,000 random steps, oracle = `HashMap`) with a `remove_oldest` operation using your own insertion sequence counters; acceptance: no stale handle ever resolves and `check_invariants()` passes after every 1000 steps.

**Use it in:** ECS entity storage and connection tables. Real: <https://github.com/tokio-rs/slab> (no generations) and generational maps like <https://github.com/orlp/slotmap>.

**Pitfall:** a too-small generation counter wrapping around, reviving old handles.

---

## Chapter projects

### 1. Small (~2 h): symbol interner and expression arena
Combine `arena_ids`, `language/newtype_ids`, `language/cow`.
- Milestones: (1) `Interner` with `intern`/`resolve`; (2) arena-allocated tokens pointing at symbols; (3) pretty-printer.
- Acceptance: interning same string twice yields equal ids; 100k random strings round-trip; allocation count roughly one per growth.
- Stretch: `Cow` return in `resolve` for case-folded lookups.

### 2. Intermediate (~weekend): connection manager on a slab
Combine `slab_generational`, `language/drop_raii`, `errors/error_enum`, plus `concurrency/sharded_map`.
- Milestones: (1) single-thread slab of sessions; (2) handles validated on every call; (3) idle timeout sweep using an injected clock; (4) sharded version across 4 threads.
- Acceptance: churn test of 1M ops against a `HashMap` oracle; no stale handle ever resolves; shutdown drops each session exactly once (counter).
- Stretch: generational handles that survive serialisation.

### 3. Advanced (~week): a tiny VM heap with GC
Combine `garbage_collectors`, `bump_arena_allocator`, `size_class_allocator`, `design_patterns_low` (patterns chapter), plus `text_and_parsing/bytecode_vm`.
- Milestones: (1) object model on a slab; (2) mark-sweep with roots from the VM stack; (3) bump nursery with promotion; (4) a tracing mode printing pauses.
- Acceptance: a seeded program generator; after each GC, reachable set from roots equals what a reference interpreter says is live; no use-after-free (handles generational).
- Stretch: compaction with forwarding pointers; compare pause times.
