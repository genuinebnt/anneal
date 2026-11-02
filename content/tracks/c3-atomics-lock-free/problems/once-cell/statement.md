Write `Once<T>`, the core of `std::sync::OnceLock`. `get_or_init(&self, f)` runs `f` in exactly one of the callers
that race on it; the others wait and every caller gets the same `&T`. `get` returns `None` until a value is stored.

- **Panics:** if `f` panics, the cell returns to empty and the panic keeps going; another caller may initialize it.
- **Drop:** a stored value is dropped exactly once, when the `Once` is dropped, or moved out by `into_inner`.
- **Threads:** `Once<T>` is `Sync` when `T: Send + Sync`, and `Send` when `T: Send`. Nothing else is allowed.

Use one `AtomicU8` for the state and `UnsafeCell<MaybeUninit<T>>` for the value. Waiting callers may
`yield_now()` in a loop.
