Write the C-facing API of a small Rust object, the way a `.so` or `.dll` written in Rust exposes it. `Counter` (a name and
an `i64`) is the Rust type; C only ever holds a `*mut Counter`.

- `counter_new(name)`: allocate on the heap and give the pointer away. A null `name` means `"counter"`; a name that is not
  valid UTF-8 returns null.
- `counter_add`: checked addition, returning a status code. **Overflow leaves the value unchanged.**
- `counter_get`: store the value through an out-pointer.
- `counter_name`: `snprintf`-style copy into the caller's buffer (at most `cap - 1` bytes plus a NUL; nothing if `cap == 0`
  or `buf` is null) returning the **full** name length.
- `counter_free`: take the object back and drop it. Null is a no-op.

Every function must tolerate null handles (returning `ERR_NULL`) instead of crashing. A `drops()` counter is provided so the
tests can check that every handle is dropped exactly once.
