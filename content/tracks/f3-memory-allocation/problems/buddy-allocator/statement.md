Implement a buddy allocator over `2^max_order` abstract units (offsets, not real memory).

`alloc(size)` rounds `size` up to a power of two (0 counts as 1), takes the **lowest-addressed** free block of the
smallest sufficient order, splits it down to the wanted order (keeping the lower half, freeing the upper half at each
level) and returns its offset, or `None`. `free(offset, size)` returns a block and **coalesces**: while the block's
buddy (`offset ^ block_size`) is free, merge them into the next order. `free_units` and `largest_free` report the state.

Offsets of an order-`k` block are always multiples of `2^k`, and live blocks never overlap.
