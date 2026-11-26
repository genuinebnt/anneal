# Memory Management

Arenas, slabs, allocators and buffer pools.

Run any recipe from the `cookbook/` directory: `cargo run --release --example <name>` (the name is the file name without `.rs`). Each file starts with a header: what it shows, `LEVEL`, `COMMON USES` and `Read:` links.

5 examples.

| Example | What it shows | Level | Common uses | Crates |
|---|---|---|---|---|
| [`arena_ids`](arena_ids.rs) | arena of nodes addressed by typed ids (no Box per node) | simple | compiler ASTs, game entities, graph nodes: allocate in one Vec and refer by index (oxc, rustc use arenas). | std only |
| [`bump_arena_allocator`](bump_arena_allocator.rs) | a bump (arena) allocator: alignment, checkpoints, drop glue, chunked growth | advanced | compiler front ends (parse the file, build the AST in an arena, drop it all at once), per-frame/per-request | std only |
| [`slab_generational`](slab_generational.rs) | a slab / object pool with a free list through vacant slots and GENERATIONAL typed handles | advanced | entity-component systems in game engines (the `slotmap` and `generational-arena` crates implement this idea; | std only |
| [`size_class_allocator`](size_class_allocator.rs) | a toy general-purpose allocator: boundary tags, size-class free lists, splitting, coalescing, realloc | advanced | the concepts behind malloc in libc, embedded/no_std heaps (linked-list or TLSF allocators), game-engine | std only |
| [`garbage_collectors`](garbage_collectors.rs) | garbage collection algorithms on a simulated managed heap, checked by a reachability oracle | advanced | the JVM's generational collectors (nursery copying, old-gen mark/compact, card-marking | std only |
