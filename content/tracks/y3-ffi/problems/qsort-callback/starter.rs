use std::cmp::Ordering;
use std::ffi::{c_int, c_void};

extern "C" {
    /// glibc's re-entrant `qsort`: sorts `nmemb` elements of `size` bytes, calling `compar(a, b, arg)`.
    fn qsort_r(base: *mut c_void, nmemb: usize, size: usize, compar: extern "C" fn(*const c_void, *const c_void, *mut c_void) -> c_int, arg: *mut c_void);
}

/// Sorts `v` with C's `qsort_r`, ordering elements with the closure `cmp`.
///
/// If `cmp` panics, the panic must not unwind through C (that is undefined behaviour): catch it, stop calling `cmp`
/// for the rest of the sort, let `qsort_r` finish, and then re-raise the same panic to the caller.
pub fn sort_by_c<T: Copy, F: FnMut(&T, &T) -> Ordering>(v: &mut [T], cmp: F) {
    todo!()
}
