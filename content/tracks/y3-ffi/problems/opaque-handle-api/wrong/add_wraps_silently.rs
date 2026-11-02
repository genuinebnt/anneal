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

pub extern "C" fn counter_new(name: *const c_char) -> *mut Counter {
    let name = if name.is_null() {
        "counter".to_string()
    } else {
        match unsafe { CStr::from_ptr(name) }.to_str() {
            Ok(s) => s.to_string(),
            Err(_) => return std::ptr::null_mut(),
        }
    };
    Box::into_raw(Box::new(Counter { name, value: 0 })) // ownership moves to the caller
}

pub unsafe extern "C" fn counter_add(c: *mut Counter, delta: i64) -> i32 {
    let Some(c) = c.as_mut() else {
        return ERR_NULL;
    };
    c.value = c.value.wrapping_add(delta);
    OK
}

pub unsafe extern "C" fn counter_get(c: *const Counter, out: *mut i64) -> i32 {
    if c.is_null() || out.is_null() {
        return ERR_NULL;
    }
    *out = (*c).value;
    OK
}

pub unsafe extern "C" fn counter_name(c: *const Counter, buf: *mut c_char, cap: usize) -> isize {
    let Some(c) = c.as_ref() else {
        return ERR_NULL as isize;
    };
    let bytes = c.name.as_bytes();
    if !buf.is_null() && cap > 0 {
        let n = bytes.len().min(cap - 1);
        std::ptr::copy_nonoverlapping(bytes.as_ptr(), buf as *mut u8, n);
        *buf.add(n) = 0;
    }
    bytes.len() as isize
}

pub unsafe extern "C" fn counter_free(c: *mut Counter) {
    if !c.is_null() {
        drop(Box::from_raw(c)); // take ownership back and let Drop run exactly once
    }
}
