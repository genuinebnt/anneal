Implement `MyVec<T>` without using `Vec`. It holds a raw buffer you allocate with `std::alloc`, a capacity and a
length. Capacity is **0** for a new vector, becomes **4** on the first push and **doubles** after that (`realloc`).

`push`, `pop`, `insert` and `remove` move elements with raw pointer operations; `truncate` and `clear` drop elements; it
dereferences to `[T]` so slices methods (`iter`, `sort`, indexing) work. Dropping a `MyVec` drops every element once
and frees the buffer. A **zero-sized** `T` must never touch the allocator: its capacity is `usize::MAX`.

Every `unsafe` block gets a `// SAFETY:` comment naming the invariant it relies on. The tests run with ordinary and
panicking-drop types; run your solution under Miri (`cargo +nightly miri test`) as the follow-up exercise.
