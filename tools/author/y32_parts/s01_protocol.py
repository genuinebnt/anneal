from y32_parts.common import *

SOLUTION = solution(1)

VISIBLE_HELPERS = FRAMES + r'''
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
'''

VISIBLE = [
    VISIBLE_HELPERS,
    r'''
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
''',
]

HIDDEN = [
    VISIBLE_HELPERS,
    r'''
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
''',
]

STATEMENT = r'''
**Capstone, step 1 of 10.** Over ten steps you build a small Redis: a server that real Redis clients could talk to. It keeps five kinds of value, expires keys, runs transactions and blocking pops, publishes messages, and survives a restart. This step is its wire protocol, called RESP2.

**Frames.** Every frame starts with a type byte and has a line that ends in CRLF (`\r\n`).

| Bytes | Frame |
|---|---|
| `+text\r\n` | `Simple` (UTF-8) |
| `-text\r\n` | `Error` (UTF-8) |
| `:n\r\n` | `Int`: an optional `-`, then digits, within `i64` |
| `$n\r\n` then `n` bytes then `\r\n` | `Bulk`: any bytes |
| `$-1\r\n` | `Null` |
| `*n\r\n` then `n` frames | `Array` |
| `*-1\r\n` | `NullArray` |

**`parse_frame(buf, limits)`** reads one frame from the front of `buf`. It returns the frame and the number of bytes it takes, or `Ok(None)` while `buf` could still become a valid frame. Later bytes are left alone.

**Errors come as early as the bytes allow.** The sender is a stranger, so never wait for input that cannot help.

- `BadType(byte)`: a frame starts with a byte not in the table. Report it at that byte.
- `LineTooLong`: a line, from its type byte up to its CRLF, is longer than `max_line`. Report it once that is certain, even with no CRLF yet. A final `\r` may be the start of the CRLF.
- When a line is complete: `BadUtf8`, `BadInteger` (not a number as described, `+5` included), `BadLength` (a negative length other than -1).
- `TooLarge`: a length over `max_bulk` or `max_array`, reported when its line is complete.
- `BadTerminator`: a byte after a bulk payload is not the expected `\r` or `\n`. Report it when that byte is there.
- `TooDeep`: arrays nest more than `max_depth` deep. An outer array is depth 1, and `*-1` does not count. Report it when the line of the array that goes too deep is complete.

A declared size must not make you allocate for it, even if the limits are `usize::MAX`. Nor may it overflow your arithmetic.

**`parse_command(buf, limits)`** reads what a client sends, and returns its arguments.

- If the first byte is `*`, it is an array of bulk strings. The errors above apply, except that `max_depth` plays no part. `*0` and `*-1` give no arguments.
- An element that is not a bulk string is `ExpectedBulk(its first byte)`, reported as soon as that byte is there. A `$-1` element is `ExpectedBulk(b'$')` once its line is complete.
- Otherwise it is an **inline** command, for typing by hand: the bytes up to the first `\n`, without one `\r` before it, split on ASCII whitespace. An empty line gives no arguments.
- An inline line longer than `max_line` (without `\r\n`) is `LineTooLong`, whether its newline has arrived or not.

**`encode(frame, out)`** appends a frame's bytes. In `Simple` and `Error`, write every `\r` and `\n` as a space, so that text cannot forge a second reply. **`encode_command(args)`** is the array of bulk strings a client sends.
'''

PROBLEM = dict(
    slug="speak-the-protocol", title="Capstone 1: speak the protocol", level="medium", stage="protocol-and-data",
    tags=["RESP2", "incremental parsing", "untrusted input", "wire protocol"],
    statement=STATEMENT, starter=starter(1), solution=SOLUTION,
    visible=VISIBLE, hidden=HIDDEN,
    teaches=[
        "A parser for a stream has three answers: a value, \"need more\" and \"never\". Say \"never\" at the first byte that makes it certain.",
        "Check every declared length before waiting for it or allocating it, and bound recursion.",
        "An encoder is a security boundary too: a line end inside a line frame forges a reply.",
    ],
    follow_up="1) Your parser starts again from the first byte on every call. What does that cost for one huge array arriving in small pieces, and what state would a resumable parser keep? 2) Return bulk strings that borrow from the buffer (`Cow` or ranges). What must the caller then do before it drains the buffer? 3) RESP3 adds maps and doubles. Which parts of your code change?",
    hints=[
        ("approach", "Write one helper that finds the line at the front and enforces the line limit, and one recursive function that parses a frame at a given depth. Decide for each error the earliest point at which you can know it."),
        ("rust", "`let Some(x) = helper(buf)? else { return Ok(None) };` keeps the three outcomes apart. `Vec::with_capacity(n.min(64))` never trusts a count. `b\"\\r\\n\".starts_with(tail)` checks a terminator of 0, 1 or 2 bytes."),
        ("edge", "With no CRLF yet and a buffer that ends in `\\r`, that byte may not belong to the line. Compare lengths without adding to the declared one first."),
    ],
    notes=("One function parses a frame at a depth and returns `None` until all of it is there. Limits are checked on the line that declares a size, before anything is waited for. The command form walks its elements itself, so it can refuse a wrong element from its first byte.",
           "O(bytes) per call", "O(frame)"),
    wrong={
        "waits_before_checking_the_bulk_limit": broken(SOLUTION, (
            "    if n > max {\n        return Err(ProtoError::TooLarge);\n    }\n    Ok(Some(n))",
            "    let _ = max;\n    Ok(Some(n))"), (
            "            let rest = &buf[used..];\n            if rest.len() < n {\n                return Ok(None);\n            }\n",
            "            let rest = &buf[used..];\n            if rest.len() < n {\n                return Ok(None);\n            }\n            if n > limits.max_bulk {\n                return Err(ProtoError::TooLarge);\n            }\n")),
        "line_limit_only_while_unfinished": broken(SOLUTION, (
            "        Some(end) if end > limits.max_line => Err(ProtoError::LineTooLong),\n", "")),
        "plus_sign_accepted": broken(SOLUTION, (
            "    if bytes.first() == Some(&b'+') {\n        return None;\n    }\n", "")),
        "reply_text_not_cleaned": broken(SOLUTION, ("            b'\\r' | b'\\n' => b' ',\n", "")),
        "terminator_checked_only_when_both_arrived": broken(SOLUTION, (
            "            if !b\"\\r\\n\".starts_with(tail) {\n                return Err(ProtoError::BadTerminator);\n            }\n            if tail.len() < 2 {\n                return Ok(None);\n            }\n",
            "            if tail.len() < 2 {\n                return Ok(None);\n            }\n            if tail != b\"\\r\\n\" {\n                return Err(ProtoError::BadTerminator);\n            }\n")),
        "bad_command_element_waits_for_all_of_it": broken(SOLUTION, (
            "            Some(b'$') => {}\n            Some(&other) => return Err(ProtoError::ExpectedBulk(other)),",
            "            Some(b'$') => {}\n            Some(&other) if parse_at(&buf[at..], 0, limits)?.is_some() => return Err(ProtoError::ExpectedBulk(other)),\n            Some(_) => return Ok(None),")),
    },
)
