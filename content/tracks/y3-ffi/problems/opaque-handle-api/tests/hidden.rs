use solution::*;

#[test]
fn truncation_is_snprintf_style() {
    check!(r#"cap 4 for a 5-byte name"#, unsafe { let name = std::ffi::CString::new("hello").unwrap(); let c = counter_new(name.as_ptr()); let mut buf = [0x7Fu8; 8]; let n = counter_name(c, buf.as_mut_ptr() as *mut std::ffi::c_char, 4); counter_free(c); (n, buf.to_vec()) }, (5, vec![b'h', b'e', b'l', 0, 0x7F, 0x7F, 0x7F, 0x7F]));
}

#[test]
fn capacity_zero_writes_nothing() {
    check!(r#"size query"#, unsafe { let name = std::ffi::CString::new("hello").unwrap(); let c = counter_new(name.as_ptr()); let mut buf = [0x7Fu8; 4]; let n = counter_name(c, buf.as_mut_ptr() as *mut std::ffi::c_char, 0); let m = counter_name(c, std::ptr::null_mut(), 0); counter_free(c); (n, m, buf.to_vec()) }, (5, 5, vec![0x7F; 4]));
}

#[test]
fn exact_fit_and_one_short() {
    check!(r#"cap = len + 1 and cap = len"#, unsafe { let name = std::ffi::CString::new("abc").unwrap(); let c = counter_new(name.as_ptr()); let mut a = [0x7Fu8; 4]; let mut b = [0x7Fu8; 4]; counter_name(c, a.as_mut_ptr() as *mut std::ffi::c_char, 4); counter_name(c, b.as_mut_ptr() as *mut std::ffi::c_char, 3); counter_free(c); (a.to_vec(), b.to_vec()) }, (vec![b'a', b'b', b'c', 0], vec![b'a', b'b', 0, 0x7F]));
}

#[test]
fn invalid_utf8_name_gives_null() {
    check!(r#"0xFF is not UTF-8"#, { let name = std::ffi::CString::new(vec![0xFFu8, b'x']).unwrap(); counter_new(name.as_ptr()).is_null() }, true);
}

#[test]
fn unicode_names_survive() {
    check!(r#"multi-byte name"#, unsafe { let name = std::ffi::CString::new("caf\u{e9}").unwrap(); let c = counter_new(name.as_ptr()); let mut buf = [0u8; 16]; let n = counter_name(c, buf.as_mut_ptr() as *mut std::ffi::c_char, buf.len()); counter_free(c); (n, buf[..6].to_vec()) }, (5, "caf\u{e9}\0".as_bytes().to_vec()));
}

#[test]
fn negative_overflow_too() {
    check!(r#"i64::MIN - 1"#, unsafe { let c = counter_new(std::ptr::null()); counter_add(c, i64::MIN); let st = counter_add(c, -1); let mut out = 0i64; counter_get(c, &mut out); counter_free(c); (st, out) }, (ERR_OVERFLOW, i64::MIN));
}

#[test]
fn null_out_pointer() {
    check!(r#"get with nowhere to write"#, unsafe { let c = counter_new(std::ptr::null()); let st = counter_get(c, std::ptr::null_mut()); counter_free(c); st }, ERR_NULL);
}

#[test]
fn handles_are_independent() {
    check!(r#"two counters"#, unsafe { let a = counter_new(std::ptr::null()); let b = counter_new(std::ptr::null()); counter_add(a, 1); counter_add(b, 10); let (mut x, mut y) = (0i64, 0i64); counter_get(a, &mut x); counter_get(b, &mut y); counter_free(a); counter_free(b); (x, y) }, (1, 10));
}

#[test]
fn every_handle_is_dropped_exactly_once() {
    check!(r#"free runs Drop, free(null) does not"#, unsafe { let base = drops(); let a = counter_new(std::ptr::null()); let b = counter_new(std::ptr::null()); counter_free(std::ptr::null_mut()); let none = drops() - base; counter_free(a); let one = drops() - base; counter_free(b); (none, one, drops() - base) }, (0, 1, 2));
}

#[test]
fn a_failed_new_does_not_allocate_a_handle_to_drop() {
    check!(r#"null result, nothing to free"#, unsafe { let base = drops(); let name = std::ffi::CString::new(vec![0xFFu8]).unwrap(); let c = counter_new(name.as_ptr()); counter_free(c); drops() - base }, 0);
}
