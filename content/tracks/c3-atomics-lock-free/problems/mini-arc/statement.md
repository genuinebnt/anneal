Write `MiniArc<T>`: `new`, `Clone`, `Deref`, `Drop`, plus the associated functions `strong_count`, `ptr_eq`,
`get_mut` (only while unique), `try_unwrap` (move the value out if unique, else give the `MiniArc` back) and
`make_mut` (clone-on-write). Allocate one block holding the count and the value, and free it when the last
owner is dropped: the value's destructor must run **exactly once**, never early, never twice.

The count is shared by threads, so `clone` and `drop` are atomic. Think about which ordering each one needs:
the thread that frees must see every write made by every other owner before it dropped its handle.
