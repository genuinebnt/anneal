use std::cell::Cell;
use std::ffi::{c_char, CStr};

/// Status codes returned to C callers.
pub const OK: i32 = 0;
pub const ERR_NULL: i32 = -1;
pub const ERR_OVERFLOW: i32 = -2;

thread_local! {
    static DROPS: Cell<usize> = const { Cell::new(0) };
}

/// How many `Counter`s have been dropped on this thread (lets tests check that every handle is freed exactly once).
pub fn drops() -> usize {
    DROPS.with(|d| d.get())
}

/// The Rust object behind the handle. C never sees its fields, only a pointer to it.
pub struct Counter {
    name: String,
    value: i64,
}

impl Drop for Counter {
    fn drop(&mut self) {
        DROPS.with(|d| d.set(d.get() + 1));
    }
}

/// Creates a counter named `name` (a C string), starting at 0. A null `name` means the default name "counter".
/// Returns null if the name is not valid UTF-8. The caller owns the result and must pass it to `counter_free`.
pub extern "C" fn counter_new(name: *const c_char) -> *mut Counter {
    todo!()
}

/// Adds `delta`. Returns `OK`, `ERR_NULL` for a null handle, or `ERR_OVERFLOW` (leaving the value unchanged) if the sum
/// does not fit in an `i64`.
///
/// # Safety
/// `c` is null or a live handle from `counter_new`.
pub unsafe extern "C" fn counter_add(c: *mut Counter, delta: i64) -> i32 {
    todo!()
}

/// Writes the current value to `*out`. Returns `OK`, or `ERR_NULL` if `c` or `out` is null.
///
/// # Safety
/// `c` is null or a live handle; `out` is null or valid for a write.
pub unsafe extern "C" fn counter_get(c: *const Counter, out: *mut i64) -> i32 {
    todo!()
}

/// Copies the name into the caller's buffer `buf` of capacity `cap`, `snprintf`-style: at most `cap - 1` bytes plus a NUL
/// (nothing at all if `cap` is 0 or `buf` is null). Returns the **full** length of the name in bytes (so the caller can detect
/// truncation), or `ERR_NULL as isize` for a null handle.
///
/// # Safety
/// `c` is null or a live handle; `buf` is null or valid for `cap` bytes.
pub unsafe extern "C" fn counter_name(c: *const Counter, buf: *mut c_char, cap: usize) -> isize {
    todo!()
}

/// Destroys the counter. Null is allowed and does nothing.
///
/// # Safety
/// `c` is null or a handle from `counter_new` that has not been freed yet.
pub unsafe extern "C" fn counter_free(c: *mut Counter) {
    todo!()
}
