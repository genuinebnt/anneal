use solution::*;

#[test]
fn strlen_counts_bytes() {
    check!(r#"e-acute is two bytes"#, (c_strlen("hello"), c_strlen("h\u{e9}llo"), c_strlen("")), (Ok(5), Ok(6), Ok(0)));
}

#[test]
fn interior_nul_is_refused() {
    check!(r#"NulError carries the position"#, c_strlen("a\0b").map_err(|e| e.nul_position()), Err(1));
}

#[test]
fn null_pointer_is_none() {
    check!(r#"no string at all"#, unsafe { from_c(std::ptr::null()) }, None);
}

#[test]
fn read_a_borrowed_c_string() {
    check!(r#"bytes with a terminator"#, unsafe { from_c(b"caf\xc3\xa9\0".as_ptr() as *const std::ffi::c_char) }, Some("caf\u{e9}".to_string()));
}

#[test]
fn greeting_round_trip() {
    check!(r#"make, read, free"#, { let p = make_greeting("Ferris").unwrap(); let s = unsafe { from_c(p) }; unsafe { free_greeting(p) }; s }, Some("hello, Ferris".to_string()));
}

#[test]
fn prefix_of_a_buffer() {
    check!(r#"stops at the first NUL"#, (c_prefix_len(b"abc\0def"), c_prefix_len(b"abc"), c_prefix_len(b"\0"), c_prefix_len(b"")), (Some(3), None, Some(0), None));
}
