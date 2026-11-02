Implement `Arena<T>`, the allocator behind compiler ASTs and graphs. `alloc(&self, value)` stores the value and returns
`&mut T` that lives as long as the **borrow of the arena**, so you may keep thousands of those references at once while
allocating more.

That only works if values never move, so a `Vec<T>` that reallocates is not an option. Keep a list of chunks: allocate
into the last chunk while it has room, else start a new chunk with **double** the previous capacity (the first holds 4).
`len` counts values, `chunk_count` the chunks. Dropping the arena drops every value exactly once and not before.

Write the `unsafe` block with a `// SAFETY:` comment that states the invariant it relies on.
