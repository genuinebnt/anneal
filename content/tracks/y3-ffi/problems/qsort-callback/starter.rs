use std::cmp::Ordering;
use std::ffi::{c_int, c_void};

#[cfg(not(target_os = "macos"))]
extern "C" {
    /// glibc's re-entrant `qsort`: sorts `nmemb` elements of `size` bytes, calling `compar(a, b, arg)`.
    fn qsort_r(
        base: *mut c_void,
        nmemb: usize,
        size: usize,
        compar: extern "C" fn(*const c_void, *const c_void, *mut c_void) -> c_int,
        arg: *mut c_void,
    );
}

/// macOS also has a `qsort_r`, but it takes `arg` before the function and passes it to the callback first. This
/// adapter gives it glibc's shape, so the code you write is the same on both. You don't need to change it.
#[cfg(target_os = "macos")]
unsafe fn qsort_r(
    base: *mut c_void,
    nmemb: usize,
    size: usize,
    compar: extern "C" fn(*const c_void, *const c_void, *mut c_void) -> c_int,
    arg: *mut c_void,
) {
    struct Call {
        compar: extern "C" fn(*const c_void, *const c_void, *mut c_void) -> c_int,
        arg: *mut c_void,
    }
    extern "C" fn reorder(call: *mut c_void, a: *const c_void, b: *const c_void) -> c_int {
        let call = unsafe { &*(call as *const Call) };
        (call.compar)(a, b, call.arg)
    }
    extern "C" {
        #[link_name = "qsort_r"]
        fn macos_qsort_r(
            base: *mut c_void,
            nmemb: usize,
            size: usize,
            call: *mut c_void,
            compar: extern "C" fn(*mut c_void, *const c_void, *const c_void) -> c_int,
        );
    }
    if size == 0 {
        return; // macOS crashes on zero-sized elements, and there's nothing to move anyway
    }
    let mut call = Call { compar, arg };
    unsafe {
        macos_qsort_r(
            base,
            nmemb,
            size,
            &mut call as *mut Call as *mut c_void,
            reorder,
        )
    }
}

/// Sorts `v` with C's `qsort_r`, ordering elements with the closure `cmp`.
///
/// If `cmp` panics, the panic must not unwind through C (that is undefined behaviour): catch it, stop calling `cmp`
/// for the rest of the sort, let `qsort_r` finish, and then re-raise the same panic to the caller.
pub fn sort_by_c<T: Copy, F: FnMut(&T, &T) -> Ordering>(v: &mut [T], cmp: F) {
    todo!()
}
