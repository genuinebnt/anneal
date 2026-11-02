Use libc's `qsort_r` (declared for you) to implement `sort_by_c(v, cmp)`: sort any slice of `Copy` values with a Rust
closure as the comparison, like `slice::sort_by` but powered by C.

`qsort_r` takes a function pointer `extern "C" fn(*const c_void, *const c_void, *mut c_void) -> c_int` and a `void *`
context. Write a generic trampoline that recovers the closure from the context, calls it on the two elements and returns
`-1 / 0 / 1`.

If the closure **panics**, the panic must not cross the C boundary. Catch it, do not call the closure again for the rest of
the sort, let `qsort_r` return, and then re-raise the same panic payload in the caller.
