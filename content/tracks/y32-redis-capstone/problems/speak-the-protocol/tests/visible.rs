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
fn every_kind_of_frame() {
    let cases: [(&[u8], Frame); 8] = [
        (b"+OK\r\n", ok()),
        (b"-ERR no\r\n", err("ERR no")),
        (b":-42\r\n", int(-42)),
        (b"$5\r\nhe\r\no\r\n", Frame::Bulk(b"he\r\no".to_vec())),
        (b"$0\r\n\r\n", bulk("")),
        (b"$-1\r\n", Frame::Null),
        (b"*-1\r\n", Frame::NullArray),
        (b"*2\r\n:1\r\n*1\r\n$1\r\na\r\n", Frame::Array(vec![int(1), arr(&["a"])])),
    ];
    for (input, want) in cases {
        check!(show(input), frame(input), Ok(Some((want, input.len()))));
    }
}

#[test]
fn only_the_first_frame_is_taken() {
    check!("two frames", frame(b":1\r\n:2\r\n"), Ok(Some((int(1), 4))));
}

#[test]
fn an_unfinished_frame_needs_more() {
    let whole = b"*2\r\n$3\r\nGET\r\n$1\r\nk\r\n";
    for cut in 0..whole.len() {
        check!(show(&whole[..cut]), frame(&whole[..cut]), Ok(None));
        check!(show(&whole[..cut]), command(&whole[..cut]), Ok(None));
    }
}

#[test]
fn a_command_as_a_client_sends_it() {
    let bytes = encode_command(&args("SET key value"));
    check!("encode_command", bytes.clone(), b"*3\r\n$3\r\nSET\r\n$3\r\nkey\r\n$5\r\nvalue\r\n".to_vec());
    check!("parse it back", command(&bytes), Ok(Some((args("SET key value"), bytes.len()))));
}

#[test]
fn an_inline_command() {
    check!("PING", command(b"PING\r\nrest"), Ok(Some((args("PING"), 6))));
    check!("bare newline, many spaces", command(b" get \t k  \n"), Ok(Some((args("get k"), 11))));
    check!("an empty line", command(b"\r\n"), Ok(Some((vec![], 2))));
}

#[test]
fn encoding_every_kind() {
    let frame = Frame::Array(vec![ok(), err("ERR x"), int(7), bulk("ab"), Frame::Null, Frame::NullArray]);
    check!("encode", encoded(&frame), b"*6\r\n+OK\r\n-ERR x\r\n:7\r\n$2\r\nab\r\n$-1\r\n*-1\r\n".to_vec());
}

#[test]
fn a_length_over_the_limit_is_refused_at_once() {
    check!("bulk of 17 with a limit of 16", frame(b"$17\r\n"), Err(ProtoError::TooLarge));
    check!("array of 5 with a limit of 4", frame(b"*5\r\n"), Err(ProtoError::TooLarge));
}
