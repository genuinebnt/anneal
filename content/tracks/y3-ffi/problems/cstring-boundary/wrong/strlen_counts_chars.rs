use std::ffi::{c_char, CStr, CString, NulError};

extern "C" {
    fn strlen(s: *const c_char) -> usize;
}

pub fn c_strlen(s: &str) -> Result<usize, NulError> {
    let c = CString::new(s)?;
    let _ = unsafe { strlen(c.as_ptr()) };
    Ok(s.chars().count())
}

pub unsafe fn from_c(p: *const c_char) -> Option<String> {
    if p.is_null() {
        return None;
    }
    Some(CStr::from_ptr(p).to_string_lossy().into_owned())
}

pub fn make_greeting(name: &str) -> Result<*mut c_char, NulError> {
    Ok(CString::new(format!("hello, {name}"))?.into_raw())
}

pub unsafe fn free_greeting(p: *mut c_char) {
    if !p.is_null() {
        drop(CString::from_raw(p)); // the allocator that made it is the one that frees it
    }
}

pub fn c_prefix_len(buf: &[u8]) -> Option<usize> {
    CStr::from_bytes_until_nul(buf).ok().map(|c| c.to_bytes().len())
}
