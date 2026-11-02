Implement `Slab<T>`: a `Vec` of slots addressed by `Key { idx, gen }`.

`insert` reuses the most recently freed slot (LIFO) or appends a new one. `remove` returns the value and bumps the
slot's generation. Every `Key` handed out earlier for that slot is now **stale**: `get`, `get_mut` and `remove` must
return `None` for it, **even after the slot has been reused** by a different value. `len` counts live values,
`slots` counts slots ever allocated (a churn workload must not grow it), and `iter` yields live entries in slot order.
