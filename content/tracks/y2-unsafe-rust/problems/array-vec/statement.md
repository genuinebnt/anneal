Implement `ArrayVec<T, N>`: up to `N` elements stored **inline**, in `[MaybeUninit<T>; N]` plus a length. It is what
`smallvec`, `arrayvec` and embedded code use when a heap allocation per small list is too expensive.

`try_push` and `try_insert` give the value back (`Err(value)`) when full instead of allocating or panicking.
`pop`, `remove` (order kept), `swap_remove` (O(1), order not kept), `as_slice`, `Deref` to `[T]`, and `into_array`
(`Ok([T; N])` if exactly full, else `Err(self)`) round it out. Dropping the vector drops exactly the live elements.
