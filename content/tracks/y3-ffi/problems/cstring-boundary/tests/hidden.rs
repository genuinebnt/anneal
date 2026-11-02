use solution::*;

#[test]
fn invalid_utf8_is_replaced() {
    check!(r#"0xFF is not UTF-8"#, unsafe { from_c(b"a\xffb\0".as_ptr() as *const std::ffi::c_char) }, Some("a\u{fffd}b".to_string()));
}

#[test]
fn greeting_rejects_a_nul_in_the_name() {
    check!(r#"cannot be a C string"#, make_greeting("a\0b").is_err(), true);
}

#[test]
fn free_null_is_a_no_op() {
    check!(r#"must not crash"#, { unsafe { free_greeting(std::ptr::null_mut()) }; true }, true);
}

#[test]
fn greetings_are_independent() {
    check!(r#"two live at once"#, { let a = make_greeting("A").unwrap(); let b = make_greeting("BB").unwrap(); let r = unsafe { (from_c(a), from_c(b)) }; unsafe { free_greeting(a); free_greeting(b); } r }, (Some("hello, A".to_string()), Some("hello, BB".to_string())));
}

#[test]
fn strlen_long_string() {
    check!(r#"a megabyte"#, c_strlen(&"x".repeat(1_000_000)), Ok(1_000_000));
}

#[test]
fn strlen_stops_nowhere_early() {
    check!(r#"all multi-byte"#, c_strlen("\u{1F980}\u{1F980}"), Ok(8));
}

#[test]
fn prefix_ignores_what_follows_the_first_nul() {
    check!(r#"two strings in one buffer"#, (c_prefix_len(b"ab\0cd\0"), c_prefix_len(b"\0\0")), (Some(2), Some(0)));
}

#[test]
fn prefix_of_a_buffer_without_terminator_but_with_nul_later_is_not_found() {
    check!(r#"no NUL at all"#, c_prefix_len(&[1u8, 2, 3, 4]), None);
}

#[test]
fn greeting_with_unicode() {
    check!(r#"UTF-8 passes through"#, { let p = make_greeting("\u{e9}").unwrap(); let s = unsafe { from_c(p) }; unsafe { free_greeting(p) }; s }, Some("hello, \u{e9}".to_string()));
}
