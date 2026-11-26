use solution::*;

use std::io::{self, BufRead, BufReader, Read};

#[allow(dead_code)]
fn all(input: &[u8], max_len: usize, buffer: usize) -> Vec<Result<String, LineError>> {
    BoundedLines::new(BufReader::with_capacity(buffer, input), max_len).take(10_000).collect()
}

#[allow(dead_code)]
fn ok(s: &str) -> Result<String, LineError> {
    Ok(s.to_string())
}

/// Plays a script of reads: data, or an error of a given kind.
struct Script(std::collections::VecDeque<Result<&'static [u8], io::ErrorKind>>);

impl Read for Script {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        match self.0.pop_front() {
            None => Ok(0),
            Some(Err(kind)) => Err(kind.into()),
            Some(Ok(data)) => {
                buf[..data.len()].copy_from_slice(data);
                Ok(data.len())
            }
        }
    }
}

#[allow(dead_code)]
fn scripted(steps: Vec<Result<&'static [u8], io::ErrorKind>>) -> Vec<Result<String, LineError>> {
    BoundedLines::new(BufReader::new(Script(steps.into())), 100).take(100).collect()
}

#[test]
fn interrupted_is_retried() {
    use io::ErrorKind::Interrupted;
    check!("Interrupted before, inside and after a line", scripted(vec![Err(Interrupted), Ok(b"he"), Err(Interrupted), Ok(b"llo\n"), Err(Interrupted)]), vec![ok("hello")]);
}

#[test]
fn other_errors_end_the_iteration() {
    check!(
        "a line, half a line, a reset, then more data",
        scripted(vec![Ok(b"one\ntw"), Err(io::ErrorKind::ConnectionReset), Ok(b"o\nthree\n")]),
        vec![ok("one"), Err(LineError::Io(io::ErrorKind::ConnectionReset))]
    );
}

#[test]
fn stays_finished() {
    let mut lines = BoundedLines::new(&b"x"[..], 5);
    check!("next four times", (lines.next(), lines.next(), lines.next(), lines.next()), (Some(ok("x")), None, None, None));
}

#[test]
fn carriage_return_without_newline_is_data() {
    check!(r#""a\r" at the end, "b\r\r\n""#, all(b"b\r\r\na\r", 10, 64), vec![ok("b\r"), ok("a\r")]);
    check!(r#""12345\r" at the end with max 5 is too long"#, all(b"12345\r", 5, 64), vec![Err(LineError::TooLong { line: 1 })]);
}

#[test]
fn crlf_split_across_buffers() {
    check!("one-byte buffer", all(b"abc\r\ndef\r\n", 3, 1), vec![ok("abc"), ok("def")]);
}

#[test]
fn multibyte_characters_split_across_buffers() {
    check!("é and 漢 with a 2-byte buffer", all("héllo\n漢字\n".as_bytes(), 20, 2), vec![ok("héllo"), ok("漢字")]);
}

#[test]
fn the_limit_is_in_bytes() {
    check!("two 3-byte characters, max 5", all("漢字".as_bytes(), 5, 64), vec![Err(LineError::TooLong { line: 1 })]);
}

#[test]
fn zero_limit() {
    check!("max 0", all(b"\n\r\nx\n\n", 0, 2), vec![ok(""), ok(""), Err(LineError::TooLong { line: 3 }), ok("")]);
}

#[test]
fn too_long_beats_not_utf8() {
    check!("six invalid bytes, max 3", all(b"\xff\xff\xff\xff\xff\xff\n", 3, 2), vec![Err(LineError::TooLong { line: 1 })]);
}

#[test]
fn huge_limit_does_not_overflow_or_preallocate() {
    check!("max usize::MAX", all(b"a\n", usize::MAX, 8), vec![ok("a")]);
}

#[test]
fn a_four_megabyte_line_stays_in_budget() {
    let mut input = vec![b'x'; 4 << 20];
    input.extend_from_slice(b"\nafter\n");
    let mut reader = BufReader::with_capacity(8192, &input[..]);
    reader.fill_buf().unwrap();
    let mut lines = BoundedLines::new(reader, 100);
    let (got, used) = anneal_prelude::allocs(|| (lines.next(), lines.next()));
    check!("results", got, (Some(Err(LineError::TooLong { line: 1 })), Some(ok("after"))));
    check!(format!("allocated {} bytes in {} allocations, at most 2048 bytes", used.bytes, used.count), used.bytes <= 2048, true);
}

#[test]
fn random_input_matches_a_simple_model() {
    let mut rng = anneal_prelude::Rng::new(8115);
    for _ in 0..400 {
        let len = rng.below(24);
        let input: Vec<u8> = (0..len).map(|_| *rng.pick(b"ab\n\n\r\xff")).collect();
        let max_len = rng.below(5);
        let buffer = 1 + rng.below(6);
        // The model reads the whole input at once, which the real reader must not do.
        let mut pieces: Vec<&[u8]> = input.split(|&b| b == b'\n').collect();
        let unterminated = pieces.pop().filter(|p| !p.is_empty());
        let terminated = pieces.into_iter().map(|p| p.strip_suffix(b"\r").unwrap_or(p));
        let want: Vec<Result<String, LineError>> = terminated
            .chain(unterminated)
            .enumerate()
            .map(|(i, p)| {
                let line = i as u64 + 1;
                if p.len() > max_len {
                    Err(LineError::TooLong { line })
                } else {
                    String::from_utf8(p.to_vec()).map_err(|_| LineError::NotUtf8 { line })
                }
            })
            .collect();
        check!(format!("{input:?}, max {max_len}, buffer {buffer}"), all(&input, max_len, buffer), want);
    }
}
