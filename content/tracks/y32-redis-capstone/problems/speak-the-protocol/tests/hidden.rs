use solution::*;

#[allow(unused_imports)]
use anneal_prelude::Rng;


#[allow(dead_code)]
fn args(line: &str) -> Vec<Vec<u8>> {
    line.split(' ').map(|w| w.as_bytes().to_vec()).collect()
}
#[allow(dead_code)]
fn bulk(s: &str) -> Frame {
    Frame::Bulk(s.as_bytes().to_vec())
}
#[allow(dead_code)]
fn int(n: i64) -> Frame {
    Frame::Int(n)
}
#[allow(dead_code)]
fn arr(items: &[&str]) -> Frame {
    Frame::Array(items.iter().map(|s| bulk(s)).collect())
}
#[allow(dead_code)]
fn ok() -> Frame {
    Frame::Simple("OK".to_string())
}
#[allow(dead_code)]
fn simple(s: &str) -> Frame {
    Frame::Simple(s.to_string())
}
#[allow(dead_code)]
fn err(s: &str) -> Frame {
    Frame::Error(s.to_string())
}
#[allow(dead_code)]
const WRONGTYPE: &str = "WRONGTYPE Operation against a key holding the wrong kind of value";
#[allow(dead_code)]
const NOT_INT: &str = "ERR value is not an integer or out of range";
/// Every error reply must start with `prefix`.
#[allow(dead_code)]
fn is_error(frame: &Frame, prefix: &str) -> bool {
    matches!(frame, Frame::Error(text) if text.starts_with(prefix))
}

#[allow(dead_code)]
fn lim() -> Limits {
    Limits { max_line: 32, max_bulk: 16, max_array: 4, max_depth: 3 }
}
#[allow(dead_code)]
fn frame(input: &[u8]) -> Result<Option<(Frame, usize)>, ProtoError> {
    parse_frame(input, &lim())
}
#[allow(dead_code)]
fn command(input: &[u8]) -> Result<Option<(Vec<Vec<u8>>, usize)>, ProtoError> {
    parse_command(input, &lim())
}
#[allow(dead_code)]
fn encoded(frame: &Frame) -> Vec<u8> {
    let mut out = Vec::new();
    encode(frame, &mut out);
    out
}
#[allow(dead_code)]
fn show(bytes: &[u8]) -> String {
    format!("{:?}", String::from_utf8_lossy(bytes))
}
/// A table of inputs that must be refused.
#[allow(dead_code)]
fn refused(parse: fn(&[u8]) -> Result<bool, ProtoError>, cases: &[(&[u8], ProtoError)]) {
    for (input, error) in cases {
        check!(show(input), parse(input), Err(*error));
    }
}
#[allow(dead_code)]
fn as_frame(input: &[u8]) -> Result<bool, ProtoError> {
    frame(input).map(|r| r.is_some())
}
#[allow(dead_code)]
fn as_command(input: &[u8]) -> Result<bool, ProtoError> {
    command(input).map(|r| r.is_some())
}

#[test]
fn bad_frames() {
    refused(as_frame, &[
        (b"?", ProtoError::BadType(b'?')),
        (b"x\r\n", ProtoError::BadType(b'x')),
        (b":\r\n", ProtoError::BadInteger),
        (b":+5\r\n", ProtoError::BadInteger),
        (b": 5\r\n", ProtoError::BadInteger),
        (b":9223372036854775808\r\n", ProtoError::BadInteger),
        (b"$x\r\n", ProtoError::BadInteger),
        (b"$-2\r\n", ProtoError::BadLength),
        (b"*-2\r\n", ProtoError::BadLength),
        (b"+\xff\r\n", ProtoError::BadUtf8),
        (b"-\xff\r\n", ProtoError::BadUtf8),
        (b"*2\r\n:1\r\n?", ProtoError::BadType(b'?')),
    ]);
}

#[test]
fn a_wrong_byte_after_a_payload_is_refused_when_it_arrives() {
    check!("X where CR belongs", frame(b"$1\r\naX"), Err(ProtoError::BadTerminator));
    check!("X where LF belongs", frame(b"$1\r\na\rX"), Err(ProtoError::BadTerminator));
    check!("only the CR so far", frame(b"$1\r\na\r"), Ok(None));
    check!("payload longer than declared", frame(b"$1\r\nab\r\n"), Err(ProtoError::BadTerminator));
}

#[test]
fn the_line_limit_is_exact_and_early() {
    let line = |n: usize| format!("+{}\r\n", "a".repeat(n - 1)).into_bytes();
    check!("32 bytes before CRLF", frame(&line(32)).map(|r| r.map(|(_, used)| used)), Ok(Some(34)));
    check!("33 bytes before CRLF", frame(&line(33)), Err(ProtoError::LineTooLong));
    check!("33 bytes with no end yet", frame(&line(33)[..33]), Err(ProtoError::LineTooLong));
    check!("32 bytes and no end yet", frame(&line(32)[..32]), Ok(None));
    check!("32 bytes and the CR", frame(&line(32)[..33]), Ok(None));
    check!("an endless length line", frame(format!("${}", "1".repeat(40)).as_bytes()), Err(ProtoError::LineTooLong));
}

#[test]
fn the_inline_limit_does_not_depend_on_how_bytes_arrive() {
    let word = "a".repeat(32);
    check!("32 bytes", command(format!("{word}\r\n").as_bytes()), Ok(Some((args(&word), 34))));
    check!("32 bytes and a CR", command(format!("{word}\r").as_bytes()), Ok(None));
    check!("33 bytes with its newline", command(format!("{word}b\r\n").as_bytes()), Err(ProtoError::LineTooLong));
    check!("33 bytes with a bare newline", command(format!("{word}b\n").as_bytes()), Err(ProtoError::LineTooLong));
    check!("33 bytes with no newline", command(format!("{word}b").as_bytes()), Err(ProtoError::LineTooLong));
}

#[test]
fn nesting_has_a_limit() {
    check!("three deep", frame(b"*1\r\n*1\r\n*0\r\n").map(|r| r.is_some()), Ok(true));
    check!("four deep", frame(b"*1\r\n*1\r\n*1\r\n*0\r\n"), Err(ProtoError::TooDeep));
    check!("a null array does not nest", frame(b"*1\r\n*1\r\n*1\r\n*-1\r\n").map(|r| r.is_some()), Ok(true));
    let no_arrays = Limits { max_depth: 0, ..lim() };
    check!("no arrays at all", parse_frame(b"*0\r\n", &no_arrays), Err(ProtoError::TooDeep));
    let deep = "*1\r\n".repeat(200_000);
    check!("200000 deep must not overflow the stack", frame(deep.as_bytes()), Err(ProtoError::TooDeep));
}

#[test]
fn a_declared_size_allocates_nothing() {
    let huge = Limits { max_line: 64, max_bulk: usize::MAX, max_array: usize::MAX, max_depth: 3 };
    check!("a huge array", parse_frame(b"*999999999999\r\n", &huge), Ok(None));
    check!("a huge bulk", parse_frame(b"$9223372036854775807\r\nabc", &huge), Ok(None));
    check!("a huge command", parse_command(b"*999999999999\r\n$1\r\na\r\n", &huge), Ok(None));
    check!("a huge argument", parse_command(b"*1\r\n$9223372036854775807\r\n", &huge), Ok(None));
}

#[test]
fn commands_must_be_arrays_of_bulk_strings() {
    refused(as_command, &[
        (b"*1\r\n:1\r\n", ProtoError::ExpectedBulk(b':')),
        (b"*1\r\n:", ProtoError::ExpectedBulk(b':')),
        (b"*2\r\n$1\r\na\r\n+", ProtoError::ExpectedBulk(b'+')),
        (b"*1\r\n*", ProtoError::ExpectedBulk(b'*')),
        (b"*1\r\n-", ProtoError::ExpectedBulk(b'-')),
        (b"*1\r\nP", ProtoError::ExpectedBulk(b'P')),
        (b"*1\r\n$-1\r\n", ProtoError::ExpectedBulk(b'$')),
        (b"*5\r\n", ProtoError::TooLarge),
        (b"*1\r\n$17\r\n", ProtoError::TooLarge),
        (b"*-3\r\n", ProtoError::BadLength),
        (b"*z\r\n", ProtoError::BadInteger),
        (b"*1\r\n$1\r\nab", ProtoError::BadTerminator),
    ]);
}

#[test]
fn empty_commands_are_consumed() {
    check!("*0", command(b"*0\r\nPING\r\n"), Ok(Some((vec![], 4))));
    check!("*-1", command(b"*-1\r\n"), Ok(Some((vec![], 5))));
    check!("only spaces", command(b"   \n"), Ok(Some((vec![], 4))));
}

#[test]
fn a_line_end_cannot_be_smuggled_into_a_reply() {
    check!("simple", encoded(&simple("a\r\n+forged")), b"+a  +forged\r\n".to_vec());
    check!("error", encoded(&err("ERR a\nb\rc")), b"-ERR a b c\r\n".to_vec());
    check!("bulk bytes stay as they are", encoded(&bulk("a\r\nb")), b"$4\r\na\r\nb\r\n".to_vec());
}

#[allow(dead_code)]
fn random_frame(rng: &mut Rng, depth: usize) -> Frame {
    let len = rng.below(10);
    let text = rng.string(len, "ab 1é");
    let bytes: Vec<u8> = rng.vec::<i64>(len, 0, 255).into_iter().map(|b| b as u8).collect();
    match rng.below(if depth < 3 { 8 } else { 6 }) {
        0 => Frame::Simple(text),
        1 => Frame::Error(text),
        2 => Frame::Int(*rng.pick(&[i64::MIN, -1, 0, 7, i64::MAX])),
        3 => Frame::Bulk(bytes),
        4 => Frame::Null,
        5 => Frame::NullArray,
        _ => {
            let n = rng.below(5);
            Frame::Array((0..n).map(|_| random_frame(rng, depth + 1)).collect())
        }
    }
}

#[test]
fn random_frames_round_trip_and_every_prefix_needs_more() {
    let mut rng = Rng::new(0x3201);
    for _ in 0..400 {
        let want = random_frame(&mut rng, 0);
        let bytes = encoded(&want);
        check!(format!("{want:?}"), frame(&bytes), Ok(Some((want.clone(), bytes.len()))));
        for cut in 0..bytes.len() {
            check!(format!("the first {cut} bytes of {want:?}"), frame(&bytes[..cut]), Ok(None));
        }
    }
}

#[test]
fn commands_cut_anywhere_come_out_the_same() {
    let mut rng = Rng::new(0x3202);
    for _ in 0..200 {
        let mut sent = Vec::new();
        let mut stream = Vec::new();
        for _ in 0..rng.below(6) + 1 {
            let n = rng.below(4) + 1;
            let command: Vec<Vec<u8>> = (0..n)
                .map(|_| {
                    let len = rng.below(8);
                    rng.vec::<i64>(len, 0, 255).into_iter().map(|b| b as u8).collect()
                })
                .collect();
            stream.extend(encode_command(&command));
            sent.push(command);
        }
        let (mut buffer, mut got, mut fed) = (Vec::new(), Vec::new(), 0);
        while fed < stream.len() {
            let n = (rng.below(9) + 1).min(stream.len() - fed);
            buffer.extend_from_slice(&stream[fed..fed + n]);
            fed += n;
            loop {
                match command(&buffer) {
                    Ok(Some((one, used))) => {
                        buffer.drain(..used);
                        got.push(one);
                    }
                    Ok(None) => break,
                    Err(e) => panic!("{e:?} for {}", show(&buffer)),
                }
            }
        }
        check!(show(&stream), got, sent);
    }
}
