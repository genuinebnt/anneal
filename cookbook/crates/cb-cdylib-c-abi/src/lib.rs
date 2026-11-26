//! Stable C ABI. Rules applied throughout:
//! 1. only `repr(C)`/primitive/pointer types cross the boundary (no String, Vec, &str, Result);
//! 2. every entry point catches panics (unwinding into C is undefined behaviour);
//! 3. errors are integer codes + a thread-local message;
//! 4. whoever allocates frees (Rust-allocated memory has a Rust free function).
use std::cell::RefCell;
use std::ffi::{CStr, CString, c_char, c_int, c_void};
use std::panic::{AssertUnwindSafe, catch_unwind};

pub const CB_ABI_VERSION: u32 = 1;
pub const CB_OK: c_int = 0;
pub const CB_ERR_NULL: c_int = -1;
pub const CB_ERR_OVERFLOW: c_int = -2;
pub const CB_ERR_BUFFER_TOO_SMALL: c_int = -3;
pub const CB_ERR_INVALID_UTF8: c_int = -4;
pub const CB_ERR_PANIC: c_int = -5;

thread_local! { static LAST_ERROR: RefCell<Option<CString>> = const { RefCell::new(None) }; }

fn set_error(code: c_int, msg: &str) -> c_int {
    // Interior NULs would make CString::new fail; replace them.
    let c = CString::new(msg.replace('\0', "?")).unwrap();
    LAST_ERROR.with(|e| *e.borrow_mut() = Some(c));
    code
}

/// Run `f`, converting a panic into CB_ERR_PANIC.
fn guard(f: impl FnOnce() -> c_int) -> c_int {
    catch_unwind(AssertUnwindSafe(f)).unwrap_or_else(|p| {
        let m = p
            .downcast_ref::<&str>()
            .map(|s| s.to_string())
            .or_else(|| p.downcast_ref::<String>().cloned())
            .unwrap_or_else(|| "panic".into());
        set_error(CB_ERR_PANIC, &format!("panic: {m}"))
    })
}

#[unsafe(no_mangle)]
pub extern "C" fn cb_abi_version() -> u32 {
    CB_ABI_VERSION
}

/// Opaque to C: C only ever sees `struct cb_counter *`.
pub struct Counter {
    value: i64,
    history: Vec<i64>,
}

#[unsafe(no_mangle)]
pub extern "C" fn cb_counter_new(start: i64) -> *mut Counter {
    Box::into_raw(Box::new(Counter {
        value: start,
        history: vec![start],
    }))
}

/// # Safety
/// `c` must be NULL or a pointer from `cb_counter_new` not yet freed.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn cb_counter_free(c: *mut Counter) {
    if !c.is_null() {
        drop(unsafe { Box::from_raw(c) });
    }
}

/// # Safety
/// `c` must be NULL or a valid live counter with no concurrent use.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn cb_counter_add(c: *mut Counter, delta: i64) -> c_int {
    guard(|| {
        let Some(c) = (unsafe { c.as_mut() }) else {
            return set_error(CB_ERR_NULL, "counter is NULL");
        };
        match c.value.checked_add(delta) {
            Some(v) => {
                c.value = v;
                c.history.push(v);
                CB_OK
            }
            None => set_error(CB_ERR_OVERFLOW, "counter overflow"),
        }
    })
}

/// # Safety
/// `c` NULL or valid; `out` NULL or writable.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn cb_counter_get(c: *const Counter, out: *mut i64) -> c_int {
    guard(|| match (unsafe { c.as_ref() }, unsafe { out.as_mut() }) {
        (Some(c), Some(out)) => {
            *out = c.value;
            CB_OK
        }
        _ => set_error(CB_ERR_NULL, "NULL argument"),
    })
}

pub type VisitFn = extern "C" fn(i64, *mut c_void);

/// # Safety
/// `c` valid; `f` a valid function; `user_data` whatever `f` expects.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn cb_counter_history_each(
    c: *const Counter,
    f: Option<VisitFn>,
    user_data: *mut c_void,
) -> c_int {
    guard(|| {
        let (Some(c), Some(f)) = (unsafe { c.as_ref() }, f) else {
            return set_error(CB_ERR_NULL, "NULL argument");
        };
        for &v in &c.history {
            f(v, user_data);
        }
        CB_OK
    })
}

/// # Safety
/// `name` NULL or a NUL-terminated string. Returns NULL on error.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn cb_greet(name: *const c_char) -> *mut c_char {
    let mut out: *mut c_char = std::ptr::null_mut();
    guard(|| {
        if name.is_null() {
            return set_error(CB_ERR_NULL, "name is NULL");
        }
        match unsafe { CStr::from_ptr(name) }.to_str() {
            Ok(n) => {
                out = CString::new(format!("Hello, {n}!")).unwrap().into_raw();
                CB_OK
            }
            Err(_) => set_error(CB_ERR_INVALID_UTF8, "name is not UTF-8"),
        }
    });
    out
}

/// # Safety
/// `s` NULL or a pointer returned by this library, freed at most once.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn cb_string_free(s: *mut c_char) {
    if !s.is_null() {
        drop(unsafe { CString::from_raw(s) });
    }
}

/// # Safety
/// `buf` valid for `cap` bytes (or NULL when cap == 0); `written` writable.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn cb_fill_buffer(buf: *mut u8, cap: usize, written: *mut usize) -> c_int {
    guard(|| {
        const DATA: &[u8] = b"rust-owned-data";
        let Some(w) = (unsafe { written.as_mut() }) else {
            return set_error(CB_ERR_NULL, "written is NULL");
        };
        *w = DATA.len();
        if cap < DATA.len() {
            return set_error(CB_ERR_BUFFER_TOO_SMALL, "buffer too small");
        }
        if buf.is_null() {
            return set_error(CB_ERR_NULL, "buf is NULL");
        }
        unsafe { std::ptr::copy_nonoverlapping(DATA.as_ptr(), buf, DATA.len()) };
        CB_OK
    })
}

#[unsafe(no_mangle)]
pub extern "C" fn cb_last_error_message() -> *const c_char {
    LAST_ERROR.with(|e| e.borrow().as_ref().map_or(std::ptr::null(), |c| c.as_ptr()))
}

#[unsafe(no_mangle)]
pub extern "C" fn cb_test_panic() -> c_int {
    guard(|| panic!("deliberate panic"))
}

#[repr(C)]
pub struct Point {
    pub struct_size: u32,
    pub x: i32,
    pub y: i32,
}
// Layout is part of the ABI: fail the BUILD if it ever changes by accident.
const _: () = assert!(size_of::<Point>() == 12 && align_of::<Point>() == 4);

/// # Safety
/// `p` NULL or writable for `size_of::<Point>()` bytes.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn cb_point_init(p: *mut Point, x: i32, y: i32) -> c_int {
    guard(|| match unsafe { p.as_mut() } {
        Some(p) => {
            *p = Point {
                struct_size: size_of::<Point>() as u32,
                x,
                y,
            };
            CB_OK
        }
        None => set_error(CB_ERR_NULL, "p is NULL"),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn rust_side_roundtrip() {
        unsafe {
            let c = cb_counter_new(i64::MAX - 1);
            assert_eq!(cb_counter_add(c, 1), CB_OK);
            assert_eq!(cb_counter_add(c, 1), CB_ERR_OVERFLOW);
            assert_eq!(cb_counter_add(std::ptr::null_mut(), 1), CB_ERR_NULL);
            cb_counter_free(c);
        }
        assert_eq!(cb_test_panic(), CB_ERR_PANIC);
    }
}
