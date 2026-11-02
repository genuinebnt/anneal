use solution::*;

#[test]
fn new_add_get_free() {
    check!(r#"the happy path"#, unsafe { let c = counter_new(std::ptr::null()); counter_add(c, 5); counter_add(c, -2); let mut out = 0i64; let st = counter_get(c, &mut out); counter_free(c); (st, out) }, (OK, 3));
}

#[test]
fn null_handles_are_errors() {
    check!(r#"no crash"#, unsafe { let mut out = 0i64; (counter_add(std::ptr::null_mut(), 1), counter_get(std::ptr::null(), &mut out), counter_name(std::ptr::null(), std::ptr::null_mut(), 0)) }, (ERR_NULL, ERR_NULL, ERR_NULL as isize));
}

#[test]
fn overflow_is_reported_and_harmless() {
    check!(r#"i64::MAX + 1"#, unsafe { let c = counter_new(std::ptr::null()); counter_add(c, i64::MAX); let st = counter_add(c, 1); let mut out = 0i64; counter_get(c, &mut out); counter_free(c); (st, out) }, (ERR_OVERFLOW, i64::MAX));
}

#[test]
fn default_name_is_copied_out() {
    check!(r#"the caller's buffer"#, unsafe { let c = counter_new(std::ptr::null()); let mut buf = [0x7Fu8; 16]; let n = counter_name(c, buf.as_mut_ptr() as *mut std::ffi::c_char, buf.len()); counter_free(c); (n, buf[..8].to_vec()) }, (7, b"counter\0".to_vec()));
}

#[test]
fn free_null_is_fine() {
    check!(r#"no crash"#, { unsafe { counter_free(std::ptr::null_mut()) }; true }, true);
}
