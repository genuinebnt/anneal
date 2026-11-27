A library function often needs "the current something": the request it is serving, the database transaction, the logger. Passing it through every call is noisy. So the thread keeps it, and anyone running on that thread can ask for it.

Something that is `Send + Sync` and implements `Context` can be made **current** for a while:

- `enter(context)` makes it current on **this thread** and returns a guard, `Entered`.
- `current()` returns the context of the **innermost guard that is still alive** on this thread, or `None`. It is the very `Arc` that was passed to `enter`.
- `with_current(f)` calls `f` with the current context, if any, and returns what `f` returns.

Rules:

- **Each thread** has its own current context. A new thread starts with none.
- Guards can be dropped **in any order**. Dropping an outer guard while an inner one is alive leaves `current()` unchanged. When the inner one is dropped later, the outer context does **not** come back.
- A guard dropped because of a panic works like any other.
- Dropping a guard releases its `Arc` at once.
- The context's **destructor may call** `current()`, `enter` and drop guards. It sees the state without the guard that is being dropped.
- Inside `with_current`, `f` may call `enter`, `current` and `with_current`, and may drop guards, including the one for the context it was given. The context it was given stays usable until `f` returns.

None of these may panic.
