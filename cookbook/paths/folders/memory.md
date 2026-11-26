# Chapter `memory`: suggested reading order

Ordered simple -> intermediate -> advanced (then alphabetical). Skim `Use in:` and `COMMON USES` in each header first. Cross-chapter order is in [the tracks](../tracks/).

| # | Recipe | Level | What it is |
|---|---|---|---|
| 1 | [`arena_ids`](../../examples/memory/arena_ids.rs) | simple | arena of nodes addressed by typed ids (no Box per node). |
| 2 | [`bump_arena_allocator`](../../examples/memory/bump_arena_allocator.rs) | advanced | a bump (arena) allocator: alignment, checkpoints, drop glue, chunked growth. |
| 3 | [`garbage_collectors`](../../examples/memory/garbage_collectors.rs) | advanced | garbage collection algorithms on a simulated managed heap, checked by a reachability oracle. |
| 4 | [`size_class_allocator`](../../examples/memory/size_class_allocator.rs) | advanced | a toy general-purpose allocator: boundary tags, size-class free lists, splitting, coalescing, realloc. |
| 5 | [`slab_generational`](../../examples/memory/slab_generational.rs) | advanced | a slab / object pool with a free list through vacant slots and GENERATIONAL typed handles. |
