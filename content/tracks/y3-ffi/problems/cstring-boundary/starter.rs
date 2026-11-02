use std::ffi::{c_char, CStr, CString, NulError};

extern "C" {
    fn strlen(s: *const c_char) -> usize;
}

/// The length in **bytes** of `s` as C's `strlen` sees it. Fails if `s` contains an interior NUL byte.
pub fn c_strlen(s: &str) -> Result<usize, NulError> {
    todo!()
}

/// Reads a C string owned by someone else. `None` for a null pointer; invalid UTF-8 is replaced with U+FFFD.
///
/// # Safety
/// `p` is null or points to a NUL-terminated string that stays valid for the call.
pub unsafe fn from_c(p: *const c_char) -> Option<String> {
    todo!()
}

/// Builds `"hello, {name}"` as a C string and hands ownership to the caller (who must give it back to `free_greeting`).
pub fn make_greeting(name: &str) -> Result<*mut c_char, NulError> {
    todo!()
}

/// Takes back a pointer from `make_greeting` and frees it. Null is allowed and does nothing.
///
/// # Safety
/// `p` is null or came from `make_greeting` and has not been freed.
pub unsafe fn free_greeting(p: *mut c_char) {
    todo!()
}

/// The length of the C string at the start of `buf` (up to its first NUL), or `None` if `buf` has no NUL.
pub fn c_prefix_len(buf: &[u8]) -> Option<usize> {
    todo!()
}
