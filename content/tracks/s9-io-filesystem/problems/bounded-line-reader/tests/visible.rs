use solution::*;

use std::io::BufReader;

#[allow(dead_code)]
fn all(input: &[u8], max_len: usize, buffer: usize) -> Vec<Result<String, LineError>> {
    BoundedLines::new(BufReader::with_capacity(buffer, input), max_len).take(10_000).collect()
}

#[allow(dead_code)]
fn ok(s: &str) -> Result<String, LineError> {
    Ok(s.to_string())
}

#[test]
fn plain_lines() {
    check!(r#""ab\ncd\r\n\nlast""#, all(b"ab\ncd\r\n\nlast", 10, 64), vec![ok("ab"), ok("cd"), ok(""), ok("last")]);
}

#[test]
fn ends_with_newline() {
    check!(r#""a\n""#, all(b"a\n", 10, 64), vec![ok("a")]);
    check!("empty input", all(b"", 10, 64), vec![]);
}

#[test]
fn long_line_is_skipped() {
    check!(r#""ok\ntoolongline\nnext\n", max 5"#, all(b"ok\ntoolongline\nnext\n", 5, 4), vec![ok("ok"), Err(LineError::TooLong { line: 2 }), ok("next")]);
}

#[test]
fn the_limit_does_not_count_the_line_ending() {
    check!(r#""12345\r\n123456\n", max 5"#, all(b"12345\r\n123456\n", 5, 3), vec![ok("12345"), Err(LineError::TooLong { line: 2 })]);
}

#[test]
fn invalid_utf8_does_not_stop_the_stream() {
    check!("an invalid second line", all(b"a\n\xff\xfe\nb", 10, 64), vec![ok("a"), Err(LineError::NotUtf8 { line: 2 }), ok("b")]);
}
