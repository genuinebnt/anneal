use std::any::Any;
use std::cmp::Ordering;
use std::ffi::{c_int, c_void};
use std::panic::{catch_unwind, resume_unwind, AssertUnwindSafe};

#[cfg(not(target_os = "macos"))]
extern "C" {
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

struct Ctx<'a, T> {
    cmp: &'a mut dyn FnMut(&T, &T) -> Ordering,
    panic: Option<Box<dyn Any + Send>>,
}

/// The `extern "C"` function C calls back. It must never unwind: a panic is caught and parked in the context.
extern "C" fn trampoline<T>(a: *const c_void, b: *const c_void, arg: *mut c_void) -> c_int {
    let ctx = unsafe { &mut *(arg as *mut Ctx<T>) };
    if ctx.panic.is_some() {
        return 0; // already failed: let C finish quickly without running user code again
    }
    let (a, b) = unsafe { (&*(a as *const T), &*(b as *const T)) };
    match catch_unwind(AssertUnwindSafe(|| (ctx.cmp)(a, b))) {
        Ok(order) => order as c_int, // Less = -1, Equal = 0, Greater = 1
        Err(payload) => {
            ctx.panic = Some(payload);
            0
        }
    }
}

pub fn sort_by_c<T: Copy, F: FnMut(&T, &T) -> Ordering>(v: &mut [T], mut cmp: F) {
    let mut ctx = Ctx::<T> {
        cmp: &mut cmp,
        panic: None,
    };
    unsafe {
        qsort_r(
            v.as_mut_ptr() as *mut c_void,
            v.len(),
            std::mem::size_of::<T>(),
            trampoline::<T>,
            &mut ctx as *mut Ctx<T> as *mut c_void,
        );
    }
    if let Some(payload) = ctx.panic {
        resume_unwind(payload); // back on the Rust side of the boundary
    }
}
